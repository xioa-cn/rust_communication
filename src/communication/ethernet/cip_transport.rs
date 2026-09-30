use super::CipOptions;
use std::io::{BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::ops::Range;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

static CONNECTION_SERIAL: AtomicU32 = AtomicU32::new(1);

pub(crate) struct CipError {
    pub message: String,
    pub broken: bool,
}
pub(crate) type Result<T> = std::result::Result<T, CipError>;

impl CipError {
    pub fn protocol(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            broken: true,
        }
    }
    pub fn local(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            broken: false,
        }
    }
}

impl From<std::io::Error> for CipError {
    fn from(error: std::io::Error) -> Self {
        Self::protocol(format!("CIP transport: {error}"))
    }
}

pub(crate) struct Reply {
    pub status: u8,
    pub data: Range<usize>,
}

pub(crate) struct Connection {
    stream: BufReader<TcpStream>,
    pub request: Vec<u8>,
    pub response: Vec<u8>,
    options: CipOptions,
    session: u32,
    context: u64,
    sequence: u16,
    connection_serial: u16,
    outgoing_id: u32,
    incoming_id: u32,
    connected: bool,
    embedded: Option<usize>,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl Connection {
    pub fn connect(
        peer: SocketAddr,
        connect_timeout: Duration,
        io_timeout: Duration,
        options: CipOptions,
    ) -> Result<Self> {
        let stream = TcpStream::connect_timeout(&peer, connect_timeout)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(io_timeout))?;
        stream.set_write_timeout(Some(io_timeout))?;
        let capacity = usize::from(options.connection_size) + options.route.len() + 128;
        let serial = CONNECTION_SERIAL.fetch_add(1, Ordering::Relaxed);
        let mut connection = Self {
            stream: BufReader::with_capacity(capacity, stream),
            request: Vec::with_capacity(capacity),
            response: Vec::with_capacity(capacity),
            options,
            session: 0,
            context: 0,
            sequence: 0,
            connection_serial: (serial % 65535 + 1) as u16,
            outgoing_id: 0,
            incoming_id: serial | 0x8000_0000,
            connected: false,
            embedded: None,
            read_timeout: Some(timeout_key(io_timeout)),
            write_timeout: Some(timeout_key(io_timeout)),
        };
        let deadline = Instant::now() + io_timeout;
        connection.header(0x65);
        connection.request.extend_from_slice(&[1, 0, 0, 0]);
        connection.exchange(deadline, true)?;
        if connection.response.as_slice() != [1, 0, 0, 0] {
            return Err(CipError::protocol("Invalid RegisterSession response"));
        }
        if connection.options.connected {
            connection.forward_open(deadline)?;
        }
        Ok(connection)
    }

    fn forward_open(&mut self, deadline: Instant) -> Result<()> {
        let large = self.options.connection_size > 511;
        let service = if large { 0x5b } else { 0x54 };
        self.begin_cpf(false);
        self.request
            .extend_from_slice(&[service, 2, 0x20, 6, 0x24, 1, 0x0a, 0x0e]);
        self.request
            .extend_from_slice(&self.outgoing_id.to_le_bytes());
        self.request
            .extend_from_slice(&self.incoming_id.to_le_bytes());
        self.identity();
        self.request
            .extend_from_slice(&[self.options.timeout_multiplier, 0, 0, 0]);
        for _ in 0..2 {
            self.request
                .extend_from_slice(&self.options.packet_interval_us.to_le_bytes());
            if large {
                self.request.extend_from_slice(
                    &(0x4200_0000 | u32::from(self.options.connection_size)).to_le_bytes(),
                );
            } else {
                self.request
                    .extend_from_slice(&(0x4200 | self.options.connection_size).to_le_bytes());
            }
        }
        self.request
            .extend_from_slice(&[0xa3, ((self.options.route.len() + 6) / 2) as u8]);
        self.connection_path();
        let reply = self.send_tag(service, deadline)?;
        let data = &self.response[reply.data];
        if reply.status != 0
            || data.len() < 26
            || data[25] != 0
            || data.len() != 26 + usize::from(data[24]) * 2
        {
            return Err(CipError::protocol("Malformed ForwardOpen acknowledgement"));
        }
        if data[8..10] != self.connection_serial.to_le_bytes()
            || data[10..12] != self.options.originator_vendor_id.to_le_bytes()
            || data[12..16] != self.options.originator_serial.to_le_bytes()
        {
            return Err(CipError::protocol(
                "ForwardOpen originator identity mismatch",
            ));
        }
        self.outgoing_id = u32::from_le_bytes(data[..4].try_into().unwrap());
        self.incoming_id = u32::from_le_bytes(data[4..8].try_into().unwrap());
        if self.outgoing_id == 0 || self.incoming_id == 0 {
            return Err(CipError::protocol(
                "ForwardOpen returned zero connection ID",
            ));
        }
        self.connected = true;
        Ok(())
    }

