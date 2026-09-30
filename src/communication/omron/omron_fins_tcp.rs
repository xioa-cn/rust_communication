use super::omron_packet::{MAX_FRAME, tcp_frame};
use super::omron_transport::{NetworkSettings, io_error, tcp_read, tcp_write};
use super::{FinsRoute, FinsTransport, OmronClient};
use crate::communication::timeout::Timeout;
use std::io::BufReader;
use std::net::{IpAddr, Shutdown, TcpStream};
use std::time::Instant;

pub struct TcpTransport {
    settings: NetworkSettings,
    stream: Option<BufReader<TcpStream>>,
    active_route: Option<FinsRoute>,
}

pub type OmronFinsTcp = OmronClient<TcpTransport>;

impl OmronClient<TcpTransport> {
    pub fn new(address: IpAddr, port: u16, timeout: Timeout) -> Self {
        Self::from_transport(TcpTransport {
            settings: NetworkSettings::new(address, port, timeout),
            stream: None,
            active_route: None,
        })
    }
}

impl FinsTransport for TcpTransport {
    fn connect(&mut self, route: FinsRoute) -> Result<FinsRoute, String> {
        if let Some(active) = self.active_route {
            return Ok(active);
        }
        route.validate(false)?;
        let (connect_timeout, io_timeout) = self.settings.durations()?;
        let mut stream =
            TcpStream::connect_timeout(&self.settings.peer, connect_timeout).map_err(io_error)?;
        stream.set_nodelay(true).map_err(io_error)?;
        let deadline = Instant::now() + io_timeout;
        tcp_write(
            &mut stream,
            &tcp_frame(0, &u32::from(route.source_node).to_be_bytes()),
            deadline,
        )?;
        let mut stream = BufReader::with_capacity(MAX_FRAME + 16, stream);
        let reply = read_frame(&mut stream, 1, deadline)?;
        if reply.len() != 8 {
            return Err("FINS/TCP node negotiation length must be 8 bytes".into());
        }
        let client_node = u32::from_be_bytes(reply[..4].try_into().unwrap());
        let server_node = u32::from_be_bytes(reply[4..].try_into().unwrap());
        if !(1..=254).contains(&client_node) || !(1..=254).contains(&server_node) {
            return Err("FINS/TCP negotiated nodes must be in 1..=254".into());
        }
        if route.source_node != 0 && u32::from(route.source_node) != client_node {
            return Err("FINS/TCP server did not assign the requested source node".into());
        }
        let active = FinsRoute {
            source_node: client_node as u8,
            destination_node: if route.destination_node == 0 {
                server_node as u8
            } else {
                route.destination_node
            },
            ..route
        };
        active.validate(true)?;
        self.stream = Some(stream);
        self.active_route = Some(active);
        Ok(active)
    }

    fn disconnect(&mut self) {
        self.active_route = None;
        if let Some(stream) = self.stream.take() {
            let _ = stream.get_ref().shutdown(Shutdown::Both);
        }
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, String> {
        if !(12..=MAX_FRAME).contains(&request.len()) {
            return Err("FINS/TCP request length is invalid".into());
        }
        let deadline = Instant::now() + self.settings.durations()?.1;
        let stream = self
            .stream
            .as_mut()
            .ok_or("Omron.cs FINS TCP is not connected")?;
        tcp_write(stream.get_mut(), &tcp_frame(2, request), deadline)?;
        read_frame(stream, 2, deadline)
    }
}

fn read_frame(
    stream: &mut BufReader<TcpStream>,
    expected_command: u32,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    let mut header = [0; 16];
    tcp_read(stream, &mut header, deadline)?;
    if &header[..4] != b"FINS" {
        return Err("FINS/TCP magic header mismatch".into());
    }
    let length = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
    if !(8..=MAX_FRAME + 8).contains(&length) {
        return Err("FINS/TCP frame length is invalid".into());
    }
    let command = u32::from_be_bytes(header[8..12].try_into().unwrap());
    let error = u32::from_be_bytes(header[12..16].try_into().unwrap());
    if error != 0 {
        return Err(format!(
            "FINS/TCP header error 0x{error:08X}, command {command}"
        ));
    }
    if command != expected_command {
        return Err("FINS/TCP response command mismatch".into());
    }
    let mut payload = vec![0; length - 8];
    tcp_read(stream, &mut payload, deadline)?;
    Ok(payload)
}
