use super::modbus_address::Address;
use super::modbus_packet::{MAX_PDU, exception_message, range_request};
use super::{ByteOrder, ModbusTransport, ModbusValue};
use crate::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use crate::communication::timeout::as_operator;
use crate::entity::operate::Operator;

pub struct ModbusClient<Transport: ModbusTransport> {
    pub(super) transport: Transport,
    unit_id: u8,
    byte_order: ByteOrder,
}

impl<Transport: ModbusTransport> ModbusClient<Transport> {
    pub(crate) fn into_transport(self) -> Transport {
        self.transport
    }

    pub(crate) fn transport_mut(&mut self) -> &mut Transport {
        &mut self.transport
    }

    pub fn from_transport(transport: Transport, unit_id: u8) -> Self {
        Self {
            transport,
            unit_id,
            byte_order: ByteOrder::default(),
        }
    }

    pub fn unit_id(&self) -> u8 {
        self.unit_id
    }

    pub fn set_unit_id(&mut self, unit_id: u8) -> Operator<bool> {
        if let Err(error) = self.transport.validate_unit(unit_id) {
            return Operator::err(&error);
        }
        self.unit_id = unit_id;
        Operator::ok(true)
    }

    pub fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }

    pub fn set_byte_order(&mut self, byte_order: ByteOrder) {
        self.byte_order = byte_order;
    }

    pub fn connect(&mut self) -> Operator<bool> {
        let result = self
            .transport
            .validate_unit(self.unit_id)
            .and_then(|()| self.transport.connect())
            .map(|()| true);
        as_operator(result)
    }

    pub fn disconnect(&mut self) -> Operator<bool> {
        self.transport.disconnect();
        Operator::ok(true)
    }

    pub fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }

    pub fn read<Value: ModbusValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Operator<Box<[Value]>> {
        as_operator(self.read_inner(address, length))
    }

    pub fn write<Value: ModbusValue>(&mut self, address: &str, value: Value) -> Operator<Value> {
        match self.write_inner(address, std::slice::from_ref(&value), true) {
            Ok(_) => Operator::ok(value),
            Err(error) => Operator::err(&error),
        }
    }

    pub fn write_all<Value: ModbusValue>(
        &mut self,
        address: &str,
        values: &[Value],
    ) -> Operator<usize> {
        as_operator(self.write_inner(address, values, false))
    }

    pub fn read_string(&mut self, address: &str, byte_length: usize) -> Operator<String> {
        let result = self
            .read_string_bytes(address, byte_length)
            .and_then(|bytes| <String as ModbusValue>::from_be_bytes(&bytes));
        as_operator(result)
    }

    pub fn write_string(&mut self, address: &str, value: &str) -> Operator<String> {
        as_operator(
            self.write_string_bytes(address, value.as_bytes())
                .map(|()| value.to_owned()),
        )
    }

    fn read_inner<Value: ModbusValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Result<Box<[Value]>, String> {
        if Value::IS_STRING {
            validate_string_type::<Value>()?;
            let bytes = self.read_string_bytes(address, length)?;
            return Ok(vec![Value::from_be_bytes(&bytes)?].into_boxed_slice());
        }
        let address = Address::parse(address, Value::IS_BIT)?;
        let unit_id = address.unit_id.unwrap_or(self.unit_id);
        self.transport.validate_unit(unit_id)?;
        let units_per_value = value_units::<Value>()?;
        let quantity = length
            .checked_mul(units_per_value)
            .ok_or("Modbus read length overflow")?;
        address.validate_length(quantity)?;
        let maximum = if Value::IS_BIT {
            2000
        } else {
            125 / units_per_value * units_per_value
        };
        let mut bytes = Vec::with_capacity(length * Value::BYTE_LEN);
        let mut offset = 0;
        while offset < quantity {
            let count = (quantity - offset).min(maximum);
            let request = range_request(
                address.read_function(),
                address.offset + offset as u16,
                count as u16,
            );
            let reply = self.exchange(unit_id, &request)?;
            let byte_count = if Value::IS_BIT {
                count.div_ceil(8)
            } else {
                count * 2
            };
            if reply.len() != byte_count + 2 || usize::from(reply[1]) != byte_count {
                return self.invalid_reply("Modbus read response byte count mismatch");
            }
            if Value::IS_BIT {
                if count % 8 != 0 && reply[reply.len() - 1] >> (count % 8) != 0 {
                    return self.invalid_reply("Modbus read response has nonzero unused coil bits");
                }
                for index in 0..count {
                    bytes.push((reply[2 + index / 8] >> (index % 8)) & 1);
                }
            } else {
                bytes.extend_from_slice(&reply[2..]);
            }
            offset += count;
        }
        let mut values = Vec::with_capacity(length);
        for chunk in bytes.chunks_exact_mut(Value::BYTE_LEN) {
            if !Value::IS_BIT {
                self.byte_order.apply(chunk);
            }
            values.push(Value::from_be_bytes(chunk)?);
        }
        Ok(values.into_boxed_slice())
    }

    fn write_inner<Value: ModbusValue>(
        &mut self,
        address: &str,
        values: &[Value],
        single: bool,
    ) -> Result<usize, String> {
        if Value::IS_STRING {
            validate_string_type::<Value>()?;
            if !single || values.len() != 1 {
                return Err(
                    "Modbus string arrays are not supported; use write_string or write::<String>"
                        .into(),
                );
            }
            self.write_string_bytes(address, &values[0].to_be_bytes())?;
            return Ok(1);
        }
        let address = Address::parse(address, Value::IS_BIT)?;
        let unit_id = address.unit_id.unwrap_or(self.unit_id);
        self.transport.validate_unit(unit_id)?;
        address.validate_write()?;
        let units_per_value = value_units::<Value>()?;
        let quantity = values
            .len()
            .checked_mul(units_per_value)
            .ok_or("Modbus write length overflow")?;
        address.validate_length(quantity)?;
        let mut bytes = Vec::with_capacity(values.len() * Value::BYTE_LEN);
        for value in values {
            let mut encoded = value.to_be_bytes();
            if encoded.len() != Value::BYTE_LEN || (Value::IS_BIT && encoded[0] > 1) {
                return Err("Modbus value produced an invalid encoding".into());
            }
            if !Value::IS_BIT {
                self.byte_order.apply(&mut encoded);
            }
            bytes.extend_from_slice(&encoded);
        }
        if single && quantity == 1 {
            let (function, value) = if Value::IS_BIT {
                (5, if bytes[0] == 1 { 0xff00 } else { 0 })
            } else {
                (6, u16::from_be_bytes([bytes[0], bytes[1]]))
            };
            let request = range_request(function, address.offset, value);
            let reply = self.exchange(unit_id, &request)?;
            if reply != request {
                return self.invalid_reply(
                    "Modbus single write echo mismatch; write outcome is unconfirmed",
                );
            }
            return Ok(values.len());
        }
        let maximum = if Value::IS_BIT {
            1968
        } else {
            123 / units_per_value * units_per_value
        };
        let mut offset = 0;
        while offset < quantity {
            let count = (quantity - offset).min(maximum);
            let function = if Value::IS_BIT { 15 } else { 16 };
            let mut request = range_request(function, address.offset + offset as u16, count as u16);
            if Value::IS_BIT {
                let byte_count = count.div_ceil(8);
                request.push(byte_count as u8);
                request.resize(6 + byte_count, 0);
                for index in 0..count {
                    request[6 + index / 8] |= bytes[offset + index] << (index % 8);
                }
            } else {
                request.push((count * 2) as u8);
                request.extend_from_slice(&bytes[offset * 2..(offset + count) * 2]);
            }
            let reply = self.exchange(unit_id, &request).map_err(|error| {
                format!("{error}; {offset} earlier units confirmed, current write outcome may be unknown; not retried")
            })?;
            if reply != request[..5] {
                return self.invalid_reply(&format!("Modbus multiple write echo mismatch; {offset} earlier units confirmed, current write unconfirmed"));
            }
            offset += count;
        }
        Ok(values.len())
    }

    fn read_string_bytes(&mut self, address: &str, byte_length: usize) -> Result<Vec<u8>, String> {
        let words = self.read_inner::<u16>(address, byte_length.div_ceil(2))?;
        let mut bytes = Vec::with_capacity(words.len() * 2);
        for word in words {
            bytes.extend_from_slice(&word.to_be_bytes());
        }
        bytes.truncate(byte_length);
        Ok(bytes)
    }

    fn write_string_bytes(&mut self, address: &str, value: &[u8]) -> Result<(), String> {
        let start = Address::parse(address, false)?;
        let unit_id = start.unit_id.unwrap_or(self.unit_id);
        self.transport.validate_unit(unit_id)?;
        start.validate_write()?;
        let quantity = value.len().div_ceil(2);
        start.validate_length(quantity)?;
        let mut bytes = Vec::with_capacity(quantity * 2);
        bytes.extend_from_slice(value);
        if value.len() % 2 != 0 {
            let tail_address =
                format!("x={unit_id};HR{}", usize::from(start.offset) + quantity - 1);
            let tail = self.read_inner::<u16>(&tail_address, 1).map_err(|error| {
                format!("Modbus string tail read failed; no string bytes written: {error}")
            })?;
            bytes.push(tail[0].to_be_bytes()[1]);
        }
        let words: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        self.write_inner(address, &words, false)?;
        Ok(())
    }

    fn exchange(&mut self, unit_id: u8, request: &[u8]) -> Result<Vec<u8>, String> {
        self.transport.validate_unit(unit_id)?;
        if !self.is_connected() {
            return Err("Modbus is not connected; call connect first".into());
        }
        let reply = match self.transport.exchange(unit_id, request) {
            Ok(reply) => reply,
            Err(error) => {
                if matches!(request[0], 5 | 6 | 15 | 16) {
                    return self.invalid_reply(&format!(
                        "{error}; write outcome is unconfirmed; not retried"
                    ));
                }
                return self.invalid_reply(&error);
            }
        };
        if reply.is_empty() || reply.len() > MAX_PDU {
            return self.invalid_reply("Modbus response PDU length is invalid");
        }
        if reply[0] == (request[0] | 0x80) {
            if reply.len() != 2 {
                return self.invalid_reply("Modbus exception response length is invalid");
            }
            return Err(exception_message(reply[1]));
        }
        if reply[0] != request[0] {
            return self.invalid_reply("Modbus response function mismatch");
        }
        Ok(reply)
    }

    fn invalid_reply<Value>(&mut self, message: &str) -> Result<Value, String> {
        self.transport.disconnect();
        Err(message.to_owned())
    }
}