    fn identity(&mut self) {
        self.request
            .extend_from_slice(&self.connection_serial.to_le_bytes());
        self.request
            .extend_from_slice(&self.options.originator_vendor_id.to_le_bytes());
        self.request
            .extend_from_slice(&self.options.originator_serial.to_le_bytes());
    }

    fn connection_path(&mut self) {
        self.request.extend_from_slice(&self.options.route);
        self.request.extend_from_slice(&[0x20, 2, 0x24, 1, 0x2c, 1]);
    }

    pub fn disconnect(&mut self, deadline: Instant) -> Result<()> {
        let close = if self.connected {
            self.connected = false;
            self.begin_cpf(false);
            self.request
                .extend_from_slice(&[0x4e, 2, 0x20, 6, 0x24, 1, 0x0a, 0x0e]);
            self.identity();
            self.request
                .extend_from_slice(&[((self.options.route.len() + 6) / 2) as u8, 0]);
            self.connection_path();
            self.send_tag(0x4e, deadline).and_then(|reply| {
                let data = &self.response[reply.data];
                if reply.status != 0
                    || data.len() < 10
                    || data[9] != 0
                    || data.len() != 10 + usize::from(data[8]) * 2
                    || data[..2] != self.connection_serial.to_le_bytes()
                    || data[2..4] != self.options.originator_vendor_id.to_le_bytes()
                    || data[4..8] != self.options.originator_serial.to_le_bytes()
                {
                    Err(CipError::protocol("Invalid ForwardClose acknowledgement"))
                } else {
                    Ok(())
                }
            })
        } else {
            Ok(())
        };
        self.header(0x66);
        let unregister = self.send(deadline);
        close.and(unregister)
    }

    fn header(&mut self, command: u16) {
        self.request.clear();
        self.request.resize(24, 0);
        self.request[..2].copy_from_slice(&command.to_le_bytes());
        self.request[4..8].copy_from_slice(&self.session.to_le_bytes());
        self.context = self.context.wrapping_add(1);
        if command != 0x70 {
            self.request[12..20].copy_from_slice(&self.context.to_le_bytes());
        }
        self.embedded = None;
    }

    fn begin_cpf(&mut self, connected: bool) {
        self.header(if connected { 0x70 } else { 0x6f });
        self.request.extend_from_slice(&[0, 0, 0, 0, 0, 0, 2, 0]);
        if connected {
            self.sequence = self.sequence.wrapping_add(1);
            self.request.extend_from_slice(&[0xa1, 0, 4, 0]);
            self.request
                .extend_from_slice(&self.outgoing_id.to_le_bytes());
            self.request.extend_from_slice(&[0xb1, 0, 0, 0]);
            self.request.extend_from_slice(&self.sequence.to_le_bytes());
        } else {
            self.request.extend_from_slice(&[0, 0, 0, 0, 0xb2, 0, 0, 0]);
        }
    }

    pub fn begin_tag(&mut self, service: u8, path: &[u8]) {
        self.begin_cpf(self.connected);
        if !self.connected {
            self.request
                .extend_from_slice(&[0x52, 2, 0x20, 6, 0x24, 1, 0x0a, 0x0e, 0, 0]);
            self.embedded = Some(self.request.len());
        }
        self.request
            .extend_from_slice(&[service, (path.len() / 2) as u8]);
        self.request.extend_from_slice(path);
    }

