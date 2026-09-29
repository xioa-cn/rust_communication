use super::modbus_packet::crc16;
use super::modbus_transport::io_error;
use super::{ModbusClient, ModbusTransport};
use std::io::{Read, Write};
use std::time::Duration;

pub struct RtuTransport<Stream: Read + Write> {
    stream: Option<Stream>,
    connected: bool,
    baud_rate: u32,
}

pub type ModbusRtu<Stream> = ModbusClient<RtuTransport<Stream>>;

impl<Stream: Read + Write> ModbusClient<RtuTransport<Stream>> {
    pub fn new(stream: Stream, unit_id: u8, baud_rate: u32) -> Self {
        Self::from_transport(
            RtuTransport {
                stream: Some(stream),
                connected: false,
                baud_rate,
            },
            unit_id,
        )
    }

    pub fn into_inner(self) -> Option<Stream> {
        self.transport.stream
    }
}

impl<Stream: Read + Write> ModbusTransport for RtuTransport<Stream> {
    fn connect(&mut self) -> Result<(), String> {
        if self.baud_rate == 0 {
            return Err("Modbus RTU baud rate must be greater than zero".into());
        }
        if self.stream.is_none() {
            return Err(
                "Modbus RTU stream was closed; create a new client with a newly opened stream"
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
            return Err("Modbus RTU is not connected".into());
        }
        let delay = if self.baud_rate > 19_200 {
            Duration::from_micros(1750)
        } else {
            Duration::from_nanos(38_500_000_000_u64.div_ceil(u64::from(self.baud_rate)))
        };
        std::thread::sleep(delay);
        let stream = self.stream.as_mut().ok_or("Modbus RTU stream is closed")?;
        let mut frame = Vec::with_capacity(request.len() + 3);
        frame.push(unit);
        frame.extend_from_slice(request);
        frame.extend_from_slice(&crc16(&frame).to_le_bytes());
        stream.write_all(&frame).map_err(io_error)?;
        stream.flush().map_err(io_error)?;
        let mut prefix = [0; 3];
        stream.read_exact(&mut prefix).map_err(io_error)?;
        if prefix[0] != unit {
            return Err("Modbus RTU unit ID mismatch".into());
        }
        let remaining = if prefix[1] == (request[0] | 0x80) {
            2
        } else if prefix[1] != request[0] {
            return Err("Modbus RTU response function mismatch".into());
        } else {
            match prefix[1] {
                1..=4 if prefix[2] <= 250 => usize::from(prefix[2]) + 2,
                5 | 6 | 15 | 16 => 5,
                _ => return Err("Modbus RTU response byte count or function is invalid".into()),
            }
        };
        let mut reply = prefix.to_vec();
        reply.resize(3 + remaining, 0);
        stream.read_exact(&mut reply[3..]).map_err(io_error)?;
        let checksum_offset = reply.len() - 2;
        let checksum = u16::from_le_bytes([reply[checksum_offset], reply[checksum_offset + 1]]);
        if checksum != crc16(&reply[..checksum_offset]) {
            return Err("Modbus RTU CRC mismatch".into());
        }
        Ok(reply[1..checksum_offset].to_vec())
    }
}
