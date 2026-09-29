//! 三菱 MELSEC 同步通讯；基于标准库，不依赖第三方 PLC 库。
//!
//! 七种客户端提供一致的 read、write、write_all 和 Operator 返回值。
//! MC/MC UDP 为 3E 二进制或 ASCII，A1E 为兼容 1E TCP，MC R 为 iQ-R 设备指令。
//! D100 是字地址：u32 连读时依次使用 D100/D102；bool 的 M100 是位地址。
//! u8/i8 按连续字节打包，每个字低字节在前；奇数字节写入会读改写末字以保留邻接字节。
//! String 采用无头 UTF-8，不等同于 S7 带头 STRING；PLC 项目需预留足够容量。
//! 地址以 X/Y/B/W/SB/SW/DX/DY/ZR 开头时用十六进制，其他区域用十进制。
//! 不支持标签、扩展模块缓冲区、寄存器点位后缀或串口协议；实际设备范围由 PLC 决定。
//! 多块读写、末字读改写均不保证原子性，写入超时绝不自动重试。
//!
//! 协议依据：MELSEC Communication Protocol Reference Manual SH(NA)-080008AB，
//! 第 8 章批量设备访问、第 18 章 1E 帧、第 7 章命令与子命令。
//! MC R 当前支持常规字/位设备及 RD；不支持长定时器/计数器当前值等特殊结构。
//! 空读取、空数组及空字符串写入返回错误，不隐式清零 PLC。
//!
//! ```no_run
//! use rs_appliaction::communication::melsec::MelsecMcNet;
//! use rs_appliaction::communication::timeout::Timeout;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut plc = MelsecMcNet::new("192.168.0.10".parse()?, 6000, Timeout::default());
//! plc.connect().to_result()?;
//! let words: Box<[u16]> = plc.read("D100", 3).to_result()?;
//! let flags: Box<[bool]> = plc.read("M100", 5).to_result()?;
//! // 写操作必须先确认地址和权限；本示例不会自动运行。
//! plc.write("D100", 123_u16).to_result()?;
//! plc.write_all("M100", &[true, false, true]).to_result()?;
//! plc.write::<String>("D200", "你好").to_result()?;
//! plc.disconnect().to_result()?;
//! # Ok(())
//! # }
//! ```

mod melsec_address;
pub mod melsec_net;
mod melsec_packet;
pub mod melsec_value;

pub use melsec_net::{
    MelsecA1EAsciiNet, MelsecA1ENet, MelsecMcAsciiNet, MelsecMcAsciiUdp, MelsecMcNet, MelsecMcRNet,
    MelsecMcUdp, MelsecNet,
};
pub use melsec_value::{MelsecReadValue, MelsecValue, MelsecWriteInput, MelsecWriteValue};
