use super::inovance_address::{Address, Kind, validate_shape};
use super::inovance_transport::InovanceTransport;
use super::inovance_value::transform;
use super::{
    ByteOrder, InovanceReadValue, InovanceType, InovanceValue, InovanceWriteInput,
    InovanceWriteValue,
};
use crate::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use crate::communication::modbus::{ModbusClient, ModbusTransport};
use crate::communication::timeout::as_operator;
use crate::entity::operate::Operator;

// 保守兼容 H3U 的单次 255 点限制；120 字同时满足读写上限并对齐 32/64 位值。
const MAX_BITS: usize = 240;
const MAX_WORDS: usize = 120;

/// 软元件地址适配层；连接、MBAP 校验、异常响应处理复用现有 Modbus 实现。
pub struct InovanceClient<Transport: ModbusTransport> {
    core: ModbusClient<InovanceTransport<Transport>>,
    series: InovanceType,
    byte_order: ByteOrder,
}

impl<Transport: ModbusTransport> InovanceClient<Transport> {
    /// 注入已经实现 Modbus 事务的传输层，便于无 PLC 单元测试；构造不访问网络。
    pub fn from_transport(transport: Transport, series: InovanceType) -> Self {
        Self {
            core: ModbusClient::from_transport(
                InovanceTransport {
                    inner: transport,
                    extended: false,
                },
                1,
            ),
            series,
            byte_order: ByteOrder::CDAB,
        }
    }

    pub fn series(&self) -> InovanceType {
        self.series
    }

    /// 已连接时禁止改变系列，避免同一地址在不同映射下读写其他软元件。
    pub fn set_series(&mut self, series: InovanceType) -> Operator<bool> {
        if self.is_connected() {
            return Operator::err("Disconnect before changing the Inovance series");
        }
        self.series = series;
        Operator::ok(true)
    }

