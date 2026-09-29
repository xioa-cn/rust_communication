use super::modbus_packet::lrc;
use super::modbus_transport::io_error;
use super::{ModbusClient, ModbusTransport};
use std::io::{Read, Write};

pub struct AsciiTransport<Stream: Read + Write> {
    stream: Option<Stream>,
    connected: bool,
}

pub type ModbusAscii<Stream> = ModbusClient<AsciiTransport<Stream>>;

impl<Stream: Read + Write> ModbusClient<AsciiTransport<Stream>> {
    pub fn new(stream: Stream, unit_id: u8) -> Self {
        Self::from_transport(
            AsciiTransport {
                stream: Some(stream),
                connected: false,
            },
            unit_id,
        )
    }

    pub fn into_inner(self) -> Option<Stream> {
        self.transport.stream
    }
}

impl<Stream: Read + Write> ModbusTransport for AsciiTransport<Stream> {
    fn connect(&mut self) -> Result<(), String> {
        if self.stream.is_none() {
            return Err(
                "Modbus ASCII stream was closed; create a new client with a newly opened stream"
                    .into(),
            );
        }
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) {
        self.connected = false;
        self.stream = None;
    }

    fn is_connected(&self) -> bool {
        self.connected && self.stream.is_some()
    }

    fn exchange(&mut self, unit: u8, request: &[u8]) -> Result<Vec<u8>, String> {
        if !self.is_connected() {
            return Err("Modbus ASCII is not connected".into());
        }
        let stream = self
            .stream
            .as_mut()
            .ok_or("Modbus ASCII stream is closed")?;
        let mut binary = Vec::with_capacity(request.len() + 2);
        binary.push(unit);
        binary.extend_from_slice(request);
        binary.push(lrc(&binary));
        let digits = b"0123456789ABCDEF";
        let mut frame = Vec::with_capacity(binary.len() * 2 + 3);
        frame.push(b':');
        for byte in binary {
            frame.push(digits[usize::from(byte >> 4)]);
            frame.push(digits[usize::from(byte & 15)]);
        }
        frame.extend_from_slice(b"\r\n");
        stream.write_all(&frame).map_err(io_error)?;
        stream.flush().map_err(io_error)?;
        let mut character = [0];
        stream.read_exact(&mut character).map_err(io_error)?;
        if character[0] != b':' {
            return Err("Modbus ASCII frame must start with ':'".into());
        }
        let mut encoded = Vec::new();
        loop {
            stream.read_exact(&mut character).map_err(io_error)?;
            if character[0] == b'\r' {
                stream.read_exact(&mut character).map_err(io_error)?;
                if character[0] != b'\n' {
                    return Err("Modbus ASCII frame must end with CRLF".into());
                }
                break;
            }
            if !character[0].is_ascii_hexdigit() || encoded.len() >= 510 {
                return Err("Modbus ASCII frame contains invalid hex or is too long".into());
            }
            encoded.push(character[0]);
        }
        if encoded.len() < 6 || encoded.len() % 2 != 0 {
            return Err("Modbus ASCII frame length is invalid".into());
        }
        let mut reply = Vec::with_capacity(encoded.len() / 2);
        for pair in encoded.chunks_exact(2) {
            reply.push(hex_digit(pair[0])? * 16 + hex_digit(pair[1])?);
        }
        if reply[0] != unit {
            return Err("Modbus ASCII unit ID mismatch".into());
        }
        if lrc(&reply) != 0 {
            return Err("Modbus ASCII LRC mismatch".into());
        }
        Ok(reply[1..reply.len() - 1].to_vec())
    }
}

fn hex_digit(digit: u8) -> Result<u8, String> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'A'..=b'F' => Ok(digit - b'A' + 10),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        _ => Err("Modbus ASCII hex digit is invalid".into()),
    }
}
