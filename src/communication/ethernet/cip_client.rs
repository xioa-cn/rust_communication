use super::cip_address::{self, TagPath};
use super::cip_transport::{CipError, Connection};
use super::{CipOptions, CipValue, CipVendor};
use crate::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use crate::communication::timeout::{Timeout, as_operator};
use crate::entity::operate::Operator;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

pub struct CipClient {
    peer: SocketAddr,
    timeout: Timeout,
    vendor: CipVendor,
    options: CipOptions,
    connection: Option<Connection>,
    cached_address: String,
    path: TagPath,
    read_buffer: Vec<u8>,
}

impl CipClient {
    pub fn new(address: IpAddr, port: u16, vendor: CipVendor, timeout: Timeout) -> Self {
        Self {
            peer: SocketAddr::new(address, port),
            timeout,
            vendor,
            options: if vendor == CipVendor::Inovance {
                CipOptions::connected()
            } else {
                CipOptions::default()
            },
            connection: None,
            cached_address: String::new(),
            path: TagPath {
                bytes: Vec::new(),
                type_code: None,
            },
            read_buffer: Vec::new(),
        }
    }

    pub fn vendor(&self) -> CipVendor {
        self.vendor
    }
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }
    pub fn options(&self) -> &CipOptions {
        &self.options
    }

    pub fn set_options(&mut self, options: CipOptions) -> Operator<bool> {
        if self.is_connected() {
            return Operator::err("Disconnect CIP before changing options");
        }
        if let Err(error) = options.validate() {
            return Operator::err(&error);
        }
        self.options = options;
        Operator::ok(true)
    }

    fn durations(&self) -> Result<(Duration, Duration), String> {
        if self.peer.port() == 0
            || self.timeout.connect_time_out() <= 0
            || self.timeout.receive_time_out() <= 0
        {
            return Err("CIP port and timeouts must be positive".into());
        }
        Ok((
            Duration::from_millis(self.timeout.connect_time_out() as u64),
            Duration::from_millis(self.timeout.receive_time_out() as u64),
        ))
    }

    pub fn connect(&mut self) -> Operator<bool> {
        if self.is_connected() {
            return Operator::ok(true);
        }
        as_operator((|| {
            self.options.validate()?;
            let (connect_timeout, io_timeout) = self.durations()?;
            self.connection = Some(
                Connection::connect(self.peer, connect_timeout, io_timeout, self.options.clone())
                    .map_err(|error| error.message)?,
            );
            Ok(true)
        })())
    }

    pub fn disconnect(&mut self) -> Operator<bool> {
        let Some(mut connection) = self.connection.take() else {
            return Operator::ok(true);
        };
        let timeout = Duration::from_millis(self.timeout.receive_time_out().max(1) as u64);
        as_operator(
            connection
                .disconnect(Instant::now() + timeout)
                .map(|()| true)
                .map_err(|error| error.message),
        )
    }

    fn prepare<T: CipValue>(&mut self, address: &str, count: usize) -> Result<usize, String> {
        let bytes = count
            .checked_mul(T::WIDTH)
            .filter(|bytes| *bytes <= 1_048_576)
            .ok_or("CIP payload exceeds 1 MiB")?;
        if count == 0 || count > u16::MAX as usize {
            return Err("CIP element count must be 1..=65535".into());
        }
        if self.path.bytes.is_empty() || address != self.cached_address {
            let path = cip_address::encode(address)?;
            self.cached_address.clear();
            self.cached_address.push_str(address);
            self.path = path;
        }
        if self.path.type_code.is_some_and(|kind| !T::accepts(kind)) {
            return Err("CIP explicit tag type does not match the requested Rust type".into());
        }
        if !self.is_connected() {
            return Err("CIP is not connected".into());
        }
        Ok(bytes)
    }

    fn finish<T>(&mut self, result: Result<T, CipError>) -> Result<T, String> {
        result.map_err(|error| {
            if error.broken {
                self.connection = None;
            }
            error.message
        })
    }

    pub fn read<T: CipValue>(&mut self, address: &str, length: usize) -> Operator<Box<[T]>> {
        as_operator((|| {
            let bytes = self.read_raw::<T>(address, length)?;
            let mut values = Vec::with_capacity(length);
            for value in bytes.chunks_exact(T::WIDTH) {
                values.push(T::decode(value)?);
            }
            Ok(values.into_boxed_slice())
        })())
    }

    pub fn read_into<T: CipValue>(
        &mut self,
        address: &str,
        destination: &mut [T],
    ) -> Operator<usize> {
        as_operator((|| {
            let bytes = self.read_raw::<T>(address, destination.len())?;
            for (target, bytes) in destination.iter_mut().zip(bytes.chunks_exact(T::WIDTH)) {
                *target = T::decode(bytes)?;
            }
            Ok(destination.len())
        })())
    }

    pub(crate) fn read_raw<T: CipValue>(
        &mut self,
        address: &str,
        count: usize,
    ) -> Result<&[u8], String> {
        let expected = self.prepare::<T>(address, count)?;
        let deadline = Instant::now() + self.durations()?.1;
        self.read_buffer.clear();
        self.read_buffer.reserve(expected);
        let result = (|| {
            let connection = self.connection.as_mut().unwrap();
            if self.path.bytes.len() + 8 > connection.tag_budget() {
                return Err(CipError::local(
                    "CIP tag path does not fit the message budget",
                ));
            }
            let mut previous_type = None;
            loop {
                let service = if self.read_buffer.is_empty() {
                    0x4c
                } else {
                    0x52
                };
                connection.begin_tag(service, &self.path.bytes);
                connection
                    .request
                    .extend_from_slice(&(count as u16).to_le_bytes());
                if service == 0x52 {
                    connection
                        .request
                        .extend_from_slice(&(self.read_buffer.len() as u32).to_le_bytes());
                }
                let reply = connection.send_tag(service, deadline)?;
                let bytes = &connection.response[reply.data];
                if bytes.len() < 2 {
                    return Err(CipError::protocol("CIP read omitted its data type"));
                }
                let kind = u16::from_le_bytes([bytes[0], bytes[1]]);
                if let Some(previous) = previous_type.filter(|previous| *previous != kind) {
                    return Err(CipError::protocol(format!(
                        "CIP fragmented read changed tag type: expected 0x{previous:04X}, received 0x{kind:04X}"
                    )));
                }
                if !T::accepts(kind) || self.path.type_code.is_some_and(|expected| expected != kind)
                {
                    let expected = self.path.type_code.unwrap_or(T::TYPE_CODE);
                    return Err(CipError::protocol(format!(
                        "CIP tag type mismatch: PLC returned {} (0x{kind:04X}), requested {} (0x{expected:04X}); select the matching data type",
                        super::cip_value::type_name(kind),
                        super::cip_value::type_name(expected)
                    )));
                }
                previous_type = Some(kind);
                let payload = &bytes[2..];
                if T::TYPE_CODE == 0xc1
                    && self.vendor == CipVendor::Omron
                    && count == 1
                    && self.read_buffer.is_empty()
                    && reply.status == 0
                    && payload.len() == 2
                {
                    let value = u16::from_le_bytes([payload[0], payload[1]]);
                    if !matches!(value, 0 | 1 | 0xff | 0xffff) {
                        return Err(CipError::protocol("CIP read returned an invalid BOOL"));
                    }
                    self.read_buffer.push(u8::from(value != 0));
                    return Ok(());
                }
                if payload.is_empty() || payload.len() > expected - self.read_buffer.len() {
                    return Err(CipError::protocol(
                        "CIP read made no progress or exceeded the requested length",
                    ));
                }
                self.read_buffer.extend_from_slice(payload);
                if reply.status == 0 {
                    if self.read_buffer.len() != expected {
                        return Err(CipError::protocol(
                            "CIP read returned an unexpected element count",
                        ));
                    }
                    if T::TYPE_CODE == 0xc1
                        && self
                            .read_buffer
                            .iter()
                            .any(|value| !matches!(*value, 0 | 1 | 0xff))
                    {
                        return Err(CipError::protocol("CIP read returned an invalid BOOL"));
                    }
                    return Ok(());
                }
                if self.read_buffer.len() == expected {
                    return Err(CipError::protocol(
                        "CIP partial status at the completed byte count",
                    ));
                }
            }
        })();
        self.finish(result)?;
        Ok(&self.read_buffer)
    }

    pub fn write<T: CipValue>(&mut self, address: &str, value: T) -> Operator<T> {
        as_operator(
            self.write_values(address, std::slice::from_ref(&value))
                .map(|_| value),
        )
    }

    pub fn write_all<T: CipValue>(&mut self, address: &str, values: &[T]) -> Operator<usize> {
        as_operator(self.write_values(address, values))
    }

    fn write_values<T: CipValue>(&mut self, address: &str, values: &[T]) -> Result<usize, String> {
        self.write_encoded::<T>(address, values.len(), |range, destination| {
            for value in &values[range] {
                value.append(destination);
            }
        })
    }

    pub(crate) fn write_raw<T: CipValue>(
        &mut self,
        address: &str,
        bytes: &[u8],
    ) -> Result<usize, String> {
        if bytes.len() % T::WIDTH != 0
            || (T::TYPE_CODE == 0xc1 && bytes.iter().any(|value| *value > 1))
        {
            return Err("Invalid CIP ABI payload".into());
        }
        self.write_encoded::<T>(address, bytes.len() / T::WIDTH, |range, destination| {
            destination.extend_from_slice(&bytes[range.start * T::WIDTH..range.end * T::WIDTH]);
        })
    }

    fn write_encoded<T: CipValue>(
        &mut self,
        address: &str,
        count: usize,
        mut append: impl FnMut(std::ops::Range<usize>, &mut Vec<u8>),
    ) -> Result<usize, String> {
        let total_bytes = self.prepare::<T>(address, count)?;
        let deadline = Instant::now() + self.durations()?.1;
        let result = (|| {
            let connection = self.connection.as_mut().unwrap();
            let overhead = self.path.bytes.len() + 6;
            let padded = self.vendor == CipVendor::Omron && !self.options.connected;
            let budget = if padded {
                connection.tag_budget() & !1
            } else {
                connection.tag_budget()
            };
            let fragmented = overhead
                .checked_add(total_bytes)
                .is_none_or(|bytes| bytes > budget);
            let overhead = overhead + if fragmented { 4 } else { 0 };
            let chunk_count = budget.saturating_sub(overhead) / T::WIDTH;
            if chunk_count == 0 {
                return Err(CipError::local(
                    "CIP tag path leaves no room for write payload",
                ));
            }
            let service = if fragmented { 0x53 } else { 0x4d };
            let mut written = 0;
            while written < count {
                let end = (written + chunk_count).min(count);
                connection.begin_tag(service, &self.path.bytes);
                connection
                    .request
                    .extend_from_slice(&self.path.type_code.unwrap_or(T::TYPE_CODE).to_le_bytes());
                connection
                    .request
                    .extend_from_slice(&(count as u16).to_le_bytes());
                if fragmented {
                    connection
                        .request
                        .extend_from_slice(&((written * T::WIDTH) as u32).to_le_bytes());
                }
                append(written..end, &mut connection.request);
                if padded && connection.request.len() % 2 != 0 {
                    connection.request.push(0);
                }
                let reply = connection.send_tag(service, deadline).map_err(|mut error| {
                    error.message = format!("{}; {written} elements acknowledged; current write outcome may be unknown; no retry performed", error.message);
                    error
                })?;
                if reply.status != 0 || !reply.data.is_empty() {
                    return Err(CipError::protocol(
                        "Invalid CIP write acknowledgement; write outcome may be unknown",
                    ));
                }
                written = end;
            }
            Ok(count)
        })();
        self.finish(result)
    }

    pub fn read_string(&mut self, address: &str, byte_length: usize) -> Operator<String> {
        as_operator((|| {
            if byte_length == 0 || byte_length > 1_048_576 {
                return Err("CIP string byte limit must be 1..=1048576".into());
            }
            let deadline = Instant::now() + self.durations()?.1;
            self.read_string_value(address, byte_length, deadline)
        })())
    }

    pub fn read_strings(&mut self, address: &str, count: usize) -> Operator<Box<[String]>> {
        as_operator((|| {
            if count == 0 || count > u16::MAX as usize {
                return Err("CIP string element count must be 1..=65535".into());
            }
            let deadline = Instant::now() + self.durations()?.1;
            if count == 1 {
                return self
                    .read_string_value(address, 1_048_576, deadline)
                    .map(|value| vec![value].into_boxed_slice());
            }
            let (base, start) = if let Some(indexed) = address.strip_suffix(']') {
                let (base, index) = indexed
                    .rsplit_once('[')
                    .ok_or("Invalid CIP string array address")?;
                let start = index
                    .parse::<u32>()
                    .map_err(|_| "CIP string arrays require a single nonnegative final index")?;
                (base, start)
            } else {
                (address, 0)
            };
            start
                .checked_add(count as u32 - 1)
                .ok_or("CIP string array index exceeds u32")?;
            let mut values = Vec::with_capacity(count);
            let mut bytes = 0;
            for index in 0..count {
                let tag = format!("{base}[{}]", start + index as u32);
                let value = self.read_string_value(&tag, 1_048_576 - bytes, deadline)?;
                bytes += value.len();
                values.push(value);
            }
            Ok(values.into_boxed_slice())
        })())
    }

    fn read_string_value(
        &mut self,
        address: &str,
        byte_limit: usize,
        deadline: Instant,
    ) -> Result<String, String> {
        if self.vendor != CipVendor::Omron {
            return Err("CIP STRING reading currently requires the Omron STRING (0x00D0) layout; UDT layouts are not supported".into());
        }
        if self.path.bytes.is_empty() || address != self.cached_address {
            let path = cip_address::encode(address)?;
            self.cached_address.clear();
            self.cached_address.push_str(address);
            self.path = path;
        }
        if self.path.type_code.is_some_and(|kind| kind != 0xd0) {
            return Err("CIP explicit tag type does not match STRING (0x00D0)".into());
        }
        if !self.is_connected() {
            return Err("CIP is not connected".into());
        }
        self.read_buffer.clear();
        let result = (|| {
            let connection = self.connection.as_mut().unwrap();
            if self.path.bytes.len() + 8 > connection.tag_budget() {
                return Err(CipError::local(
                    "CIP tag path does not fit the message budget",
                ));
            }
            loop {
                let service = if self.read_buffer.is_empty() {
                    0x4c
                } else {
                    0x52
                };
                connection.begin_tag(service, &self.path.bytes);
                connection.request.extend_from_slice(&1u16.to_le_bytes());
                if service == 0x52 {
                    connection
                        .request
                        .extend_from_slice(&(self.read_buffer.len() as u32).to_le_bytes());
                }
                let reply = connection.send_tag(service, deadline)?;
                let bytes = &connection.response[reply.data];
                if bytes.len() < 2 {
                    return Err(CipError::protocol("CIP string read omitted its data type"));
                }
                let kind = u16::from_le_bytes([bytes[0], bytes[1]]);
                if kind != 0xd0 {
                    return Err(CipError::protocol(format!(
                        "CIP tag type mismatch: PLC returned {} (0x{kind:04X}), requested STRING (0x00D0)",
                        super::cip_value::type_name(kind)
                    )));
                }
                let payload = &bytes[2..];
                if payload.is_empty() || self.read_buffer.len() + payload.len() > 65_538 {
                    return Err(CipError::protocol(
                        "CIP string read made no progress or exceeded its payload limit",
                    ));
                }
                self.read_buffer.extend_from_slice(payload);
                if reply.status == 0 {
                    break;
                }
            }
            let bytes = &self.read_buffer;
            if bytes.len() < 2 {
                return Err(CipError::protocol("CIP STRING omitted its length prefix"));
            }
            let length = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
            if length > bytes.len() - 2 || bytes[2 + length..].iter().any(|byte| *byte != 0) {
                return Err(CipError::protocol(
                    "CIP STRING length or padding is invalid",
                ));
            }
            if length > byte_limit {
                return Err(CipError::local(format!(
                    "CIP STRING contains {length} UTF-8 bytes, exceeding the requested limit {byte_limit}"
                )));
            }
            std::str::from_utf8(&bytes[2..2 + length])
                .map(str::to_owned)
                .map_err(|_| CipError::local("CIP STRING contains invalid UTF-8"))
        })();
        self.finish(result)
    }

    pub fn write_string(&mut self, _address: &str, _value: &str) -> Operator<String> {
        Operator::err(
            "CIP STRING/UDT writes require an explicit vendor layout and are not supported",
        )
    }
}

