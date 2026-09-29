//! 将界面连接配置转换为库参数；这里不建立任何实际网络连接。
use super::PlcClient;
use super::serial::SerialSession;
use crate::models::{
    ConnectRequest, CpuModel, ModbusByteOrder, ModbusOptions, OmronOptions, PlcProtocol,
};
use rs_appliaction::communication::{
    inovace::InovanceModbusTcp,
    melsec::*,
    modbus::{ByteOrder, ModbusClient, ModbusTcp, ModbusTransport, ModbusUdp},
    omron::{
        ByteOrder as OmronByteOrder, FinsRoute, FinsTransport, OmronClient, OmronFinsTcp,
        OmronFinsUdp,
    },
    s7::{s7_net::S7Net, s7_type::S7Type},
    timeout::Timeout,
};
use std::net::IpAddr;

/// 依据协议创建客户端，保持构造过程不访问网络。
pub fn create_client(request: ConnectRequest) -> Result<PlcClient, String> {
    if request.protocol.is_serial() {
        let client = SerialSession::new(
            request.protocol,
            request.serial,
            request.modbus,
            request.receive_timeout_ms,
        )?;
        return Ok(if request.protocol == PlcProtocol::ModbusRtu {
            PlcClient::ModbusRtu(client)
        } else {
            PlcClient::ModbusAscii(client)
        });
    }
    let address: IpAddr = request
        .host
        .trim()
        .parse()
        .map_err(|_| "请输入有效的 IPv4/IPv6 地址")?;
    if request.port == 0
        || (request.protocol == PlcProtocol::S7 && (request.rack > 7 || request.slot > 31))
    {
        return Err("端口必须非零，rack 范围 0..7，slot 范围 0..31".into());
    }
    if !(1..=60_000).contains(&request.connect_timeout_ms)
        || !(1..=60_000).contains(&request.receive_timeout_ms)
    {
        return Err("超时必须在 1..60000 毫秒之间".into());
    }
    if request.protocol == PlcProtocol::InovanceModbusTcp {
        let options = request.inovance;
        let mut client = InovanceModbusTcp::new(
            address,
            request.port,
            options.series.into(),
            Timeout::new(request.connect_timeout_ms, request.receive_timeout_ms),
        );
        super::result(client.set_unit_id(options.unit_id))?;
        client.set_byte_order(match options.byte_order {
            ModbusByteOrder::Abcd => ByteOrder::ABCD,
            ModbusByteOrder::Badc => ByteOrder::BADC,
            ModbusByteOrder::Cdab => ByteOrder::CDAB,
            ModbusByteOrder::Dcba => ByteOrder::DCBA,
        });
        return Ok(PlcClient::InovanceModbusTcp(client));
    }
    if request.protocol.is_omron() {
        if !address.is_ipv4()
            || address.is_unspecified()
            || address.is_multicast()
            || address == IpAddr::V4(std::net::Ipv4Addr::BROADCAST)
        {
            return Err("FINS 请输入有效的单播 IPv4 地址，不支持 IPv6 或广播".into());
        }
        let timeout = Timeout::new(request.connect_timeout_ms, request.receive_timeout_ms);
        return if request.protocol == PlcProtocol::OmronFinsTcp {
            let mut client = OmronFinsTcp::new(address, request.port, timeout);
            configure_omron(&mut client, request.omron)?;
            Ok(PlcClient::OmronFinsTcp(client))
        } else {
            let mut client = OmronFinsUdp::new(address, request.port, timeout);
            configure_omron(&mut client, request.omron)?;
            Ok(PlcClient::OmronFinsUdp(client))
        };
    }
    if request.protocol == PlcProtocol::ModbusTcp {
        let mut client = ModbusTcp::new(
            address,
            request.port,
            Timeout::new(request.connect_timeout_ms, request.receive_timeout_ms),
        );
        configure_modbus(&mut client, request.modbus)?;
        return Ok(PlcClient::ModbusTcp(client));
    }
    if request.protocol == PlcProtocol::ModbusUdp {
        let mut client = ModbusUdp::new(
            address,
            request.port,
            Timeout::new(request.connect_timeout_ms, request.receive_timeout_ms),
        );
        configure_modbus(&mut client, request.modbus)?;
        return Ok(PlcClient::ModbusUdp(client));
    }
    if request.protocol != PlcProtocol::S7 {
        let timeout = Timeout::new(request.connect_timeout_ms, request.receive_timeout_ms);
        let route = request.melsec;
        return Ok(match request.protocol {
            PlcProtocol::McBinary => PlcClient::McBinary(
                MelsecMcNet::new(address, request.port, timeout)
                    .with_route(
                        route.network_number,
                        route.pc_number,
                        route.io_number,
                        route.station_number,
                    )
                    .with_monitoring_timer(route.monitoring_timer),
            ),
            PlcProtocol::McAscii => PlcClient::McAscii(
                MelsecMcAsciiNet::new(address, request.port, timeout)
                    .with_route(
                        route.network_number,
                        route.pc_number,
                        route.io_number,
                        route.station_number,
                    )
                    .with_monitoring_timer(route.monitoring_timer),
            ),
            PlcProtocol::McUdpBinary => PlcClient::McUdpBinary(
                MelsecMcUdp::new(address, request.port, timeout)
                    .with_route(
                        route.network_number,
                        route.pc_number,
                        route.io_number,
                        route.station_number,
                    )
                    .with_monitoring_timer(route.monitoring_timer),
            ),
            PlcProtocol::McUdpAscii => PlcClient::McUdpAscii(
                MelsecMcAsciiUdp::new(address, request.port, timeout)
                    .with_route(
                        route.network_number,
                        route.pc_number,
                        route.io_number,
                        route.station_number,
                    )
                    .with_monitoring_timer(route.monitoring_timer),
            ),
            PlcProtocol::A1eBinary => PlcClient::A1eBinary(
                MelsecA1ENet::new(address, request.port, timeout)
                    .with_route(
                        route.network_number,
                        route.pc_number,
                        route.io_number,
                        route.station_number,
                    )
                    .with_monitoring_timer(route.monitoring_timer),
            ),
            PlcProtocol::A1eAscii => PlcClient::A1eAscii(
                MelsecA1EAsciiNet::new(address, request.port, timeout)
                    .with_route(
                        route.network_number,
                        route.pc_number,
                        route.io_number,
                        route.station_number,
                    )
                    .with_monitoring_timer(route.monitoring_timer),
            ),
            PlcProtocol::McRBinary => PlcClient::McRBinary(
                MelsecMcRNet::new(address, request.port, timeout)
                    .with_route(
                        route.network_number,
                        route.pc_number,
                        route.io_number,
                        route.station_number,
                    )
                    .with_monitoring_timer(route.monitoring_timer),
            ),
            _ => return Err("此分支只支持三菱协议".into()),
        });
    }
    let cpu = match request.cpu {
        CpuModel::S1200 => S7Type::S1200,
        CpuModel::S1500 => S7Type::S1500,
        CpuModel::S300 => S7Type::S300,
        CpuModel::S400 => S7Type::S400,
        CpuModel::S200 => S7Type::S200,
        CpuModel::S200Smart => S7Type::S200Smart,
    };
    let mut client = S7Net::new(
        address,
        request.port,
        cpu,
        Timeout::new(request.connect_timeout_ms, request.receive_timeout_ms),
        request.rack,
        request.slot,
    );
    let local = request
        .local_tsap
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let remote = request
        .remote_tsap
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (local, remote) {
        (None, None) => {}
        (Some(local), Some(remote)) => {
            client = client.with_tsap(parse_tsap(local)?, parse_tsap(remote)?)
        }
        _ => return Err("本地与远端 TSAP 必须同时填写，或同时留空".into()),
    }
    Ok(PlcClient::S7(client))
}

