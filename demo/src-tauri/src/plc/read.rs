use super::PlcClient;
use super::{
    result,
    validation::{validate_address, validate_count},
};
use crate::models::{DataType, ReadRequest};
use rs_appliaction::communication::inovace::InovanceReadValue;
use rs_appliaction::communication::modbus::ModbusValue;
use rs_appliaction::communication::omron::OmronReadValue;
use rs_appliaction::communication::{melsec::MelsecReadValue, s7::s7_value::S7ReadValue};
use std::fmt::Display;

fn read_values<T: S7ReadValue + MelsecReadValue + OmronReadValue + InovanceReadValue + Display>(
    client: &mut PlcClient,
    address: &str,
    length: usize,
) -> Result<Box<[String]>, String> {
    Ok(result(client.read::<T>(address, length))?
        .iter()
        .map(ToString::to_string)
        .collect())
}

/// S7 STRING 由库自动解析长度头；原始文本才需要指定 UTF-8 字节数。
pub fn read(client: &mut PlcClient, request: &ReadRequest) -> Result<Box<[String]>, String> {
    validate_address(&request.address)?;
    let address = request.address.trim();
    if client.protocol().is_modbus() {
        return read_modbus(client, request);
    }
    if request.data_type == DataType::S7String {
        return result(client.read_s7_strings(address));
    }
    let length = request.length.ok_or("此类型必须指定读取数量")?;
    validate_count(length)?;
    match request.data_type {
        DataType::Bool => read_values::<bool>(client, address, length),
        DataType::U8 => read_values::<u8>(client, address, length),
        DataType::I8 => read_values::<i8>(client, address, length),
        DataType::U16 => read_values::<u16>(client, address, length),
        DataType::I16 => read_values::<i16>(client, address, length),
        DataType::U32 => read_values::<u32>(client, address, length),
        DataType::I32 => read_values::<i32>(client, address, length),
        DataType::U64 => read_values::<u64>(client, address, length),
        DataType::I64 => read_values::<i64>(client, address, length),
        DataType::F32 => read_values::<f32>(client, address, length),
        DataType::F64 => read_values::<f64>(client, address, length),
        DataType::RawString => result(client.read::<String>(address, length)),
        DataType::S7String => unreachable!(),
    }
}

fn read_modbus_values<Value: ModbusValue + Display>(
    client: &mut PlcClient,
    address: &str,
    length: usize,
) -> Result<Box<[String]>, String> {
    Ok(result(client.read_modbus::<Value>(address, length))?
        .iter()
        .map(ToString::to_string)
        .collect())
}

fn read_modbus(client: &mut PlcClient, request: &ReadRequest) -> Result<Box<[String]>, String> {
    if matches!(
        request.data_type,
        DataType::U8 | DataType::I8 | DataType::S7String
    ) {
        return Err("Modbus 不支持 u8/i8 和 S7 STRING，请选择数值或原始 UTF-8 字符串".into());
    }
    let address = request.address.trim();
    let length = request.length.ok_or("此类型必须指定读取数量")?;
    validate_count(length)?;
    match request.data_type {
        DataType::Bool => read_modbus_values::<bool>(client, address, length),
        DataType::U16 => read_modbus_values::<u16>(client, address, length),
        DataType::I16 => read_modbus_values::<i16>(client, address, length),
        DataType::U32 => read_modbus_values::<u32>(client, address, length),
        DataType::I32 => read_modbus_values::<i32>(client, address, length),
        DataType::U64 => read_modbus_values::<u64>(client, address, length),
        DataType::I64 => read_modbus_values::<i64>(client, address, length),
        DataType::F32 => read_modbus_values::<f32>(client, address, length),
        DataType::F64 => read_modbus_values::<f64>(client, address, length),
        DataType::RawString => result(client.read_modbus::<String>(address, length)),
        _ => Err("Modbus 数据类型不受支持".into()),
    }
}