fn validate_string_type<Value: ModbusValue>() -> Result<(), String> {
    if Value::IS_BIT || Value::BYTE_LEN != 0 {
        return Err(
            "Modbus string types must have zero fixed byte length and cannot be bit values".into(),
        );
    }
    Ok(())
}

fn value_units<Value: ModbusValue>() -> Result<usize, String> {
    if Value::IS_BIT && Value::BYTE_LEN == 1 {
        return Ok(1);
    }
    if !Value::IS_BIT && matches!(Value::BYTE_LEN, 2 | 4 | 8) {
        return Ok(Value::BYTE_LEN / 2);
    }
    Err("Modbus values must be a bit or a 2/4/8-byte numeric value".into())
}

impl<Transport: ModbusTransport> DeviceBase for ModbusClient<Transport> {
    fn connect(&mut self) -> Operator<bool> {
        ModbusClient::connect(self)
    }
    fn disconnect(&mut self) -> Operator<bool> {
        ModbusClient::disconnect(self)
    }
}

impl<Transport: ModbusTransport, Value: ModbusValue> ReadBase<Value> for ModbusClient<Transport> {
    fn read(&mut self, address: &str, length: usize) -> Operator<Box<[Value]>> {
        ModbusClient::read(self, address, length)
    }
}

impl<Transport: ModbusTransport, Value: ModbusValue> WriteBase<Value> for ModbusClient<Transport> {
    fn write(&mut self, address: &str, value: Value) -> Operator<Value> {
        ModbusClient::write(self, address, value)
    }
    fn write_all(&mut self, address: &str, values: &[Value]) -> Operator<usize> {
        ModbusClient::write_all(self, address, values)
    }
}
