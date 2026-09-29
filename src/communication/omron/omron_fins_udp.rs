use super::omron_packet::MAX_FRAME;
use super::omron_transport::{NetworkSettings, io_error, remaining};
use super::{FinsRoute, FinsTransport, OmronClient};
use crate::communication::timeout::Timeout;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::time::Instant;

pub struct UdpTransport {
    settings: NetworkSettings,
    socket: Option<UdpSocket>,
    active_route: Option<FinsRoute>,
}

pub type OmronFinsUdp = OmronClient<UdpTransport>;

impl OmronClient<UdpTransport> {
    pub fn new(address: IpAddr, port: u16, timeout: Timeout) -> Self {
        Self::from_transport(UdpTransport {
            settings: NetworkSettings::new(address, port, timeout),
            socket: None,
            active_route: None,
        })
    }
}

impl FinsTransport for UdpTransport {
    fn connect(&mut self, route: FinsRoute) -> Result<FinsRoute, String> {
        if let Some(active) = self.active_route {
            return Ok(active);
        }
        route.validate(false)?;
        let (_, timeout) = self.settings.durations()?;
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(io_error)?;
        socket.connect(self.settings.peer).map_err(io_error)?;
        socket.set_read_timeout(Some(timeout)).map_err(io_error)?;
        socket.set_write_timeout(Some(timeout)).map_err(io_error)?;
        let active = FinsRoute {
            source_node: if route.source_node == 0 {
                ip_node(socket.local_addr().map_err(io_error)?.ip())?
            } else {
                route.source_node
            },
            destination_node: if route.destination_node == 0 {
                ip_node(self.settings.peer.ip())?
            } else {
                route.destination_node
            },
            ..route
        };
        active.validate(true)?;
        self.socket = Some(socket);
        self.active_route = Some(active);
        Ok(active)
    }

    fn disconnect(&mut self) {
        self.socket = None;
        self.active_route = None;
    }

    fn is_connected(&self) -> bool {
        self.socket.is_some()
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, String> {
        if !(12..=MAX_FRAME).contains(&request.len()) {
            return Err("FINS/UDP request length is invalid".into());
        }
        let deadline = Instant::now() + self.settings.durations()?.1;
        let socket = self
            .socket
            .as_ref()
            .ok_or("Omron.cs FINS UDP is not connected")?;
        socket
            .set_write_timeout(Some(remaining(deadline)?))
            .map_err(io_error)?;
        if socket.send(request).map_err(io_error)? != request.len() {
            return Err("FINS/UDP datagram was not sent completely".into());
        }
        let mut buffer = [0; MAX_FRAME + 1];
        loop {
            socket
                .set_read_timeout(Some(remaining(deadline)?))
                .map_err(io_error)?;
            let count = match socket.recv(&mut buffer) {
                Ok(count) => count,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(io_error(error)),
            };
            if count > MAX_FRAME {
                return Err("FINS/UDP response exceeds the maximum frame length".into());
            }
            if count >= 10 && buffer[9] != request[9] {
                continue;
            }
            if count < 14 {
                return Err("FINS/UDP response is truncated".into());
            }
            return Ok(buffer[..count].to_vec());
        }
    }
}

fn ip_node(address: IpAddr) -> Result<u8, String> {
    match address {
        IpAddr::V4(address) => Ok(address.octets()[3]),
        _ => Err("FINS/UDP automatic node addressing requires IPv4".into()),
    }
}
