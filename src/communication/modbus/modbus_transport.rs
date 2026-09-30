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

#[derive(Default)]
pub(super) struct TcpTimeouts {
    read: TimeoutOption,
    write: TimeoutOption,
}

impl TcpTimeouts {
    pub(super) fn configured(timeout: Duration) -> Self {
        let key = timeout_key(timeout);
        Self {
            read: TimeoutOption(Some(key)),
            write: TimeoutOption(Some(key)),
        }
    }
}

#[derive(Default)]
struct TimeoutOption(Option<Duration>);

impl TimeoutOption {
    fn update(
        &mut self,
        timeout: Duration,
        setter: impl FnOnce(Duration) -> std::io::Result<()>,
    ) -> Result<(), String> {
        let key = timeout_key(timeout);
        if self.0 != Some(key) {
            setter(timeout).map_err(io_error)?;
            self.0 = Some(key);
        }
        Ok(())
    }
}

fn timeout_key(timeout: Duration) -> Duration {
    #[cfg(windows)]
    {
        let millis = timeout
            .as_nanos()
            .div_ceil(1_000_000)
            .min(u128::from(u32::MAX));
        Duration::from_millis(millis as u64)
    }
    #[cfg(not(windows))]
    {
        timeout
    }
}

pub(super) fn tcp_write(
    stream: &mut TcpStream,
    bytes: &[u8],
    deadline: Instant,
    timeouts: &mut TcpTimeouts,
) -> Result<(), String> {
    let mut sent = 0;
    while sent < bytes.len() {
        timeouts.write.update(remaining(deadline)?, |timeout| {
            stream.set_write_timeout(Some(timeout))
        })?;
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
    timeouts: &mut TcpTimeouts,
) -> Result<(), String> {
    let mut received = 0;
    while received < bytes.len() {
        received += tcp_read_some(stream, &mut bytes[received..], deadline, timeouts)?;
    }
    Ok(())
}

pub(super) fn tcp_read_some(
    stream: &mut TcpStream,
    bytes: &mut [u8],
    deadline: Instant,
    timeouts: &mut TcpTimeouts,
) -> Result<usize, String> {
    loop {
        timeouts.read.update(remaining(deadline)?, |timeout| {
            stream.set_read_timeout(Some(timeout))
        })?;
        match stream.read(bytes) {
            Ok(0) => return Err("Modbus peer closed the connection".into()),
            Ok(count) => return Ok(count),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, TcpListener};

    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (peer, _) = listener.accept().unwrap();
        (stream, peer)
    }

    #[test]
    fn identical_timeout_is_not_set_again_and_changes_are_applied() {
        let mut option = TimeoutOption::default();
        let mut applied = Vec::new();
        for millis in [100, 100, 100, 60, 60, 100] {
            option
                .update(Duration::from_millis(millis), |timeout| {
                    applied.push(timeout);
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(applied, [100, 60, 100].map(Duration::from_millis));
    }

    #[test]
    fn a_failed_option_update_is_not_cached() {
        let mut option = TimeoutOption(Some(Duration::from_millis(100)));
        let error = option.update(Duration::from_millis(60), |_| {
            Err(std::io::Error::other("injected setsockopt failure"))
        });
        assert!(error.is_err());
        assert_eq!(option.0, Some(Duration::from_millis(100)));
        let mut applied = false;
        option
            .update(Duration::from_millis(60), |_| {
                applied = true;
                Ok(())
            })
            .unwrap();
        assert!(applied);
        assert_eq!(option.0, Some(Duration::from_millis(60)));
    }

    #[test]
    fn expiration_is_checked_before_cached_read_or_write() {
        let (mut stream, mut peer) = pair();
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        peer.set_read_timeout(Some(Duration::from_millis(30)))
            .unwrap();
        peer.write_all(&[42]).unwrap();
        let mut timeouts = TcpTimeouts::configured(Duration::from_millis(100));
        let expired = Instant::now() - Duration::from_millis(1);
        let mut output = [0xa5];
        assert!(
            tcp_read_some(&mut stream, &mut output, expired, &mut timeouts)
                .unwrap_err()
                .contains("timed out")
        );
        assert_eq!(output, [0xa5]);
        assert!(
            tcp_write(&mut stream, &[9], expired, &mut timeouts)
                .unwrap_err()
                .contains("timed out")
        );
        let error = peer.read(&mut [0]).unwrap_err();
        assert!(matches!(
            error.kind(),
            ErrorKind::TimedOut | ErrorKind::WouldBlock
        ));
        tcp_read(
            &mut stream,
            &mut output,
            Instant::now() + Duration::from_millis(100),
            &mut timeouts,
        )
        .unwrap();
        assert_eq!(output, [42]);
    }

    #[cfg(windows)]
    #[test]
    fn windows_keys_match_the_socket_options_used_by_rust_std() {
        let (stream, _peer) = pair();
        for timeout in [
            Duration::from_nanos(1),
            Duration::from_micros(999),
            Duration::from_millis(1),
            Duration::from_nanos(1_000_001),
            Duration::from_micros(1_999),
            Duration::from_micros(999_123),
            Duration::from_millis(i32::MAX as u64),
        ] {
            stream.set_read_timeout(Some(timeout)).unwrap();
            stream.set_write_timeout(Some(timeout)).unwrap();
            assert_eq!(stream.read_timeout().unwrap(), Some(timeout_key(timeout)));
            assert_eq!(stream.write_timeout().unwrap(), Some(timeout_key(timeout)));
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_fractional_milliseconds_share_only_equivalent_keys() {
        let mut option = TimeoutOption::default();
        let mut applied = Vec::new();
        for micros in [999_001, 999_500, 999_999, 1_000_000, 999_000, 999_001] {
            option
                .update(Duration::from_micros(micros), |timeout| {
                    applied.push(timeout);
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(
            applied,
            [999_001, 999_000, 999_001].map(Duration::from_micros)
        );
    }
}