    pub fn tag_budget(&self) -> usize {
        usize::from(self.options.connection_size).saturating_sub(if self.connected {
            2
        } else {
            self.options.route.len() + 13
        })
    }

    pub fn send_tag(&mut self, service: u8, deadline: Instant) -> Result<Reply> {
        let routed = self.embedded.is_some();
        if let Some(start) = self.embedded.take() {
            let length = self.request.len() - start;
            self.request[start - 2..start].copy_from_slice(&(length as u16).to_le_bytes());
            if length % 2 != 0 {
                self.request.push(0);
            }
            self.request
                .extend_from_slice(&[(self.options.route.len() / 2) as u8, 0]);
            self.request.extend_from_slice(&self.options.route);
        }
        let data_start = if self.connected { 44 } else { 40 };
        let length = self.request.len() - data_start;
        self.request[data_start - 2..data_start].copy_from_slice(&(length as u16).to_le_bytes());
        self.exchange(deadline, false)?;
        let range = self.cpf_data()?;
        let data = &self.response[range.clone()];
        let limit = if self.connected {
            usize::from(self.options.connection_size) - 2
        } else {
            504
        };
        if data.len() > limit {
            return Err(CipError::protocol(
                "CIP response exceeds the connection payload limit",
            ));
        }
        if data.len() < 4 || data[1] != 0 {
            return Err(CipError::protocol("Malformed CIP response header"));
        }
        let extended = usize::from(data[3]) * 2;
        if data.len() < 4 + extended {
            return Err(CipError::protocol("Truncated CIP additional status"));
        }
        if routed && data[0] == 0xd2 && !matches!(data[2], 0 | 6) {
            return Err(CipError::local(format!(
                "CIP routed request failed: status=0x{:02X}, additional={:02X?}",
                data[2],
                &data[4..4 + extended]
            )));
        }
        if data[0] != (service | 0x80) {
            return Err(CipError::protocol(format!(
                "Unexpected CIP reply service 0x{:02X}",
                data[0]
            )));
        }
        if !matches!(data[2], 0 | 6) {
            return Err(CipError::local(format!(
                "CIP service 0x{service:02X} failed: status=0x{:02X}, additional={:02X?}",
                data[2],
                &data[4..4 + extended]
            )));
        }
        Ok(Reply {
            status: data[2],
            data: range.start + 4 + extended..range.end,
        })
    }

    fn exchange(&mut self, deadline: Instant, registration: bool) -> Result<()> {
        let length = u16::try_from(self.request.len() - 24)
            .map_err(|_| CipError::local("EtherNet/IP request too large"))?;
        self.request[2..4].copy_from_slice(&length.to_le_bytes());
        self.send(deadline)?;
        let mut header = [0u8; 24];
        self.receive(&mut header, deadline)?;
        if header[..2] != self.request[..2] {
            return Err(CipError::protocol(format!(
                "EtherNet/IP command mismatch: expected 0x{:04X}, received 0x{:04X}",
                u16::from_le_bytes(self.request[..2].try_into().unwrap()),
                u16::from_le_bytes(header[..2].try_into().unwrap())
            )));
        }
        let status = u32::from_le_bytes(header[8..12].try_into().unwrap());
        if status != 0 {
            return Err(CipError::protocol(format!(
                "EtherNet/IP status 0x{status:08X}"
            )));
        }
        if self.request[0] != 0x70 && header[12..20] != self.context.to_le_bytes() {
            return Err(CipError::protocol(format!(
                "EtherNet/IP sender context mismatch: expected 0x{:016X}, received 0x{:016X}",
                self.context,
                u64::from_le_bytes(header[12..20].try_into().unwrap())
            )));
        }
        if header[20..24] != [0; 4] {
            return Err(CipError::protocol(format!(
                "EtherNet/IP options mismatch: expected 0x00000000, received 0x{:08X}",
                u32::from_le_bytes(header[20..24].try_into().unwrap())
            )));
        }
        let session = u32::from_le_bytes(header[4..8].try_into().unwrap());
        if session == 0 || (!registration && session != self.session) {
            return Err(CipError::protocol("EtherNet/IP session mismatch"));
        }
        self.session = session;
        let length = usize::from(u16::from_le_bytes([header[2], header[3]]));
        let limit = if self.connected {
            usize::from(self.options.connection_size)
        } else {
            504
        };
        if length > limit + 24 {
            return Err(CipError::protocol(
                "EtherNet/IP response exceeds configured message budget",
            ));
        }
        let mut response = std::mem::take(&mut self.response);
        response.resize(length, 0);
        let result = self.receive(&mut response, deadline);
        self.response = response;
        result
    }

