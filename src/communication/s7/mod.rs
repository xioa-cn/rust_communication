//! 基于标准库 TCP Socket 的 S7 客户端，不依赖第三方 S7 通讯库。
//! 支持 S1200、S1500、S300、S400、S200、S200Smart；型号影响默认 TSAP 和 STRING 头。
//! S300/S400 的 rack/slot 使用调用方的实际组态，不自动更改。
//! S200 需以太网模块/网关支持 S7 over TCP，可用 with_tsap(local, remote) 指定组态值。
//! 不包含串口 PPI、S7 Plus、符号寻址或定时器/计数器特殊类型。
//!
//! `s7_net` 管理会话与读写流程；地址、报文、传输、数据转换分别独立实现。
//! 读取返回 Box<[T]>，数值/bool 的 length 表示元素个数；读取原始字节使用 read::<u8>。
//! write 写入单值，write_all 借用数组或切片，自动按元素类型及数量计算长度。
//! bool 地址可简写为 DB1.100 或 M100，省略位号时默认第 0 位。
//! DB1.100.3 / M100.3 表示指定第 3 位，连续读取时自动跨字节。
//! read::<String>(address, length) 按字节长度读取原始文本，返回只含一个字符串的 Box<[String]>。
//! read_string(address, byte_length) 同样读取原始文本，但直接返回 Operator<String>。
//! read_s7_string(address) 自动读取型号对应长度头及实际内容，返回 Operator<String>。
//! 字符串内容按 UTF-8 解码（兼容 ASCII）；不自动识别其他编码或 WSTRING。
//! read_s7_strings(address) 同样自动读取一个 S7 STRING，返回只含一个字符串的 Box<[String]>。
//! S200/SMART 使用一字节长度头及 V/VB/VW/VD 地址（映射 DB1）；其他系列使用两字节头。
//! write::<String>(address, "text") 写入带头的 S7 STRING，校验 UTF-8 字节数。
//! 两字节头保留 PLC 最大容量；S200/SMART 无容量字段，调用方必须确保已预留足够内存。
//! 字符串写入成功返回 String；原始无头文本可通过 write_all(address, text.as_bytes()) 写入。
//!
//! ```no_run
//! use rs_appliaction::communication::timeout::Timeout;
//! use rs_appliaction::communication::s7::s7_net::S7Net;
//! use rs_appliaction::communication::s7::s7_type::S7Type;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut plc = S7Net::new(
//!     "192.168.0.10".parse()?, 102, S7Type::S1200,
//!     Timeout::new(3_000, 5_000), 0, 0,
//! );
//! plc.connect().to_result()?;
//! let values: Box<[u16]> = plc.read::<u16>("DB1.DBW0", 3).to_result()?;
//! let bytes: Box<[u8]> = plc.read::<u8>("DB1.0", 16).to_result()?;
//! let flags: Box<[bool]> = plc.read::<bool>("DB1.100", 8).to_result()?;
//! let texts: Box<[String]> = plc.read::<String>("DB1.200", 20).to_result()?;
//! let raw_text: String = plc.read_string("DB1.200", 20).to_result()?;
//! let s7_text: String = plc.read_s7_string("DB1.300").to_result()?;
//! let s7_texts: Box<[String]> = plc.read_s7_strings("DB1.300").to_result()?;
//! // 写操作请使用经过确认的目标地址及数据。
//! plc.write("DB1.DBW0", 123_u16).to_result()?;
//! plc.write::<String>("DB1.300", "nihao").to_result()?;
//! let count = plc.write_all("DB1.DBW0", &[1_u16, 2, 3]).to_result()?;
//! plc.disconnect().to_result()?;
//! # Ok(())
//! # }
//! ```

mod s7_address;
pub mod s7_net;
mod s7_packet;
mod s7_string;
mod s7_transport;
pub mod s7_type;
pub mod s7_value;
