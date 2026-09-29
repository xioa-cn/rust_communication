use super::omron_address::Address;
use super::omron_packet::{MAX_ITEMS, end_code_message, memory_request, validate_response};
use super::{
    ByteOrder, FinsRoute, FinsTransport, OmronReadValue, OmronValue, OmronWriteInput,
    OmronWriteValue,
};
use crate::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use crate::communication::timeout::as_operator;
use crate::entity::operate::Operator;

pub struct OmronClient<Transport: FinsTransport> {
    transport: Transport,
    configured_route: FinsRoute,
    active_route: Option<FinsRoute>,
    byte_order: ByteOrder,
    sid: u8,
}

impl<Transport: FinsTransport> OmronClient<Transport> {
    pub fn from_transport(transport: Transport) -> Self {
        Self {
            transport,
            configured_route: FinsRoute::default(),
            active_route: None,
            byte_order: ByteOrder::default(),
            sid: 0,
        }
    }

    pub fn route(&self) -> FinsRoute {
        self.active_route.unwrap_or(self.configured_route)
    }

    pub fn set_route(&mut self, route: FinsRoute) -> Operator<bool> {
        if self.is_connected() {
            return Operator::err("Disconnect Omron.cs before changing the FINS route");
        }
        if let Err(error) = route.validate(false) {
            return Operator::err(&error);
        }
        self.configured_route = route;
        Operator::ok(true)
    }

    pub fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }

    pub fn set_byte_order(&mut self, byte_order: ByteOrder) {
        self.byte_order = byte_order;
    }

    pub fn connect(&mut self) -> Operator<bool> {
        if self.is_connected() {
            return Operator::ok(true);
        }
        let result = (|| {
            self.configured_route.validate(false)?;
            let route = self.transport.connect(self.configured_route)?;
            route.validate(true)?;
            if !self.transport.is_connected() {
                return Err("Omron.cs transport did not establish a connection".into());
            }
            self.active_route = Some(route);
            Ok(true)
        })();
        if result.is_err() {
            self.disconnect();
        }
        as_operator(result)
    }

    pub fn disconnect(&mut self) -> Operator<bool> {
        self.transport.disconnect();
        self.active_route = None;
        Operator::ok(true)
    }

    pub fn is_connected(&self) -> bool {
        self.active_route.is_some() && self.transport.is_connected()
    }

    pub fn read<Value: OmronReadValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Operator<Box<[Value]>> {
        as_operator(self.read_inner(address, length))
    }

    pub fn write<Value: OmronWriteValue>(
        &mut self,
        address: &str,
        value: impl OmronWriteInput<Value>,
    ) -> Operator<Value> {
        let value = value.into_value();
        as_operator(self.write_inner(address, &value).map(|()| value))
    }

    pub fn write_all<Value: OmronValue>(
        &mut self,
        address: &str,
        values: &[Value],
    ) -> Operator<usize> {
        as_operator(self.write_all_inner(address, values))
    }

    pub fn read_string(&mut self, address: &str, byte_length: usize) -> Operator<String> {
        as_operator(
            self.read_inner::<String>(address, byte_length)
                .map(|values| values.into_vec().remove(0)),
        )
    }

    pub fn write_string(&mut self, address: &str, value: &str) -> Operator<String> {
        self.write::<String>(address, value)
    }

    fn read_inner<Value: OmronReadValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Result<Box<[Value]>, String> {
        let bytes_length = value_length(Value::BYTE_LEN, Value::IS_BIT, Value::IS_STRING, length)?;
        let address = Address::parse(address, Value::IS_BIT)?;
        let units = if Value::IS_BIT {
            bytes_length
        } else {
            bytes_length.div_ceil(2)
        };
        let mut bytes = self.read_units(address, units)?;
        if !Value::IS_BIT {
            if Value::IS_STRING || Value::BYTE_LEN == 1 {
                self.byte_order.apply_byte_stream(&mut bytes);
            } else {
                for value in bytes.chunks_mut(Value::BYTE_LEN) {
                    self.byte_order.apply(value);
                }
            }
        }
        bytes.truncate(bytes_length);
        if Value::IS_STRING {
            return Ok(vec![Value::from_be_bytes(&bytes)?].into_boxed_slice());
        }
        bytes
            .chunks(Value::BYTE_LEN)
            .map(Value::from_be_bytes)
            .collect::<Result<Vec<_>, _>>()
            .map(Vec::into_boxed_slice)
    }

    fn write_inner<Value: OmronWriteValue>(
        &mut self,
        address: &str,
        value: &Value,
    ) -> Result<(), String> {
        let expected = value_length(Value::BYTE_LEN, Value::IS_BIT, Value::IS_STRING, 1)?;
        let bytes = value.encode();
        if !Value::IS_STRING && bytes.len() != expected {
            return Err("Omron.cs value encoding length mismatch".into());
        }
        let address = Address::parse(address, Value::IS_BIT)?;
        self.write_bytes(address, bytes, Value::BYTE_LEN)
    }

    fn write_all_inner<Value: OmronValue>(
        &mut self,
        address: &str,
        values: &[Value],
    ) -> Result<usize, String> {
        let length = value_length(Value::BYTE_LEN, Value::IS_BIT, false, values.len())?;
        let address = Address::parse(address, Value::IS_BIT)?;
        address.validate_length(if Value::IS_BIT {
            length
        } else {
            length.div_ceil(2)
        })?;
        let mut bytes = Vec::with_capacity(length);
        for value in values {
            let encoded = value.to_be_bytes();
            if encoded.len() != Value::BYTE_LEN {
                return Err("Omron.cs array element encoding length mismatch; no data written".into());
            }
            bytes.extend_from_slice(&encoded);
        }
        self.write_bytes(address, bytes, Value::BYTE_LEN)?;
        Ok(values.len())
    }

    fn write_bytes(
        &mut self,
        address: Address,
        mut bytes: Vec<u8>,
        width: usize,
    ) -> Result<(), String> {
        let units = if address.is_bit {
            bytes.len()
        } else {
            bytes.len().div_ceil(2)
        };
        address.validate_length(units)?;
        if address.is_bit {
            if bytes.iter().any(|byte| *byte > 1) {
                return Err("Omron.cs bit encoding must be zero or one; no data written".into());
            }
        } else if width <= 1 {
            if !bytes.len().is_multiple_of(2) {
                let mut tail = self
                    .read_units(address.advance(units - 1), 1)
                    .map_err(|error| {
                        format!("Omron.cs tail read failed; no bytes written: {error}")
                    })?;
                self.byte_order.apply_byte_stream(&mut tail);
                bytes.push(tail[1]);
            }
            self.byte_order.apply_byte_stream(&mut bytes);
        } else {
            for value in bytes.chunks_exact_mut(width) {
                self.byte_order.apply(value);
            }
        }
        let unit_width = if address.is_bit { 1 } else { 2 };
        let mut offset = 0;
        while offset < units {
            let count = (units - offset).min(MAX_ITEMS);
            let data = &bytes[offset * unit_width..(offset + count) * unit_width];
            self.exchange(address.advance(offset), count, Some(data))
                .map_err(|error| format!("{error}; {offset} earlier {} confirmed, current block not confirmed; not retried", if address.is_bit { "bits" } else { "words" }))?;
            offset += count;
        }
        Ok(())
    }

    fn read_units(&mut self, address: Address, units: usize) -> Result<Vec<u8>, String> {
        address.validate_length(units)?;
        if !self.is_connected() {
            return Err("Omron.cs is not connected; call connect first".into());
        }
        let mut bytes = Vec::with_capacity(units * if address.is_bit { 1 } else { 2 });
        let mut offset = 0;
        while offset < units {
            let count = (units - offset).min(MAX_ITEMS);
            bytes.extend_from_slice(&self.exchange(address.advance(offset), count, None)?);
            offset += count;
        }
        Ok(bytes)
    }

    fn exchange(
        &mut self,
        address: Address,
        count: usize,
        data: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        if !self.is_connected() {
            return Err("Omron.cs is not connected; call connect first".into());
        }
        self.sid = self.sid.wrapping_add(1);
        let request = memory_request(self.route(), self.sid, address, count, data);
        let reply = match self.transport.exchange(&request) {
            Ok(reply) => reply,
            Err(error) => return self.invalid_reply(&error),
        };
        let end_code = match validate_response(&request, &reply) {
            Ok(code) => code,
            Err(error) => return self.invalid_reply(&error),
        };
        if end_code != 0 {
            return Err(end_code_message(end_code));
        }
        let expected = if data.is_some() {
            0
        } else {
            count * if address.is_bit { 1 } else { 2 }
        };
        if reply.len() != expected + 14 {
            return self.invalid_reply("FINS response payload length mismatch");
        }
        if address.is_bit && reply[14..].iter().any(|byte| *byte > 1) {
            return self.invalid_reply("FINS response contains an invalid bit value");
        }
        Ok(reply[14..].to_vec())
    }

    fn invalid_reply<Value>(&mut self, error: &str) -> Result<Value, String> {
        self.disconnect();
        Err(error.to_owned())
    }
}

