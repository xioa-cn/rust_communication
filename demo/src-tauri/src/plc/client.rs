//! demo 的协议适配层；只分发给根库，不在桌面示例中重复实现报文。
use super::serial::SerialSession;
use crate::models::PlcProtocol;
use rs_appliaction::{
    communication::{
        ethernet::{CipClient, CipVendor},
        inovace::{InovanceModbusTcp, InovanceReadValue, InovanceValue},
        melsec::*,
        modbus::{ModbusTcp, ModbusUdp, ModbusValue},
        omron::{OmronFinsTcp, OmronFinsUdp, OmronReadValue, OmronValue},
        s7::{
            s7_net::S7Net,
            s7_value::{S7ReadValue, S7Value},
        },
    },
    entity::operate::Operator,
};

/// 会话只能持有一种协议；Mutex 仍覆盖一次完整的读写操作。
pub enum PlcClient {
    Cip(CipClient),
    S7(S7Net),
    McBinary(MelsecMcNet),
    McAscii(MelsecMcAsciiNet),
    McUdpBinary(MelsecMcUdp),
    McUdpAscii(MelsecMcAsciiUdp),
    A1eBinary(MelsecA1ENet),
    A1eAscii(MelsecA1EAsciiNet),
    McRBinary(MelsecMcRNet),
    ModbusTcp(ModbusTcp),
    ModbusUdp(ModbusUdp),
    ModbusRtu(SerialSession),
    ModbusAscii(SerialSession),
    OmronFinsTcp(OmronFinsTcp),
    OmronFinsUdp(OmronFinsUdp),
    InovanceModbusTcp(InovanceModbusTcp),
}

