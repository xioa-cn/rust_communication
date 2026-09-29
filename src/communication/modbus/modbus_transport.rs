use crate::communication::timeout::Timeout;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

pub trait ModbusTransport {
    fn connect(&mut self) -> Result<(), String>;
    fn disconnect(&mut self);
    fn is_connected(&self) -> bool;
    fn exchange(&mut self, unit: u8, request: &[u8]) -> Result<Vec<u8>, String>;
    fn validate_unit(&self, unit: u8) -> Result<(), String> {
        if !(1..=247).contains(&unit) {
            return Err(
                "Modbus serial unit ID must be in 1..=247; broadcasts are not supported".into(),
            );
        }
        Ok(())
    }
}

pub(super) struct NetworkSettings {
    pub peer: SocketAddr,
    pub timeout: Timeout,
}

impl NetworkSettings {
    pub fn new(address: IpAddr, port: u16, timeout: Timeout) -> Self {
        Self {
            peer: SocketAddr::new(address, port),
            timeout,
        }
    }

    pub fn durations(&self) -> Result<(Duration, Duration), String> {
        if self.peer.port() == 0 {
            return Err("Modbus port must be greater than zero".into());
        }
        let connect = self.timeout.connect_time_out();
        let receive = self.timeout.receive_time_out();
        if connect <= 0 || receive <= 0 {
            return Err("Modbus timeouts must be greater than zero".into());
        }
        Ok((
            Duration::from_millis(connect as u64),
            Duration::from_millis(receive as u64),
        ))
    }
}

pub(super) fn validate_network_unit(unit: u8) -> Result<(), String> {
    if unit == 0 {
        return Err(
            "Modbus unit ID zero is not supported; broadcasts have no confirmed response".into(),
        );
    }
    Ok(())
}

pub(super) fn io_error(error: std::io::Error) -> String {
    format!("Modbus I/O error: {error}")
}

pub(super) fn remaining(deadline: Instant) -> Result<Duration, String> {
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() {
        return Err("Modbus transaction timed out".into());
    }
    Ok(duration)
}

pub(super) fn tcp_write(
    stream: &mut TcpStream,
    bytes: &[u8],
    deadline: Instant,
) -> Result<(), String> {
    let mut sent = 0;
    while sent < bytes.len() {
        stream
            .set_write_timeout(Some(remaining(deadline)?))
            .map_err(io_error)?;
        match stream.write(&bytes[sent..]) {
            Ok(0) => return Err("Modbus socket stopped accepting data".into()),
            Ok(count) => sent += count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}

pub(super) fn tcp_read(
    stream: &mut TcpStream,
    bytes: &mut [u8],
    deadline: Instant,
) -> Result<(), String> {
    let mut received = 0;
    while received < bytes.len() {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(io_error)?;
        match stream.read(&mut bytes[received..]) {
            Ok(0) => return Err("Modbus peer closed the connection".into()),
            Ok(count) => received += count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}