    pub fn unit_id(&self) -> u8 {
        self.core.unit_id()
    }
    pub fn set_unit_id(&mut self, unit: u8) -> Operator<bool> {
        self.core.set_unit_id(unit)
    }
    pub fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }
    pub fn set_byte_order(&mut self, order: ByteOrder) {
        self.byte_order = order;
    }

    /// 仅建立传输会话；不代表 PLC 点位健康检查成功，不执行隐式读写。
    pub fn connect(&mut self) -> Operator<bool> {
        self.core.connect()
    }
    pub fn disconnect(&mut self) -> Operator<bool> {
        self.core.disconnect()
    }
    pub fn is_connected(&self) -> bool {
        self.core.is_connected()
    }

    /// 连续读取 length 个元素；String 的 length 是字节数，返回一个字符串。
    pub fn read<Value: InovanceReadValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Operator<Box<[Value]>> {
        as_operator(self.read_inner(address, length))
    }

    /// 写入一个值并返回原值；String 也接受 &str 或 &String。
    pub fn write<Value: InovanceWriteValue>(
        &mut self,
        address: &str,
        value: impl InovanceWriteInput<Value>,
    ) -> Operator<Value> {
        let value = value.into_value();
        let result = (|| {
            validate_shape(Value::BYTE_LEN, Value::IS_BIT, Value::IS_STRING)?;
            let bytes = value.encode();
            validate_encoding(&bytes, Value::BYTE_LEN, Value::IS_BIT, Value::IS_STRING)?;
            self.write_encoded(
                address,
                &bytes,
                Value::BYTE_LEN,
                Value::IS_BIT,
                Value::IS_STRING,
                true,
            )
        })();
        as_operator(result.map(|()| value))
    }

    /// 写入连续数组/切片并返回元素数；不支持字符串数组。整段地址先校验，分包不是原子操作。
    pub fn write_all<Value: InovanceValue>(
        &mut self,
        address: &str,
        values: &[Value],
    ) -> Operator<usize> {
        let result = (|| {
            validate_shape(Value::BYTE_LEN, Value::IS_BIT, false)?;
            let parsed = Address::parse(address, self.series, Value::IS_BIT)?;
            parsed.validate(Value::BYTE_LEN, Value::IS_BIT, false, values.len(), true)?;
            let mut bytes = Vec::with_capacity(values.len() * Value::BYTE_LEN);
            for value in values {
                let encoded = value.to_be_bytes();
                validate_encoding(&encoded, Value::BYTE_LEN, Value::IS_BIT, false)?;
                bytes.extend_from_slice(&encoded);
            }
            self.write_encoded(
                address,
                &bytes,
                Value::BYTE_LEN,
                Value::IS_BIT,
                false,
                false,
            )?;
            Ok(values.len())
        })();
        as_operator(result)
    }

    pub fn read_string(&mut self, address: &str, byte_length: usize) -> Operator<String> {
        as_operator(
            self.read_inner::<String>(address, byte_length)
                .map(|values| values.into_vec().remove(0)),
        )
    }

    /// 原始 UTF-8，无长度头；奇数末字节先读后写保留相邻字节，空字符串返回错误。
    pub fn write_string(&mut self, address: &str, value: &str) -> Operator<String> {
        self.write::<String>(address, value)
    }

    fn prepare(&mut self, address: &Address) -> Result<(), String> {
        let unit = address.unit.unwrap_or(self.unit_id());
        self.core.transport_mut().validate_unit(unit)?;
        self.core.transport_mut().extended = address.extended;
        Ok(())
    }

    fn read_inner<Value: InovanceReadValue>(
        &mut self,
        text: &str,
        length: usize,
    ) -> Result<Box<[Value]>, String> {
        let address = Address::parse(text, self.series, Value::IS_BIT)?;
        let units = address.validate(
            Value::BYTE_LEN,
            Value::IS_BIT,
            Value::IS_STRING,
            length,
            false,
        )?;
        self.prepare(&address)?;
        let mut bytes = if Value::IS_BIT {
            self.read_bits(&address, length, units)?
                .into_iter()
                .map(u8::from)
                .collect::<Vec<_>>()
        } else {
            let words = self.read_words(&address, 0, units)?;
            let mut bytes =
                self.words_to_stream(&address, &words, Value::IS_STRING || Value::BYTE_LEN == 1);
            if let Kind::Byte(start) = address.kind {
                bytes.drain(..usize::from(start));
            }
            bytes.truncate(if Value::IS_STRING {
                length
            } else {
                length * Value::BYTE_LEN
            });
            bytes
        };
        if Value::IS_STRING {
            return Ok(vec![Value::from_be_bytes(&bytes)?].into_boxed_slice());
        }
        let mut values = Vec::with_capacity(length);
        let width = Value::BYTE_LEN;
        for chunk in bytes.chunks_exact_mut(width) {
            if !Value::IS_BIT && Value::BYTE_LEN > 1 {
                transform(self.byte_order, chunk, false);
            }
            values.push(Value::from_be_bytes(chunk)?);
        }
        Ok(values.into_boxed_slice())
    }

    fn write_encoded(
        &mut self,
        text: &str,
        encoded: &[u8],
        width: usize,
        bit: bool,
        string: bool,
        single: bool,
    ) -> Result<(), String> {
        let count = if string {
            encoded.len()
        } else {
            encoded.len() / width
        };
        let address = Address::parse(text, self.series, bit)?;
        let units = address.validate(width, bit, string, count, true)?;
        self.prepare(&address)?;
        if bit {
            let values: Vec<_> = encoded.iter().map(|&value| value != 0).collect();
            return self.write_bits(&address, &values, units, single);
        }
        if string || width == 1 {
            return self.write_bytes(&address, encoded, units, single);
        }
        let mut bytes = encoded.to_vec();
        for chunk in bytes.chunks_exact_mut(width) {
            transform(self.byte_order, chunk, false);
        }
        let words: Vec<_> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        self.write_words(&address, &words, single)
    }

    fn read_words(
        &mut self,
        address: &Address,
        offset: usize,
        count: usize,
    ) -> Result<Vec<u16>, String> {
        let mut words = Vec::with_capacity(count);
        while words.len() < count {
            let amount = (count - words.len()).min(MAX_WORDS);
            let target = address.modbus_address(offset + words.len(), self.unit_id());
            words.extend(result(self.core.read::<u16>(&target, amount))?.into_vec());
        }
        Ok(words)
    }

    fn write_words(
        &mut self,
        address: &Address,
        words: &[u16],
        single: bool,
    ) -> Result<(), String> {
        if single && words.len() == 1 {
            result(
                self.core
                    .write(&address.modbus_address(0, self.unit_id()), words[0]),
            )?;
            return Ok(());
        }
        for (index, chunk) in words.chunks(MAX_WORDS).enumerate() {
            result(self.core.write_all(
                &address.modbus_address(index * MAX_WORDS, self.unit_id()),
                chunk,
            ))
            .map_err(|error| {
                format!(
                    "{error}; {} earlier registers confirmed; not retried",
                    index * MAX_WORDS
                )
            })?;
        }
        Ok(())
    }

    fn read_bits(
        &mut self,
        address: &Address,
        count: usize,
        units: usize,
    ) -> Result<Vec<bool>, String> {
        if let Kind::WordBit(start) = address.kind {
            let words = self.read_words(address, 0, units)?;
            return Ok((0..count)
                .map(|index| {
                    let bit = usize::from(start) + index;
                    words[bit / 16] & (1 << (bit % 16)) != 0
                })
                .collect());
        }
        let mut bits = Vec::with_capacity(count);
        while bits.len() < count {
            let target = address.modbus_address(bits.len(), self.unit_id());
            bits.extend(
                result(
                    self.core
                        .read::<bool>(&target, (count - bits.len()).min(MAX_BITS)),
                )?
                .into_vec(),
            );
        }
        Ok(bits)
    }

    fn write_bits(
        &mut self,
        address: &Address,
        values: &[bool],
        units: usize,
        single: bool,
    ) -> Result<(), String> {
        if let Kind::WordBit(start) = address.kind {
            // 先读完全部受影响的字，读失败时不写；仍不能消除与 PLC 程序并发更新的竞争。
            let mut words = self.read_words(address, 0, units).map_err(|error| {
                format!("{error}; bit read-modify-write aborted before writing")
            })?;
            for (index, &value) in values.iter().enumerate() {
                let bit = usize::from(start) + index;
                let mask = 1 << (bit % 16);
                words[bit / 16] = (words[bit / 16] & !mask) | if value { mask } else { 0 };
            }
            return self.write_words(address, &words, single);
        }
        if single {
            result(
                self.core
                    .write(&address.modbus_address(0, self.unit_id()), values[0]),
            )?;
            return Ok(());
        }
        for (index, chunk) in values.chunks(MAX_BITS).enumerate() {
            result(self.core.write_all(
                &address.modbus_address(index * MAX_BITS, self.unit_id()),
                chunk,
            ))
            .map_err(|error| {
                format!(
                    "{error}; {} earlier bits confirmed; not retried",
                    index * MAX_BITS
                )
            })?;
        }
        Ok(())
    }

    fn words_to_stream(&self, address: &Address, words: &[u16], stream: bool) -> Vec<u8> {
        let mut bytes: Vec<_> = words
            .iter()
            .flat_map(|word| {
                if stream && matches!(address.kind, Kind::Byte(_)) {
                    u16::to_le_bytes(*word)
                } else {
                    u16::to_be_bytes(*word)
                }
            })
            .collect();
        if stream && !matches!(address.kind, Kind::Byte(_)) {
            transform(self.byte_order, &mut bytes, true);
        }
        bytes
    }

    fn write_bytes(
        &mut self,
        address: &Address,
        value: &[u8],
        units: usize,
        single: bool,
    ) -> Result<(), String> {
        let start = if let Kind::Byte(start) = address.kind {
            usize::from(start)
        } else {
            0
        };
        let end = start + value.len();
        let mut bytes = vec![0; units * 2];
        // 只读取未覆盖的首/尾字，两个边界都成功后才开始写入，不吞掉读失败。
        if start != 0 {
            let words = self
                .read_words(address, 0, 1)
                .map_err(|error| format!("{error}; byte head read failed; no bytes written"))?;
            bytes[..2].copy_from_slice(&self.words_to_stream(address, &words, true));
        }
        if !end.is_multiple_of(2) && !(start != 0 && units == 1) {
            let words = self
                .read_words(address, units - 1, 1)
                .map_err(|error| format!("{error}; byte tail read failed; no bytes written"))?;
            bytes[(units - 1) * 2..].copy_from_slice(&self.words_to_stream(address, &words, true));
        }
        bytes[start..end].copy_from_slice(value);
        let words: Vec<_> = if matches!(address.kind, Kind::Byte(_)) {
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect()
        } else {
            transform(self.byte_order, &mut bytes, true);
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect()
        };
        self.write_words(address, &words, single)
    }
}

