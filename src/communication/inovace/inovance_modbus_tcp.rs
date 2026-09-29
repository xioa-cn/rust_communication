use super::{InovanceClient, InovanceType};
use crate::communication::modbus::{ModbusTcp, TcpTransport};
use crate::communication::timeout::Timeout;
use std::net::IpAddr;

/// 汇川 Modbus TCP；端口通常为 502，必须在 PLC 侧启用对应服务。
pub type InovanceModbusTcp = InovanceClient<TcpTransport>;

impl InovanceClient<TcpTransport> {
    /// 与其他网络客户端一致显式传入 IP/端口/超时；series 决定软元件映射。
    /// 构造不连接，调用 connect 后才会创建 TCP socket。
    pub fn new(address: IpAddr, port: u16, series: InovanceType, timeout: Timeout) -> Self {
        Self::from_transport(
            ModbusTcp::new(address, port, timeout).into_transport(),
            series,
        )
    }
}
