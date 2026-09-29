//! 汇川同步 Modbus TCP 客户端，沿用 `Operator` 和通用设备读写 trait。
//!
//! 构造时显式选择系列，不建立网络连接；默认站号 1、数值字序 CDAB。
//! `read::<T>` 的 length 是元素数；u8/i8 和 String 的长度为字节数。
//! `read::<String>` 返回一个字符串；不解析 S7 字符串头，不裁剪零字节。
//! 地址支持 `x=2;D100`，也接受 `s=2;D100`；仅改变本次站号。
//! 写单值返回原值，写数组返回元素个数；不自动重连、重试写入或回滚。
//!
//! # 地址与边界
//! - H3U：M0..7679、M8000..8511、SM0..1023、S0..4095、X/Y0..377（八进制）；
//!   D0..8511、SD0..1023、R0..32767、T0..511、C0..255。
//!   T/C 的 bool 地址表示触点，数值地址表示当前值；C200..255 必须使用 32 位类型。
//! - H5U/Easy：M0..7999、B0..32767、S0..4095、X/Y0..1777（八进制）；D0..7999、R0..32767。
//!   X/Y 的 `X10` 与 `X1.0` 等价，均表示十进制偏移 8。X 为只读输入。
//! - AM/AC/AP 共用一套通讯与地址映射，分别使用枚举 `AM`、`AC`、`AP` 选择。
//!   I/IX 输入、Q/QX 输出、MW 字、MD 双字、MB 字节、MX 位；可选前缀 `%`。
//!   `MD100 = MW200`；`MB1` 是 MW0 高字节，`MX1.0` 是 MW0 第 8 位。
//!   SM、SD/SDW 使用 AM600 扩展功能码；逻辑偏移为 0..65535，实际可用范围以 CPU 为准。
//!   I/IX 使用只读功能码 02，不降级为 Q；是否对应物理输入由 PLC 的 Modbus 映射决定。
//! - EVO 使用独立枚举 `EVO`：支持 Q/QX、I/IX、MW、MD、MB、MX，I/IX 为只读输入。
//!   EVO 不套用 AM600 的 SM/SD 扩展功能码；MB 支持从奇数字节地址开始读写。
//! - 支持 `D100.3`、`MW100.15` 等寄存器位访问；位号按最低有效位为 0。
//!   寄存器位写入及不满字的字节写入先读后写保留邻位/邻字节，**并非原子操作**。
//! - MW/D 等地址的原始字节按寄存器顺序排列；BADC/DCBA 交换字内字节，CDAB 不倒转整个缓冲区。
//!   MB 的字节/字符串流按 PLC 内存低字节在先，独立于数值字序；
//!   MB 多字节数值要求偶数起始地址，并按配置的数值字序转换。
//! - 先校验整段地址，禁止跨软元件区或 H3U M 区间隙；分包和读改写都不是 PLC 快照/事务。
//!   实际可访问范围、系统区写权限及 AM 扩展支持仍取决于 CPU/固件配置。
//! - 仅实现内置 Modbus TCP，不包含 EtherNet/IP、EasyNet 或 PLC 启停控制。
//!
//! ```no_run
//! use rs_appliaction::communication::inovace::{InovanceModbusTcp, InovanceType};
//! use rs_appliaction::communication::timeout::Timeout;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut plc = InovanceModbusTcp::new(
//!     "192.168.0.10".parse()?, 502, InovanceType::H5U, Timeout::default());
//! plc.set_unit_id(1).to_result()?;
//! plc.connect().to_result()?;
//! let words = plc.read::<u16>("D100", 10).to_result()?;
//! let inputs = plc.read::<bool>("X10", 8).to_result()?;
//! let value = plc.read::<f32>("D200", 1).to_result()?;
//! // 仅在确认目标 PLC 和写入地址安全后，才执行以下写入。
//! plc.write::<u16>("D100", 123).to_result()?;
//! plc.write_all("D110", &[1_u16, 2, 3]).to_result()?;
//! plc.write::<String>("D300", "hello").to_result()?;
//! let text = plc.read_string("D300", 5).to_result()?;
//! plc.disconnect().to_result()?;
//! # Ok(())
//! # }
//! ```
//!
//! ```no_run
//! use rs_appliaction::communication::inovace::{InovanceModbusTcp, InovanceType};
//! use rs_appliaction::communication::timeout::Timeout;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // AM、AC、AP 仅切换枚举，连接和读写方法完全相同；EVO 也沿用同一套 API。
//! let mut plc = InovanceModbusTcp::new(
//!     "192.168.0.10".parse()?, 502, InovanceType::EVO, Timeout::default());
//! plc.connect().to_result()?;
//! let inputs = plc.read::<bool>("IX0.0", 8).to_result()?;
//! let words = plc.read::<u16>("MW100", 10).to_result()?;
//! let bytes = plc.read::<u8>("MB1", 3).to_result()?;
//! plc.disconnect().to_result()?;
//! # Ok(())
//! # }
//! ```

mod inovance_address;
mod inovance_client;
pub mod inovance_modbus_tcp;
mod inovance_transport;
pub mod inovance_type;
mod inovance_value;

pub use crate::communication::modbus::ByteOrder;
pub use inovance_client::InovanceClient;
pub use inovance_modbus_tcp::InovanceModbusTcp;
pub use inovance_type::InovanceType;
pub use inovance_value::{
    InovanceReadValue, InovanceValue, InovanceWriteInput, InovanceWriteValue,
};
