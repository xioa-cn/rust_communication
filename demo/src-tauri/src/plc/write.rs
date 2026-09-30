use super::PlcClient;
use super::{
    result,
    validation::{MAX_ITEMS, validate_address, validate_count},
};
use crate::models::{DataType, WriteMode, WriteRequest};
use rs_appliaction::communication::inovace::InovanceValue;
use rs_appliaction::communication::modbus::ModbusValue;
use rs_appliaction::communication::omron::OmronValue;
use rs_appliaction::communication::{melsec::MelsecValue, s7::s7_value::S7Value};
use std::{fmt::Display, str::FromStr};

/// 整个数组解析成功后才开始写入，避免后续元素格式错误造成部分修改。
pub(super) fn parse_values<T: FromStr + Display>(values: &[String]) -> Result<Vec<T>, String> {
    values
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let value = text
                .trim()
                .parse::<T>()
                .map_err(|_| format!("第 {} 项不是有效的目标类型数值：{text}", index + 1))?;
            if matches!(value.to_string().as_str(), "NaN" | "inf" | "-inf") {
                return Err(format!("第 {} 项必须是有限数值", index + 1));
            }
            Ok(value)
        })
        .collect()
}
fn write_values<T: S7Value + MelsecValue + OmronValue + InovanceValue + FromStr + Display>(
    client: &mut PlcClient,
    request: &WriteRequest,
) -> Result<usize, String> {
    let values = parse_values::<T>(&request.values)?;
    match request.mode {
        WriteMode::Single => {
            let value = values.into_iter().next().ok_or("缺少写入值")?;
            result(client.write::<T>(request.address.trim(), value))?;
            Ok(1)
        }
        WriteMode::Array => result(client.write_all::<T>(request.address.trim(), &values)),
    }
}
pub(super) fn validate_write(request: &WriteRequest) -> Result<(), String> {
    if !request.confirmed {
        return Err("写入会修改 PLC，请先明确确认本次操作".into());
    }
    validate_address(&request.address)?;
    validate_count(request.values.len())?;
    let is_string = matches!(request.data_type, DataType::RawString | DataType::S7String);
    if (request.mode == WriteMode::Single || is_string) && request.values.len() != 1 {
        return Err("单值/字符串写入只能提供一个值".into());
    }
    if is_string && request.mode != WriteMode::Single {
        return Err("本示例字符串只支持单值模式".into());
    }
    if request.values.iter().any(|value| value.len() > MAX_ITEMS) {
        return Err("单个输入值不得超过 1024 个 UTF-8 字节".into());
    }
    if request.data_type == DataType::RawString && request.values[0].is_empty() {
        return Err("原始字符串不能为空；清空 S7 STRING 请使用 S7 字符串类型".into());
    }
    Ok(())
}

/// 写入不重试；超时不表示 PLC 未执行，多包数组写入也不是事务。
pub fn write(
    client: &mut PlcClient,
    request: &WriteRequest,
) -> Result<(usize, &'static str), String> {
    validate_write(request)?;
    if let PlcClient::Cip(inner) = client {
        return super::cip::write(inner, request);
    }
    if client.protocol().is_modbus() {
        let unit = if request.data_type == DataType::RawString {
            "字节"
        } else {
            "元素"
        };
        return write_modbus(client, request).map(|count| (count, unit));
    }
    let address = request.address.trim();
    match request.data_type {
        DataType::RawString => {
            return Ok((
                result(client.write_all::<u8>(address, request.values[0].as_bytes()))?,
                "字节",
            ));
        }
        DataType::S7String => {
            result(client.write_s7_string(address, request.values[0].as_str()))?;
            return Ok((1, "字符串"));
        }
        _ => {}
    }
    let count = match request.data_type {
        DataType::Bool => write_values::<bool>(client, request),
        DataType::U8 => write_values::<u8>(client, request),
        DataType::I8 => write_values::<i8>(client, request),
        DataType::U16 => write_values::<u16>(client, request),
        DataType::I16 => write_values::<i16>(client, request),
        DataType::U32 => write_values::<u32>(client, request),
        DataType::I32 => write_values::<i32>(client, request),
        DataType::U64 => write_values::<u64>(client, request),
        DataType::I64 => write_values::<i64>(client, request),
        DataType::F32 => write_values::<f32>(client, request),
        DataType::F64 => write_values::<f64>(client, request),
        DataType::RawString | DataType::S7String => unreachable!(),
    }?;
    Ok((count, "元素"))
}

fn write_modbus_values<Value: ModbusValue + FromStr + Display>(
    client: &mut PlcClient,
    request: &WriteRequest,
) -> Result<usize, String> {
    let values = parse_values::<Value>(&request.values)?;
    match request.mode {
        WriteMode::Single => {
            let value = values.into_iter().next().ok_or("缺少写入值")?;
            result(client.write_modbus(request.address.trim(), value))?;
            Ok(1)
        }
        WriteMode::Array => result(client.write_all_modbus(request.address.trim(), &values)),
    }
}

fn write_modbus(client: &mut PlcClient, request: &WriteRequest) -> Result<usize, String> {
    match request.data_type {
        DataType::Bool => write_modbus_values::<bool>(client, request),
        DataType::U16 => write_modbus_values::<u16>(client, request),
        DataType::I16 => write_modbus_values::<i16>(client, request),
        DataType::U32 => write_modbus_values::<u32>(client, request),
        DataType::I32 => write_modbus_values::<i32>(client, request),
        DataType::U64 => write_modbus_values::<u64>(client, request),
        DataType::I64 => write_modbus_values::<i64>(client, request),
        DataType::F32 => write_modbus_values::<f32>(client, request),
        DataType::F64 => write_modbus_values::<f64>(client, request),
        DataType::RawString => {
            let value = &request.values[0];
            result(client.write_modbus(request.address.trim(), value.clone()))?;
            Ok(value.len())
        }
        _ => Err("Modbus 不支持 u8/i8 和 S7 STRING，请选择数值或原始 UTF-8 字符串".into()),
    }
}
