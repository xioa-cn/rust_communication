use super::modbus_packet::{mbap_length, mbap_request};
use super::modbus_transport::{
    NetworkSettings, io_error, tcp_read, tcp_write, validate_network_unit,
};
use super::{ModbusClient, ModbusTransport};
use crate::communication::timeout::Timeout;
use std::net::{IpAddr, Shutdown, TcpStream};
use std::time::Instant;

pub struct TcpTransport {
    settings: NetworkSettings,
    stream: Option<TcpStream>,
    transaction: u16,
}

pub type ModbusTcp = ModbusClient<TcpTransport>;

impl ModbusClient<TcpTransport> {
    pub fn new(address: IpAddr, port: u16, timeout: Timeout) -> Self {
        Self::from_transport(
            TcpTransport {
                settings: NetworkSettings::new(address, port, timeout),
                stream: None,
                transaction: 0,
            },
            1,
        )
    }
}

impl ModbusTransport for TcpTransport {
    fn connect(&mut self) -> Result<(), String> {
        if self.stream.is_some() {
            return Ok(());
        }
        let (connect_timeout, io_timeout) = self.settings.durations()?;
        let stream =
            TcpStream::connect_timeout(&self.settings.peer, connect_timeout).map_err(io_error)?;
        stream
            .set_read_timeout(Some(io_timeout))
            .map_err(io_error)?;
        stream
            .set_write_timeout(Some(io_timeout))
            .map_err(io_error)?;
        stream.set_nodelay(true).map_err(io_error)?;
        self.stream = Some(stream);
        Ok(())
    }

    fn disconnect(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    fn exchange(&mut self, unit: u8, request: &[u8]) -> Result<Vec<u8>, String> {
        let (_, timeout) = self.settings.durations()?;
        let deadline = Instant::now() + timeout;
        let stream = self.stream.as_mut().ok_or("Modbus TCP is not connected")?;
        self.transaction = self.transaction.wrapping_add(1);
        let frame = mbap_request(self.transaction, unit, request);
        tcp_write(stream, &frame, deadline)?;
        let mut header = [0; 7];
        tcp_read(stream, &mut header, deadline)?;
        let length = mbap_length(&header, self.transaction, unit)?;
        let mut reply = vec![0; length];
        tcp_read(stream, &mut reply, deadline)?;
        Ok(reply)
    }

    fn validate_unit(&self, unit: u8) -> Result<(), String> {
        validate_network_unit(unit)
    }
}