fn validate_encoding(bytes: &[u8], width: usize, bit: bool, string: bool) -> Result<(), String> {
    if (!string && bytes.len() != width) || (bit && bytes.iter().any(|&byte| byte > 1)) {
        return Err("Inovance value produced an invalid encoding".into());
    }
    Ok(())
}

fn result<Value>(operator: Operator<Value>) -> Result<Value, String> {
    if operator.is_success {
        operator
            .content
            .ok_or_else(|| "Modbus returned no result".into())
    } else {
        Err(operator.msg)
    }
}

impl<Transport: ModbusTransport> DeviceBase for InovanceClient<Transport> {
    fn connect(&mut self) -> Operator<bool> {
        InovanceClient::connect(self)
    }
    fn disconnect(&mut self) -> Operator<bool> {
        InovanceClient::disconnect(self)
    }
}

impl<Transport: ModbusTransport, Value: InovanceReadValue> ReadBase<Value>
    for InovanceClient<Transport>
{
    fn read(&mut self, address: &str, length: usize) -> Operator<Box<[Value]>> {
        InovanceClient::read(self, address, length)
    }
}

impl<Transport: ModbusTransport, Value: InovanceValue> WriteBase<Value>
    for InovanceClient<Transport>
{
    fn write(&mut self, address: &str, value: Value) -> Operator<Value> {
        InovanceClient::write(self, address, value)
    }
    fn write_all(&mut self, address: &str, values: &[Value]) -> Operator<usize> {
        InovanceClient::write_all(self, address, values)
    }
}
