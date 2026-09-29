//! 通用文本解码与带长度头的 S7 STRING 读取；两种布局明确分开，不自动猜测。

use super::s7_address::S7Address;
use super::s7_type::S7Type;
use super::s7_value::{S7ReadValue, S7WriteValue};

impl S7ReadValue for String {
    const BYTE_LEN: usize = 0;
    const IS_STRING: bool = true;

    /// UTF-8 同时兼容 ASCII；遇到其他编码返回错误，不用替换字符掩盖乱码。
    fn from_be_bytes(bytes: &[u8]) -> Result<Self, String> {
        String::from_utf8(bytes.to_vec()).map_err(|error| {
            format!("String data is not valid UTF-8: {error}; read as u8 for other encodings")
        })
    }
}

impl S7WriteValue for String {
    const BYTE_LEN: usize = 0;
    const IS_STRING: bool = true;

    fn encode(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

/// 生成实际长度+正文；写入偏移由型号决定，两字节头必须保留最大容量。
/// 容量按编码后字节数计算；空字符串只写入实际长度 0，不清除未使用的正文。
pub(super) fn prepare_write(
    address: &S7Address,
    s7_type: S7Type,
    header_bytes: &[u8],
    content: &[u8],
) -> Result<Vec<u8>, String> {
    let header = StringHeader::parse(header_bytes, s7_type)?;
    let header_length = s7_type.string_header_length();
    // S200/SMART 的容量不在头中，只校验实际要写入的范围，不推测 PLC 变量空间。
    let reserved = if header_length == 1 {
        content.len()
    } else {
        header.capacity
    };
    address.validate(header_length + reserved, false)?;
    if content.len() > header.capacity {
        return Err(format!(
            "S7 STRING requires {} bytes but PLC capacity is {}; nothing was written",
            content.len(),
            header.capacity
        ));
    }
    let mut data = Vec::with_capacity(1 + content.len());
    data.push(content.len() as u8);
    data.extend_from_slice(content);
    Ok(data)
}

/// 型号相关的 STRING 长度头，统一暴露实际长度及协议/声明容量上限。
struct StringHeader {
    capacity: usize,
    length: usize,
}

impl StringHeader {
    /// 在读取内容前校验容量上限和实际长度，拒绝损坏或非 STRING 格式的数据。
    fn parse(bytes: &[u8], s7_type: S7Type) -> Result<Self, String> {
        let header_length = s7_type.string_header_length();
        if bytes.len() != header_length {
            return Err(format!(
                "Invalid S7 STRING header: expected {header_length} bytes"
            ));
        }
        let (capacity, length) = if header_length == 1 {
            // 单字节头只有实际长度，254 是协议上限，不是声明的内存容量。
            (254, usize::from(bytes[0]))
        } else {
            (usize::from(bytes[0]), usize::from(bytes[1]))
        };
        if capacity > 254 || length > capacity {
            return Err(format!(
                "Invalid S7 STRING header: capacity={capacity}, length={length}; \
                 expected length <= capacity <= 254"
            ));
        }
        Ok(Self { capacity, length })
    }
}

/// 自动读取一个 S7 STRING，内容长度由对应型号的一/两字节长度头决定。
pub(super) fn read_s7_value(
    address: &S7Address,
    s7_type: S7Type,
    mut read: impl FnMut(usize, usize) -> Result<Vec<u8>, String>,
) -> Result<String, String> {
    let header_length = s7_type.string_header_length();
    address.validate(header_length, false)?;
    let header = StringHeader::parse(&read(0, header_length)?, s7_type)?;
    let reserved = if header_length == 1 {
        header.length
    } else {
        header.capacity
    };
    address.validate(header_length + reserved, false)?;
    // 不读取未使用的容量，也不发送长度为零的 Read Var 请求。
    let bytes = if header.length == 0 {
        Vec::new()
    } else {
        read(header_length, header.length)?
    };
    <String as S7ReadValue>::from_be_bytes(&bytes)
}