impl PlcClient {
    /// 仅手动连接，不轮询或探测任意 PLC 地址。
    pub fn connect(&mut self) -> Operator<bool> {
        match self {
            Self::Cip(client) => client.connect(),
            Self::S7(client) => client.connect(),
            Self::McBinary(client) => client.connect(),
            Self::McAscii(client) => client.connect(),
            Self::McUdpBinary(client) => client.connect(),
            Self::McUdpAscii(client) => client.connect(),
            Self::A1eBinary(client) => client.connect(),
            Self::A1eAscii(client) => client.connect(),
            Self::McRBinary(client) => client.connect(),
            Self::OmronFinsTcp(client) => client.connect(),
            Self::OmronFinsUdp(client) => client.connect(),
            Self::InovanceModbusTcp(client) => client.connect(),
            Self::ModbusTcp(client) => client.connect(),
            Self::ModbusUdp(client) => client.connect(),
            Self::ModbusRtu(client) | Self::ModbusAscii(client) => client.connect(),
        }
    }
    /// 释放实际协议的本地 Socket。
    pub fn disconnect(&mut self) -> Operator<bool> {
        match self {
            Self::Cip(client) => client.disconnect(),
            Self::S7(client) => client.disconnect(),
            Self::McBinary(client) => client.disconnect(),
            Self::McAscii(client) => client.disconnect(),
            Self::McUdpBinary(client) => client.disconnect(),
            Self::McUdpAscii(client) => client.disconnect(),
            Self::A1eBinary(client) => client.disconnect(),
            Self::A1eAscii(client) => client.disconnect(),
            Self::McRBinary(client) => client.disconnect(),
            Self::OmronFinsTcp(client) => client.disconnect(),
            Self::OmronFinsUdp(client) => client.disconnect(),
            Self::InovanceModbusTcp(client) => client.disconnect(),
            Self::ModbusTcp(client) => client.disconnect(),
            Self::ModbusUdp(client) => client.disconnect(),
            Self::ModbusRtu(client) | Self::ModbusAscii(client) => client.disconnect(),
        }
    }
    /// UDP 只代表本地 Socket 已就绪，不代表远端 PLC 在线。
    pub fn is_connected(&self) -> bool {
        match self {
            Self::Cip(client) => client.is_connected(),
            Self::S7(client) => client.is_connected(),
            Self::McBinary(client) => client.is_connected(),
            Self::McAscii(client) => client.is_connected(),
            Self::McUdpBinary(client) => client.is_connected(),
            Self::McUdpAscii(client) => client.is_connected(),
            Self::A1eBinary(client) => client.is_connected(),
            Self::A1eAscii(client) => client.is_connected(),
            Self::McRBinary(client) => client.is_connected(),
            Self::OmronFinsTcp(client) => client.is_connected(),
            Self::OmronFinsUdp(client) => client.is_connected(),
            Self::InovanceModbusTcp(client) => client.is_connected(),
            Self::ModbusTcp(client) => client.is_connected(),
            Self::ModbusUdp(client) => client.is_connected(),
            Self::ModbusRtu(client) | Self::ModbusAscii(client) => client.is_connected(),
        }
    }
    /// 返回实际会话协议，不使用前端表单推测后端状态。
    pub fn protocol(&self) -> PlcProtocol {
        match self {
            Self::Cip(client) => match client.vendor() {
                CipVendor::Omron => PlcProtocol::OmronCip,
                CipVendor::Melsec => PlcProtocol::MelsecCip,
                CipVendor::Inovance => PlcProtocol::InovanceCip,
            },
            Self::S7(_) => PlcProtocol::S7,
            Self::McBinary(_) => PlcProtocol::McBinary,
            Self::McAscii(_) => PlcProtocol::McAscii,
            Self::McUdpBinary(_) => PlcProtocol::McUdpBinary,
            Self::McUdpAscii(_) => PlcProtocol::McUdpAscii,
            Self::A1eBinary(_) => PlcProtocol::A1eBinary,
            Self::A1eAscii(_) => PlcProtocol::A1eAscii,
            Self::McRBinary(_) => PlcProtocol::McRBinary,
            Self::ModbusTcp(_) => PlcProtocol::ModbusTcp,
            Self::ModbusUdp(_) => PlcProtocol::ModbusUdp,
            Self::ModbusRtu(_) => PlcProtocol::ModbusRtu,
            Self::ModbusAscii(_) => PlcProtocol::ModbusAscii,
            Self::OmronFinsTcp(_) => PlcProtocol::OmronFinsTcp,
            Self::OmronFinsUdp(_) => PlcProtocol::OmronFinsUdp,
            Self::InovanceModbusTcp(_) => PlcProtocol::InovanceModbusTcp,
        }
    }
    /// 返回实际连接的汇川型号，用于恢复左侧分类和连接参数。
    pub fn inovance_series(&self) -> Option<String> {
        match self {
            Self::InovanceModbusTcp(client) => Some(format!("{:?}", client.series())),
            _ => None,
        }
    }