/// TSAP 统一按十六进制解析；空值交由型号默认规则处理。
fn parse_tsap(value: &str) -> Result<u16, String> {
    let digits = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    if digits.is_empty() || digits.len() > 4 {
        return Err("TSAP 必须为 1..4 位十六进制数".into());
    }
    u16::from_str_radix(digits, 16).map_err(|_| "TSAP 包含无效的十六进制字符".into())
}

pub(super) fn configure_modbus<Transport: ModbusTransport>(
    client: &mut ModbusClient<Transport>,
    options: ModbusOptions,
) -> Result<(), String> {
    super::result(client.set_unit_id(options.unit_id))?;
    client.set_byte_order(match options.byte_order {
        ModbusByteOrder::Abcd => ByteOrder::ABCD,
        ModbusByteOrder::Badc => ByteOrder::BADC,
        ModbusByteOrder::Cdab => ByteOrder::CDAB,
        ModbusByteOrder::Dcba => ByteOrder::DCBA,
    });
    Ok(())
}

fn configure_omron<Transport: FinsTransport>(
    client: &mut OmronClient<Transport>,
    options: OmronOptions,
) -> Result<(), String> {
    super::result(client.set_route(FinsRoute {
        source_node: options.source_node,
        destination_node: options.destination_node,
        source_network: options.source_network,
        destination_network: options.destination_network,
        source_unit: options.source_unit,
        destination_unit: options.destination_unit,
        gateway_count: options.gateway_count,
    }))?;
    client.set_byte_order(match options.byte_order {
        ModbusByteOrder::Abcd => OmronByteOrder::ABCD,
        ModbusByteOrder::Badc => OmronByteOrder::BADC,
        ModbusByteOrder::Cdab => OmronByteOrder::CDAB,
        ModbusByteOrder::Dcba => OmronByteOrder::DCBA,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> ConnectRequest {
        ConnectRequest {
            protocol: PlcProtocol::S7,
            melsec: Default::default(),
            modbus: Default::default(),
            omron: Default::default(),
            inovance: Default::default(),
            serial: Default::default(),
            host: "127.0.0.1".into(),
            port: 102,
            cpu: CpuModel::S1200,
            rack: 0,
            slot: 1,
            connect_timeout_ms: 5000,
            receive_timeout_ms: 5000,
            local_tsap: None,
            remote_tsap: None,
        }
    }
    #[test]
    fn creating_client_never_connects() {
        assert!(!create_client(config()).unwrap().is_connected());
    }
    #[test]
    fn validates_connection_inputs_without_network() {
        let mut request = config();
        request.slot = 32;
        assert!(create_client(request).is_err());
        let mut request = config();
        request.receive_timeout_ms = 0;
        assert!(create_client(request).is_err());
        let mut request = config();
        request.host = "not-an-ip".into();
        assert!(create_client(request).is_err());
    }
    #[test]
    fn validates_tsap_pair_and_hex() {
        assert_eq!(parse_tsap("0x0301"), Ok(0x0301));
        assert_eq!(parse_tsap("1000"), Ok(0x1000));
        assert!(parse_tsap("10000").is_err());
        assert!(parse_tsap("0xGG").is_err());
        let mut request = config();
        request.local_tsap = Some("0100".into());
        assert!(create_client(request).is_err());
    }
}