fn value_length(
    width: usize,
    is_bit: bool,
    is_string: bool,
    count: usize,
) -> Result<usize, String> {
    if count == 0 {
        return Err("Omron.cs reads and writes require at least one value/byte".into());
    }
    if is_string {
        if width != 0 || is_bit {
            return Err("Omron.cs string types require zero fixed width and word access".into());
        }
        return Ok(count);
    }
    if (is_bit && width != 1) || !matches!(width, 1 | 2 | 4 | 8) {
        return Err("Omron.cs values must have 1/2/4/8 bytes, or one byte for a bit".into());
    }
    count
        .checked_mul(width)
        .ok_or_else(|| "Omron.cs value length overflow".into())
}

impl<Transport: FinsTransport> DeviceBase for OmronClient<Transport> {
    fn connect(&mut self) -> Operator<bool> {
        OmronClient::connect(self)
    }
    fn disconnect(&mut self) -> Operator<bool> {
        OmronClient::disconnect(self)
    }
}

impl<Transport: FinsTransport, Value: OmronReadValue> ReadBase<Value> for OmronClient<Transport> {
    fn read(&mut self, address: &str, length: usize) -> Operator<Box<[Value]>> {
        OmronClient::read(self, address, length)
    }
}

impl<Transport: FinsTransport, Value: OmronValue> WriteBase<Value> for OmronClient<Transport> {
    fn write(&mut self, address: &str, value: Value) -> Operator<Value> {
        OmronClient::write(self, address, value)
    }
    fn write_all(&mut self, address: &str, values: &[Value]) -> Operator<usize> {
        OmronClient::write_all(self, address, values)
    }
}