    /// S7 专有的 CPU 型号；其他厂商不套用 SIMATIC 系列。
    pub fn cpu(&self) -> Option<String> {
        match self {
            Self::S7(inner) => Some(format!("{:?}", inner.s7_type())),
            _ => None,
        }
    }
    /// 只有 S7 会话具有协商 PDU，三菱返回空值。
    pub fn negotiated_pdu_length(&self) -> Option<usize> {
        match self {
            Self::S7(inner) => inner.negotiated_pdu_length(),
            _ => None,
        }
    }
    /// 数值和原始文本沿用公共泛型读取接口及 Box 数组返回值。
    pub fn read<T: S7ReadValue + MelsecReadValue + OmronReadValue + InovanceReadValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Operator<Box<[T]>> {
        match self {
            Self::S7(client) => client.read(address, length),
            Self::McBinary(client) => client.read(address, length),
            Self::McAscii(client) => client.read(address, length),
            Self::McUdpBinary(client) => client.read(address, length),
            Self::McUdpAscii(client) => client.read(address, length),
            Self::A1eBinary(client) => client.read(address, length),
            Self::A1eAscii(client) => client.read(address, length),
            Self::McRBinary(client) => client.read(address, length),
            Self::OmronFinsTcp(client) => client.read(address, length),
            Self::OmronFinsUdp(client) => client.read(address, length),
            Self::InovanceModbusTcp(client) => client.read(address, length),
            _ => Operator::err("Modbus 请选择 bool 或 16/32/64 位数值类型"),
        }
    }
    /// 单值数值写入，返回库中原始值；调用者在进入此方法前完成确认。
    pub fn write<T: S7Value + MelsecValue + OmronValue + InovanceValue>(
        &mut self,
        address: &str,
        value: T,
    ) -> Operator<T> {
        match self {
            Self::S7(client) => client.write(address, value),
            Self::McBinary(client) => client.write(address, value),
            Self::McAscii(client) => client.write(address, value),
            Self::McUdpBinary(client) => client.write(address, value),
            Self::McUdpAscii(client) => client.write(address, value),
            Self::A1eBinary(client) => client.write(address, value),
            Self::A1eAscii(client) => client.write(address, value),
            Self::McRBinary(client) => client.write(address, value),
            Self::OmronFinsTcp(client) => client.write(address, value),
            Self::OmronFinsUdp(client) => client.write(address, value),
            Self::InovanceModbusTcp(client) => client.write(address, value),
            _ => Operator::err("Modbus 请选择 bool 或 16/32/64 位数值类型"),
        }
    }
    /// 数组、原始字节和原始文本使用统一批量写入，不自动重试。
    pub fn write_all<T: S7Value + MelsecValue + OmronValue + InovanceValue>(
        &mut self,
        address: &str,
        values: &[T],
    ) -> Operator<usize> {
        match self {
            Self::S7(client) => client.write_all(address, values),
            Self::McBinary(client) => client.write_all(address, values),
            Self::McAscii(client) => client.write_all(address, values),
            Self::McUdpBinary(client) => client.write_all(address, values),
            Self::McUdpAscii(client) => client.write_all(address, values),
            Self::A1eBinary(client) => client.write_all(address, values),
            Self::A1eAscii(client) => client.write_all(address, values),
            Self::McRBinary(client) => client.write_all(address, values),
            Self::OmronFinsTcp(client) => client.write_all(address, values),
            Self::OmronFinsUdp(client) => client.write_all(address, values),
            Self::InovanceModbusTcp(client) => client.write_all(address, values),
            _ => Operator::err("Modbus 请选择 bool 或 16/32/64 位数值类型"),
        }
    }

    pub fn read_modbus<T: ModbusValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Operator<Box<[T]>> {
        match self {
            Self::ModbusTcp(client) => client.read(address, length),
            Self::ModbusUdp(client) => client.read(address, length),
            Self::ModbusRtu(client) | Self::ModbusAscii(client) => client.read(address, length),
            _ => Operator::err("当前连接不是 Modbus 协议"),
        }
    }

    pub fn write_modbus<T: ModbusValue>(&mut self, address: &str, value: T) -> Operator<T> {
        match self {
            Self::ModbusTcp(client) => client.write(address, value),
            Self::ModbusUdp(client) => client.write(address, value),
            Self::ModbusRtu(client) | Self::ModbusAscii(client) => client.write(address, value),
            _ => Operator::err("当前连接不是 Modbus 协议"),
        }
    }

    pub fn write_all_modbus<T: ModbusValue>(
        &mut self,
        address: &str,
        values: &[T],
    ) -> Operator<usize> {
        match self {
            Self::ModbusTcp(client) => client.write_all(address, values),
            Self::ModbusUdp(client) => client.write_all(address, values),
            Self::ModbusRtu(client) | Self::ModbusAscii(client) => {
                client.write_all(address, values)
            }
            _ => Operator::err("当前连接不是 Modbus 协议"),
        }
    }
    /// 只允许 S7 使用带长度头的 STRING；三菱误选时在发包前返回错误。
    pub fn read_s7_strings(&mut self, address: &str) -> Operator<Box<[String]>> {
        match self {
            Self::S7(inner) => inner.read_s7_strings(address),
            _ => Operator::err("只有 S7 协议支持 S7 STRING"),
        }
    }
    /// 保留 S7 字符串的容量检查，不把 S7 长度头写入三菱寄存器。
    pub fn write_s7_string(&mut self, address: &str, value: &str) -> Operator<String> {
        match self {
            Self::S7(inner) => inner.write::<String>(address, value),
            _ => Operator::err("只有 S7 协议支持 S7 STRING"),
        }
    }
}
