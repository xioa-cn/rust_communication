use super::{result, validation::validate_count, write::parse_values};
use crate::models::{DataType, ReadRequest, WriteMode, WriteRequest};
use rs_appliaction::communication::ethernet::{CipClient, CipValue};
use std::{fmt::Display, str::FromStr};

macro_rules! dispatch {
    ($kind:expr, $operation:ident, $($argument:expr),+) => {
        match $kind {
            DataType::Bool => $operation::<bool>($($argument),+),
            DataType::U8 => $operation::<u8>($($argument),+),
            DataType::I8 => $operation::<i8>($($argument),+),
            DataType::U16 => $operation::<u16>($($argument),+),
            DataType::I16 => $operation::<i16>($($argument),+),
            DataType::U32 => $operation::<u32>($($argument),+),
            DataType::I32 => $operation::<i32>($($argument),+),
            DataType::U64 => $operation::<u64>($($argument),+),
            DataType::I64 => $operation::<i64>($($argument),+),
            DataType::F32 => $operation::<f32>($($argument),+),
            DataType::F64 => $operation::<f64>($($argument),+),
            _ => Err("CIP 不支持 S7 STRING、UDT 或打包 BOOL 数组".into()),
        }
    };
}

fn read_values<Value: CipValue + Display>(
    client: &mut CipClient,
    address: &str,
    length: usize,
) -> Result<Box<[String]>, String> {
    Ok(result(client.read::<Value>(address, length))?
        .iter()
        .map(ToString::to_string)
        .collect())
}

pub(super) fn read(client: &mut CipClient, request: &ReadRequest) -> Result<Box<[String]>, String> {
    let length = request.length.ok_or("CIP 必须指定读取元素数量")?;
    validate_count(length)?;
    if request.data_type == DataType::RawString {
        return result(client.read_strings(request.address.trim(), length));
    }
    dispatch!(
        request.data_type,
        read_values,
        client,
        request.address.trim(),
        length
    )
}

fn write_values<Value: CipValue + FromStr + Display>(
    client: &mut CipClient,
    request: &WriteRequest,
) -> Result<usize, String> {
    let values = parse_values::<Value>(&request.values)?;
    match request.mode {
        WriteMode::Single => {
            result(client.write(request.address.trim(), values[0]))?;
            Ok(1)
        }
        WriteMode::Array => result(client.write_all(request.address.trim(), &values)),
    }
}

pub(super) fn write(
    client: &mut CipClient,
    request: &WriteRequest,
) -> Result<(usize, &'static str), String> {
    if request.data_type == DataType::RawString {
        return Err("CIP STRING 当前仅支持读取，尚不支持写入".into());
    }
    dispatch!(request.data_type, write_values, client, request).map(|count| (count, "元素"))
}
