use serde::{Deserialize, Serialize};

/// IPC 使用固定协议标识；省略时保留原有 S7 调用行为。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlcProtocol {
    #[default]
    S7,
    McBinary,
    McAscii,
    McUdpBinary,
    McUdpAscii,
    A1eBinary,
    A1eAscii,
    McRBinary,
    ModbusTcp,
    ModbusUdp,
    ModbusRtu,
    ModbusAscii,
    OmronFinsTcp,
    OmronFinsUdp,
    InovanceModbusTcp,
    OmronCip,
    MelsecCip,
    InovanceCip,
}

impl PlcProtocol {
    pub fn is_cip(self) -> bool {
        matches!(self, Self::OmronCip | Self::MelsecCip | Self::InovanceCip)
    }
    pub fn is_omron(self) -> bool {
        matches!(self, Self::OmronFinsTcp | Self::OmronFinsUdp)
    }

    pub fn is_modbus(self) -> bool {
        matches!(
            self,
            Self::ModbusTcp | Self::ModbusUdp | Self::ModbusRtu | Self::ModbusAscii
        )
    }

    pub fn is_serial(self) -> bool {
        matches!(self, Self::ModbusRtu | Self::ModbusAscii)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ModbusByteOrder {
    #[default]
    Abcd,
    Badc,
    Cdab,
    Dcba,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CipOptions {
    pub connected: Option<bool>,
    pub connection_size: Option<u16>,
    pub route: Vec<u8>,
    pub packet_interval_us: u32,
    pub timeout_multiplier: u8,
}

impl Default for CipOptions {
    fn default() -> Self {
        Self {
            connected: None,
            connection_size: None,
            route: Vec::new(),
            packet_interval_us: 2_000_000,
            timeout_multiplier: 2,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModbusOptions {
    pub unit_id: u8,
    pub byte_order: ModbusByteOrder,
}

impl Default for ModbusOptions {
    fn default() -> Self {
        Self {
            unit_id: 1,
            byte_order: ModbusByteOrder::Abcd,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OmronOptions {
    pub source_node: u8,
    pub destination_node: u8,
    pub source_network: u8,
    pub destination_network: u8,
    pub source_unit: u8,
    pub destination_unit: u8,
    pub gateway_count: u8,
    pub byte_order: ModbusByteOrder,
}

/// 型号独立于协议：AM/AC/AP 共用根库映射，但保留用户实际选择的型号。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum InovanceSeries {
    #[default]
    AM,
    AC,
    AP,
    EVO,
    H3U,
    H5U,
    Easy,
}

impl From<InovanceSeries> for rs_appliaction::communication::inovace::InovanceType {
    fn from(series: InovanceSeries) -> Self {
        match series {
            InovanceSeries::AM => Self::AM,
            InovanceSeries::AC => Self::AC,
            InovanceSeries::AP => Self::AP,
            InovanceSeries::EVO => Self::EVO,
            InovanceSeries::H3U => Self::H3U,
            InovanceSeries::H5U => Self::H5U,
            InovanceSeries::Easy => Self::Easy,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InovanceOptions {
    pub series: InovanceSeries,
    pub unit_id: u8,
    pub byte_order: ModbusByteOrder,
}

impl Default for InovanceOptions {
    fn default() -> Self {
        Self {
            series: InovanceSeries::AM,
            unit_id: 1,
            byte_order: ModbusByteOrder::Cdab,
        }
    }
}

impl Default for OmronOptions {
    fn default() -> Self {
        Self {
            source_node: 0,
            destination_node: 0,
            source_network: 0,
            destination_network: 0,
            source_unit: 0,
            destination_unit: 0,
            gateway_count: 2,
            byte_order: ModbusByteOrder::Cdab,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialParity {
    None,
    Odd,
    #[default]
    Even,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SerialOptions {
    pub path: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: SerialParity,
    pub stop_bits: u8,
}

impl Default for SerialOptions {
    fn default() -> Self {
        Self {
            path: "COM1".into(),
            baud_rate: 9600,
            data_bits: 8,
            parity: SerialParity::Even,
            stop_bits: 1,
        }
    }
}

#[derive(Clone, Copy, Default, Deserialize)]
pub enum CpuModel {
    #[default]
    S1200,
    S1500,
    S300,
    S400,
    S200,
    S200Smart,
}

/// 三菱路由字段使用原库相同的整数宽度；非法越界输入由反序列化拒绝。
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MelsecOptions {
    pub network_number: u8,
    pub pc_number: u8,
    pub io_number: u16,
    pub station_number: u8,
    pub monitoring_timer: u16,
}

impl Default for MelsecOptions {
    fn default() -> Self {
        Self {
            network_number: 0,
            pc_number: 0xff,
            io_number: 0x03ff,
            station_number: 0,
            monitoring_timer: 16,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    #[serde(default)]
    pub protocol: PlcProtocol,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub cpu: CpuModel,
    #[serde(default)]
    pub rack: usize,
    #[serde(default)]
    pub slot: usize,
    pub connect_timeout_ms: i32,
    pub receive_timeout_ms: i32,
    pub local_tsap: Option<String>,
    pub remote_tsap: Option<String>,
    #[serde(default)]
    pub melsec: MelsecOptions,
    #[serde(default)]
    pub modbus: ModbusOptions,
    #[serde(default)]
    pub omron: OmronOptions,
    #[serde(default)]
    pub inovance: InovanceOptions,
    #[serde(default)]
    pub cip: CipOptions,
    #[serde(default)]
    pub serial: SerialOptions,
}

/// 三菱没有 S7 CPU/PDU 协商值，返回 None 而不是伪造零值。
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub connected: bool,
    pub protocol: Option<PlcProtocol>,
    pub cpu: Option<String>,
    pub pdu_length: Option<usize>,
    pub inovance_series: Option<String>,
}