    fn cpf_data(&self) -> Result<Range<usize>> {
        let frame = &self.response;
        if frame.len() < 8 || frame[..4] != [0; 4] || frame[6..8] != [2, 0] {
            return Err(CipError::protocol("Invalid CIP common packet format"));
        }
        let mut offset = 8;
        for index in 0..2 {
            if frame.len() < offset + 4 {
                return Err(CipError::protocol("Truncated CPF item"));
            }
            let kind = u16::from_le_bytes([frame[offset], frame[offset + 1]]);
            let length = usize::from(u16::from_le_bytes([frame[offset + 2], frame[offset + 3]]));
            offset += 4;
            let end = offset
                .checked_add(length)
                .filter(|end| *end <= frame.len())
                .ok_or_else(|| CipError::protocol("CPF length exceeds frame"))?;
            if index == 0 {
                if self.connected {
                    if kind != 0xa1
                        || length != 4
                        || frame[offset..end] != self.incoming_id.to_le_bytes()
                    {
                        return Err(CipError::protocol("Connected CIP address mismatch"));
                    }
                } else if kind != 0 || length != 0 {
                    return Err(CipError::protocol("Invalid unconnected CPF address"));
                }
            } else {
                if end != frame.len() {
                    return Err(CipError::protocol("Trailing CPF data"));
                }
                if self.connected {
                    if kind != 0xb1
                        || length < 2
                        || frame[offset..offset + 2] != self.sequence.to_le_bytes()
                    {
                        return Err(CipError::protocol("Connected CIP sequence mismatch"));
                    }
                    return Ok(offset + 2..end);
                }
                if kind != 0xb2 {
                    return Err(CipError::protocol("Unexpected CPF data type"));
                }
                return Ok(offset..end);
            }
            offset = end;
        }
        Err(CipError::protocol("Missing CPF data"))
    }

    fn send(&mut self, deadline: Instant) -> Result<()> {
        let mut offset = 0;
        while offset < self.request.len() {
            let remaining = remaining(deadline)?;
            if self.write_timeout != Some(timeout_key(remaining)) {
                self.stream.get_mut().set_write_timeout(Some(remaining))?;
                self.write_timeout = Some(timeout_key(remaining));
            }
            match self.stream.get_mut().write(&self.request[offset..]) {
                Ok(0) => return Err(CipError::protocol("CIP socket stopped accepting data")),
                Ok(written) => offset += written,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn receive(&mut self, output: &mut [u8], deadline: Instant) -> Result<()> {
        let mut offset = 0;
        while offset < output.len() {
            let remaining = remaining(deadline)?;
            if self.read_timeout != Some(timeout_key(remaining)) {
                self.stream.get_mut().set_read_timeout(Some(remaining))?;
                self.read_timeout = Some(timeout_key(remaining));
            }
            match self.stream.read(&mut output[offset..]) {
                Ok(0) => return Err(CipError::protocol("CIP peer closed the connection")),
                Ok(read) => offset += read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.stream.get_mut().shutdown(Shutdown::Both);
    }
}

fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| CipError::protocol("CIP transaction timed out"))
}

fn timeout_key(duration: Duration) -> Duration {
    #[cfg(windows)]
    {
        Duration::from_millis(
            duration
                .as_nanos()
                .div_ceil(1_000_000)
                .min(u32::MAX as u128) as u64,
        )
    }
    #[cfg(not(windows))]
    {
        duration
    }
}
