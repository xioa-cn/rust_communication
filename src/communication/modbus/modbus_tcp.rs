use super::modbus_packet::{MAX_PDU, mbap_length, mbap_request};
use super::modbus_transport::{
    NetworkSettings, TcpTimeouts, io_error, tcp_read, tcp_read_some, tcp_write,
    validate_network_unit,
};
use super::{ModbusClient, ModbusTransport};
use crate::communication::timeout::Timeout;
use std::net::{IpAddr, Shutdown, TcpStream};
use std::time::Instant;

pub struct TcpTransport {
    settings: NetworkSettings,
    stream: Option<TcpStream>,
    transaction: u16,
    timeouts: TcpTimeouts,
}

pub type ModbusTcp = ModbusClient<TcpTransport>;

impl ModbusClient<TcpTransport> {
    pub fn new(address: IpAddr, port: u16, timeout: Timeout) -> Self {
        Self::from_transport(
            TcpTransport {
                settings: NetworkSettings::new(address, port, timeout),
                stream: None,
                transaction: 0,
                timeouts: TcpTimeouts::default(),
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
        self.timeouts = TcpTimeouts::configured(io_timeout);
        self.stream = Some(stream);
        Ok(())
    }

    fn disconnect(&mut self) {
        self.timeouts = TcpTimeouts::default();
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
        tcp_write(stream, &frame, deadline, &mut self.timeouts)?;
        let mut response = [0; MAX_PDU + 7];
        let mut received = tcp_read_some(stream, &mut response, deadline, &mut self.timeouts)?;
        if received < 7 {
            tcp_read(
                stream,
                &mut response[received..7],
                deadline,
                &mut self.timeouts,
            )?;
            received = 7;
        }
        let header = response[..7].try_into().unwrap();
        let total = 7 + mbap_length(header, self.transaction, unit)?;
        if received > total {
            return Err("Unexpected trailing data after Modbus TCP response".into());
        }
        tcp_read(
            stream,
            &mut response[received..total],
            deadline,
            &mut self.timeouts,
        )?;
        Ok(response[7..total].to_vec())
    }

    fn validate_unit(&self, unit: u8) -> Result<(), String> {
        validate_network_unit(unit)
    }
}
