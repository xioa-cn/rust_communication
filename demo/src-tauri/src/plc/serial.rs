use super::{connection::configure_modbus, result};
use crate::models::{ModbusOptions, PlcProtocol, SerialOptions, SerialParity};
use rs_appliaction::communication::modbus::{ModbusAscii, ModbusRtu, ModbusValue};
use rs_appliaction::entity::operate::Operator;
use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};
use std::time::Duration;

enum SerialConnection {
    Rtu(ModbusRtu<Box<dyn SerialPort>>),
    Ascii(ModbusAscii<Box<dyn SerialPort>>),
}

pub struct SerialSession {
    protocol: PlcProtocol,
    serial: SerialOptions,
    modbus: ModbusOptions,
    timeout: Duration,
    connection: Option<SerialConnection>,
}

impl SerialSession {
    pub fn new(
        protocol: PlcProtocol,
        serial: SerialOptions,
        modbus: ModbusOptions,
        timeout_ms: i32,
    ) -> Result<Self, String> {
        if !protocol.is_serial() {
            return Err("串口会话只支持 Modbus RTU/ASCII".into());
        }
        validate_serial(protocol, &serial)?;
        if !(1..=247).contains(&modbus.unit_id) || !(1..=60_000).contains(&timeout_ms) {
            return Err("串口 Modbus 站号范围 1..247，超时范围 1..60000 ms".into());
        }
        Ok(Self {
            protocol,
            serial,
            modbus,
            timeout: Duration::from_millis(timeout_ms as u64),
            connection: None,
        })
    }

    pub fn connect(&mut self) -> Operator<bool> {
        if self.is_connected() {
            return Operator::ok(true);
        }
        match self.open() {
            Ok(connection) => {
                self.connection = Some(connection);
                Operator::ok(true)
            }
            Err(error) => Operator::err(&error),
        }
    }

    fn open(&self) -> Result<SerialConnection, String> {
        let data_bits = if self.serial.data_bits == 7 {
            DataBits::Seven
        } else {
            DataBits::Eight
        };
        let stop_bits = if self.serial.stop_bits == 2 {
            StopBits::Two
        } else {
            StopBits::One
        };
        let parity = match self.serial.parity {
            SerialParity::None => Parity::None,
            SerialParity::Odd => Parity::Odd,
            SerialParity::Even => Parity::Even,
        };
        let port = serialport::new(self.serial.path.trim(), self.serial.baud_rate)
            .data_bits(data_bits)
            .parity(parity)
            .stop_bits(stop_bits)
            .flow_control(FlowControl::None)
            .timeout(self.timeout)
            .dtr_on_open(false)
            .open()
            .map_err(|error| format!("打开串口 {} 失败：{error}", self.serial.path.trim()))?;
        if self.protocol == PlcProtocol::ModbusRtu {
            let mut client = ModbusRtu::new(port, self.modbus.unit_id, self.serial.baud_rate);
            configure_modbus(&mut client, self.modbus)?;
            result(client.connect())?;
            Ok(SerialConnection::Rtu(client))
        } else {
            let mut client = ModbusAscii::new(port, self.modbus.unit_id);
            configure_modbus(&mut client, self.modbus)?;
            result(client.connect())?;
            Ok(SerialConnection::Ascii(client))
        }
    }

    pub fn disconnect(&mut self) -> Operator<bool> {
        self.connection = None;
        Operator::ok(true)
    }

    pub fn is_connected(&self) -> bool {
        match &self.connection {
            Some(SerialConnection::Rtu(client)) => client.is_connected(),
            Some(SerialConnection::Ascii(client)) => client.is_connected(),
            None => false,
        }
    }

    pub fn read<Value: ModbusValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Operator<Box<[Value]>> {
        match &mut self.connection {
            Some(SerialConnection::Rtu(client)) => client.read(address, length),
            Some(SerialConnection::Ascii(client)) => client.read(address, length),
            None => Operator::err("串口尚未连接"),
        }
    }

    pub fn write<Value: ModbusValue>(&mut self, address: &str, value: Value) -> Operator<Value> {
        match &mut self.connection {
            Some(SerialConnection::Rtu(client)) => client.write(address, value),
            Some(SerialConnection::Ascii(client)) => client.write(address, value),
            None => Operator::err("串口尚未连接"),
        }
    }

    pub fn write_all<Value: ModbusValue>(
        &mut self,
        address: &str,
        values: &[Value],
    ) -> Operator<usize> {
        match &mut self.connection {
            Some(SerialConnection::Rtu(client)) => client.write_all(address, values),
            Some(SerialConnection::Ascii(client)) => client.write_all(address, values),
            None => Operator::err("串口尚未连接"),
        }
    }
}

fn validate_serial(protocol: PlcProtocol, options: &SerialOptions) -> Result<(), String> {
    let path = options.path.trim();
    if path.is_empty() || path.len() > 256 || path.chars().any(char::is_control) {
        return Err("请输入有效串口名，例如 COM3 或 /dev/ttyUSB0".into());
    }
    if !(1..=4_000_000).contains(&options.baud_rate) {
        return Err("串口波特率范围 1..4000000".into());
    }
    if !matches!(options.data_bits, 7 | 8)
        || (protocol == PlcProtocol::ModbusRtu && options.data_bits != 8)
    {
        return Err("Modbus RTU 必须使用 8 个数据位；ASCII 支持 7 或 8 个数据位".into());
    }
    if !matches!(options.stop_bits, 1 | 2) {
        return Err("串口停止位只能为 1 或 2".into());
    }
    Ok(())
}
