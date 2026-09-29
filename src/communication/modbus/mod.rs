//! 同步 Modbus 客户端；地址从零开始，不自动转换 40001 等编号。
//! TCP/UDP 使用 MBAP，RTU/ASCII 接收已经配置好超时的双向字节流。
//! 支持 x=2;100 或 x=2;HR100：仅本次读写使用站号 2，不改变默认站号。
//! String 使用原始 UTF-8；读取长度为字节数，返回单个字符串，不解析长度头或删除零字节。
//! 字符串保持寄存器的地址顺序；BADC/DCBA 交换每个寄存器的两个字节，ABCD/CDAB 不交换。
//! 奇数字节写入先读取末寄存器以保留相邻字节；读改写与分包不是原子操作，不自动重试。
//! 空字符串写入及字符串数组不支持；非 UTF-8 内容返回错误。
//!
//! ```no_run
//! use rs_appliaction::communication::modbus::{ByteOrder, ModbusTcp};
//! use rs_appliaction::communication::timeout::Timeout;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut device = ModbusTcp::new("192.168.0.10".parse()?, 502, Timeout::default());
//! device.set_unit_id(1).to_result()?;
//! device.set_byte_order(ByteOrder::ABCD);
//! device.connect().to_result()?;
//! let registers = device.read::<u16>("HR0", 3).to_result()?;
//! let coils = device.read::<bool>("C0", 8).to_result()?;
//! let text = device.read::<String>("x=2;100", 10).to_result()?;
//! let text = device.read_string("x=2;HR100", 10).to_result()?;
//! device.disconnect().to_result()?;
//! # Ok(())
//! # }
//! ```

mod modbus_address;
pub mod modbus_ascii;
mod modbus_client;
mod modbus_packet;
pub mod modbus_rtu;
pub mod modbus_tcp;
mod modbus_transport;
pub mod modbus_udp;
mod modbus_value;

pub use modbus_ascii::{AsciiTransport, ModbusAscii};
pub use modbus_client::ModbusClient;
pub use modbus_rtu::{ModbusRtu, RtuTransport};
pub use modbus_tcp::{ModbusTcp, TcpTransport};
pub use modbus_transport::ModbusTransport;
pub use modbus_udp::{ModbusUdp, UdpTransport};
pub use modbus_value::{ByteOrder, ModbusValue};