impl DeviceBase for CipClient {
    fn connect(&mut self) -> Operator<bool> {
        self.connect()
    }
    fn disconnect(&mut self) -> Operator<bool> {
        self.disconnect()
    }
}
impl<T: CipValue> ReadBase<T> for CipClient {
    fn read(&mut self, address: &str, length: usize) -> Operator<Box<[T]>> {
        self.read(address, length)
    }
}
impl<T: CipValue> WriteBase<T> for CipClient {
    fn write(&mut self, address: &str, value: T) -> Operator<T> {
        self.write(address, value)
    }
    fn write_all(&mut self, address: &str, values: &[T]) -> Operator<usize> {
        self.write_all(address, values)
    }
}

macro_rules! brand {
    ($name:ident, $vendor:ident) => {
        pub struct $name(super::CipClient);
        impl $name {
            pub fn new(
                address: std::net::IpAddr,
                port: u16,
                timeout: crate::communication::timeout::Timeout,
            ) -> Self {
                Self(super::CipClient::new(
                    address,
                    port,
                    super::CipVendor::$vendor,
                    timeout,
                ))
            }
        }
        impl std::ops::Deref for $name {
            type Target = super::CipClient;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
        impl std::ops::DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
        impl crate::communication::device_base::DeviceBase for $name {
            fn connect(&mut self) -> crate::entity::operate::Operator<bool> {
                self.0.connect()
            }
            fn disconnect(&mut self) -> crate::entity::operate::Operator<bool> {
                self.0.disconnect()
            }
        }
        impl<T: super::CipValue> crate::communication::device_base::ReadBase<T> for $name {
            fn read(
                &mut self,
                address: &str,
                length: usize,
            ) -> crate::entity::operate::Operator<Box<[T]>> {
                self.0.read(address, length)
            }
        }
        impl<T: super::CipValue> crate::communication::device_base::WriteBase<T> for $name {
            fn write(&mut self, address: &str, value: T) -> crate::entity::operate::Operator<T> {
                self.0.write(address, value)
            }
            fn write_all(
                &mut self,
                address: &str,
                values: &[T],
            ) -> crate::entity::operate::Operator<usize> {
                self.0.write_all(address, values)
            }
        }
    };
}
pub(crate) use brand;
