use super::modbus_packet::{mbap_length, mbap_request};
use super::modbus_transport::{NetworkSettings, io_error, remaining, validate_network_unit};
use super::{ModbusClient, ModbusTransport};
use crate::communication::timeout::Timeout;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::Instant;

pub struct UdpTransport {
    settings: NetworkSettings,
    socket: Option<UdpSocket>,
    transaction: u16,
}

pub type ModbusUdp = ModbusClient<UdpTransport>;

impl ModbusClient<UdpTransport> {
    pub fn new(address: IpAddr, port: u16, timeout: Timeout) -> Self {
        Self::from_transport(
            UdpTransport {
                settings: NetworkSettings::new(address, port, timeout),
                socket: None,
                transaction: 0,
            },
            1,
        )
    }
}

impl ModbusTransport for UdpTransport {
    fn connect(&mut self) -> Result<(), String> {
        if self.socket.is_some() {
            return Ok(());
        }
        let (_, timeout) = self.settings.durations()?;
        let local = match self.settings.peer.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        };
        let socket = UdpSocket::bind(SocketAddr::new(local, 0)).map_err(io_error)?;
        socket.connect(self.settings.peer).map_err(io_error)?;
        socket.set_read_timeout(Some(timeout)).map_err(io_error)?;
        socket.set_write_timeout(Some(timeout)).map_err(io_error)?;
        self.socket = Some(socket);
        Ok(())
    }

    fn disconnect(&mut self) {
        self.socket = None;
    }

    fn is_connected(&self) -> bool {
        self.socket.is_some()
    }

    fn exchange(&mut self, unit: u8, request: &[u8]) -> Result<Vec<u8>, String> {
        let (_, timeout) = self.settings.durations()?;
        let deadline = Instant::now() + timeout;
        let socket = self.socket.as_ref().ok_or("Modbus UDP is not connected")?;
        self.transaction = self.transaction.wrapping_add(1);
        let frame = mbap_request(self.transaction, unit, request);
        socket
            .set_write_timeout(Some(remaining(deadline)?))
            .map_err(io_error)?;
        if socket.send(&frame).map_err(io_error)? != frame.len() {
            return Err("Modbus UDP request was not sent completely".into());
        }
        let mut buffer = [0; 261];
        loop {
            socket
                .set_read_timeout(Some(remaining(deadline)?))
                .map_err(io_error)?;
            let count = match socket.recv(&mut buffer) {
                Ok(count) => count,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(io_error(error)),
            };
            if count >= 2 && u16::from_be_bytes([buffer[0], buffer[1]]) != self.transaction {
                continue;
            }
            if !(8..=260).contains(&count) {
                return Err("Modbus UDP datagram length is invalid".into());
            }
            let mut header = [0; 7];
            header.copy_from_slice(&buffer[..7]);
            let length = mbap_length(&header, self.transaction, unit)?;
            if count != 7 + length {
                return Err("Modbus UDP datagram length does not match MBAP length".into());
            }
            return Ok(buffer[7..count].to_vec());
        }
    }

    fn validate_unit(&self, unit: u8) -> Result<(), String> {
        validate_network_unit(unit)
    }
}
