//! Stable C ABI for C# P/Invoke. See `src/hooks/CSharp/README.md` for the ABI contract.
//!
//! Handles are registry IDs, not Rust pointers. Operations serialize per handle.
//! Caller-owned buffers must be valid for their declared lengths and must not overlap.
//! Strings passed as addresses/hosts are NUL-terminated UTF-8; payloads use explicit lengths.
//! Numeric payloads are little-endian; bool is one byte, 0 or 1. No allocation crosses the ABI.
//! Unwinding panics become error codes; invalid pointers and aborting panics cannot be caught.

use crate::communication::{
    ethernet::{CipClient, CipOptions, CipValue, CipVendor},
    inovace::{InovanceModbusTcp, InovanceType},
    melsec::{
        MelsecA1EAsciiNet, MelsecA1ENet, MelsecMcAsciiNet, MelsecMcAsciiUdp, MelsecMcNet,
        MelsecMcRNet, MelsecMcUdp,
    },
    modbus::{
        ByteOrder, ModbusAscii, ModbusClient, ModbusRtu, ModbusTcp, ModbusTransport, ModbusUdp,
    },
    omron::{ByteOrder as FinsByteOrder, FinsRoute, OmronFinsTcp, OmronFinsUdp},
    s7::{s7_net::S7Net, s7_type::S7Type},
    timeout::Timeout,
};
use crate::entity::operate::Operator;
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{CStr, c_char},
    io::{self, Read, Write},
    mem::{MaybeUninit, size_of},
    net::IpAddr,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

pub const PLC_OK: i32 = 0;
pub const PLC_INVALID_ARGUMENT: i32 = -1;
pub const PLC_INVALID_HANDLE: i32 = -2;
pub const PLC_BUFFER_TOO_SMALL: i32 = -3;
pub const PLC_NOT_SUPPORTED: i32 = -4;
pub const PLC_OPERATION_FAILED: i32 = -5;
pub const PLC_INTERNAL_ERROR: i32 = -6;
pub const PLC_MAX_BUFFER_BYTES: u32 = 1_048_576;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlcCipOptions {
    pub struct_size: u32,
    pub vendor: u32,
    pub port: u32,
    pub connect_timeout_ms: u32,
    pub receive_timeout_ms: u32,
    pub connected: u32,
    pub connection_size: u32,
    pub packet_interval_us: u32,
    pub timeout_multiplier: u32,
    pub originator_vendor_id: u32,
    pub originator_serial: u32,
}

fn cip_vendor(vendor: u32) -> FfiResult<CipVendor> {
    match vendor {
        1 => Ok(CipVendor::Omron),
        2 => Ok(CipVendor::Melsec),
        3 => Ok(CipVendor::Inovance),
        _ => Err(invalid("Unknown CIP vendor")),
    }
}

impl PlcCipOptions {
    fn for_vendor(vendor: u32) -> FfiResult<Self> {
        let defaults = if cip_vendor(vendor)? == CipVendor::Inovance {
            CipOptions::connected()
        } else {
            CipOptions::default()
        };
        Ok(Self {
            struct_size: size_of::<Self>() as u32,
            vendor,
            port: 44818,
            connect_timeout_ms: 5000,
            receive_timeout_ms: 5000,
            connected: u32::from(defaults.connected),
            connection_size: defaults.connection_size.into(),
            packet_interval_us: defaults.packet_interval_us,
            timeout_multiplier: defaults.timeout_multiplier.into(),
            originator_vendor_id: defaults.originator_vendor_id.into(),
            originator_serial: defaults.originator_serial,
        })
    }
}

/// # Safety
/// Output must point to writable storage of exactly `output_size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_default_cip_options(
    vendor: u32,
    output: *mut PlcCipOptions,
    output_size: u32,
) -> i32 {
    ffi_call(|| {
        require(
            !output.is_null() && output_size == size_of::<PlcCipOptions>() as u32,
            "Invalid CIP options output or size",
        )?;
        unsafe { ptr::write_unaligned(output, PlcCipOptions::for_vendor(vendor)?) };
        Ok(())
    })
}

/// # Safety
/// Options and host must be readable; host is NUL-terminated UTF-8. Route contains
/// `route_length` readable bytes (may be null when zero); output is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_create_cip(
    config: *const PlcCipOptions,
    host: *const c_char,
    route: *const u8,
    route_length: u32,
    output: *mut u64,
) -> i32 {
    ffi_call(|| {
        require(!output.is_null(), "Handle output is null")?;
        unsafe { ptr::write_unaligned(output, 0) };
        require(!config.is_null(), "CIP options are null")?;
        require(
            unsafe { ptr::read_unaligned(config.cast::<u32>()) }
                == size_of::<PlcCipOptions>() as u32,
            "CIP options size does not match this DLL ABI",
        )?;
        let config = unsafe { ptr::read_unaligned(config) };
        let vendor = cip_vendor(config.vendor)?;
        require(config.connected <= 1, "CIP connected must be zero or one")?;
        require(
            route_length <= 500 && route_length % 2 == 0,
            "CIP route must be even and at most 500 bytes",
        )?;
        require(route_length == 0 || !route.is_null(), "CIP route is null")?;
        let route = if route_length == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(route, route_length as usize) }.to_vec()
        };
        let mut common = PlcOptions::for_protocol(config.vendor + 15)?;
        common.connect_timeout_ms = config.connect_timeout_ms;
        common.receive_timeout_ms = config.receive_timeout_ms;
        let port = word(config.port)?;
        require(port != 0, "CIP port must be positive")?;
        let address = unsafe { text(host)? }
            .parse::<IpAddr>()
            .map_err(|_| invalid("Host must be an IP address"))?;
        let mut client = CipClient::new(address, port, vendor, common.timeout()?);
        let options = CipOptions {
            connected: config.connected != 0,
            route,
            connection_size: word(config.connection_size)?,
            packet_interval_us: config.packet_interval_us,
            timeout_multiplier: byte(config.timeout_multiplier)?,
            originator_vendor_id: word(config.originator_vendor_id)?,
            originator_serial: config.originator_serial,
        };
        options.validate().map_err(|error| invalid(&error))?;
        operation(client.set_options(options))?;
        let handle = register(Device::Cip(client))?;
        unsafe { ptr::write_unaligned(output, handle) };
        Ok(())
    })
}

/// All fields are 32-bit integers; initialize with `plc_default_options` before editing.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PlcOptions {
    pub struct_size: u32,
    pub protocol: u32,
    pub port: u32,
    pub connect_timeout_ms: u32,
    pub receive_timeout_ms: u32,
    pub s7_type: u32,
    pub rack: u32,
    pub slot: u32,
    pub use_tsap: u32,
    pub local_tsap: u32,
    pub remote_tsap: u32,
    pub unit_id: u32,
    pub byte_order: u32,
    pub inovance_type: u32,
    pub melsec_network: u32,
    pub melsec_pc: u32,
    pub melsec_io: u32,
    pub melsec_station: u32,
    pub monitoring_timer: u32,
    pub fins_destination_network: u32,
    pub fins_destination_node: u32,
    pub fins_destination_unit: u32,
    pub fins_source_network: u32,
    pub fins_source_node: u32,
    pub fins_source_unit: u32,
    pub fins_gateway_count: u32,
    pub baud_rate: u32,
}

impl PlcOptions {
    fn for_protocol(protocol: u32) -> FfiResult<Self> {
        require((1..=18).contains(&protocol), "Unknown protocol")?;
        Ok(Self {
            struct_size: size_of::<Self>() as u32,
            protocol,
            port: match protocol {
                1 => 102,
                2..=8 => 6000,
                9 | 10 => 9600,
                16..=18 => 44818,
                _ => 502,
            },
            connect_timeout_ms: 5000,
            receive_timeout_ms: 5000,
            s7_type: 0,
            rack: 0,
            slot: 0,
            use_tsap: 0,
            local_tsap: 0,
            remote_tsap: 0,
            unit_id: 1,
            byte_order: if matches!(protocol, 9 | 10 | 15) {
                2
            } else {
                0
            },
            inovance_type: 0,
            melsec_network: 0,
            melsec_pc: 255,
            melsec_io: 0x03ff,
            melsec_station: 0,
            monitoring_timer: 16,
            fins_destination_network: 0,
            fins_destination_node: 0,
            fins_destination_unit: 0,
            fins_source_network: 0,
            fins_source_node: 0,
            fins_source_unit: 0,
            fins_gateway_count: 2,
            baud_rate: 9600,
        })
    }

    fn timeout(self) -> FfiResult<Timeout> {
        require(
            self.connect_timeout_ms > 0 && self.connect_timeout_ms <= i32::MAX as u32,
            "Connect timeout must be 1..=2147483647 milliseconds",
        )?;
        require(
            self.receive_timeout_ms > 0 && self.receive_timeout_ms <= i32::MAX as u32,
            "Receive timeout must be 1..=2147483647 milliseconds",
        )?;
        Ok(Timeout::new(
            self.connect_timeout_ms as i32,
            self.receive_timeout_ms as i32,
        ))
    }

    fn byte_order(self) -> FfiResult<ByteOrder> {
        match self.byte_order {
            0 => Ok(ByteOrder::ABCD),
            1 => Ok(ByteOrder::BADC),
            2 => Ok(ByteOrder::CDAB),
            3 => Ok(ByteOrder::DCBA),
            _ => Err(invalid("Byte order must be 0..=3")),
        }
    }
}

pub type PlcSerialRead = unsafe extern "C" fn(usize, *mut u8, u32) -> i32;
pub type PlcSerialWrite = unsafe extern "C" fn(usize, *const u8, u32) -> i32;
pub type PlcSerialFlush = unsafe extern "C" fn(usize) -> i32;

/// The caller owns the stream and must root callbacks/context until `plc_destroy` returns.
/// Callbacks must not throw, unwind or re-enter this handle. Return -2 for timeout, -1 for failure.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PlcSerialCallbacks {
    pub context: usize,
    pub read: Option<PlcSerialRead>,
    pub write: Option<PlcSerialWrite>,
    pub flush: Option<PlcSerialFlush>,
}

struct CallbackStream(PlcSerialCallbacks);

fn callback_result(result: i32, capacity: usize) -> io::Result<usize> {
    match result {
        -2 => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "C# serial callback timed out",
        )),
        value if value < 0 => Err(io::Error::other("C# serial callback failed")),
        value if value as usize > capacity => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "C# serial callback returned more bytes than requested",
        )),
        value => Ok(value as usize),
    }
}

impl Read for CallbackStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let callback = self
            .0
            .read
            .ok_or_else(|| io::Error::other("Missing serial read callback"))?;
        let length = buffer.len().min(i32::MAX as usize) as u32;
        callback_result(
            unsafe { callback(self.0.context, buffer.as_mut_ptr(), length) },
            length as usize,
        )
    }
}

impl Write for CallbackStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let callback = self
            .0
            .write
            .ok_or_else(|| io::Error::other("Missing serial write callback"))?;
        let length = buffer.len().min(i32::MAX as usize) as u32;
        callback_result(
            unsafe { callback(self.0.context, buffer.as_ptr(), length) },
            length as usize,
        )
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.0.flush {
            Some(callback) => callback_result(unsafe { callback(self.0.context) }, 0).map(|_| ()),
            None => Ok(()),
        }
    }
}

enum Device {
    Cip(CipClient),
    S7(S7Net),
    Mc(MelsecMcNet),
    McAscii(MelsecMcAsciiNet),
    McUdp(MelsecMcUdp),
    McAsciiUdp(MelsecMcAsciiUdp),
    A1e(MelsecA1ENet),
    A1eAscii(MelsecA1EAsciiNet),
    McR(MelsecMcRNet),
    FinsTcp(OmronFinsTcp),
    FinsUdp(OmronFinsUdp),
    ModbusTcp(ModbusTcp),
    ModbusUdp(ModbusUdp),
    ModbusRtu(ModbusRtu<CallbackStream>),
    ModbusAscii(ModbusAscii<CallbackStream>),
    Inovance(InovanceModbusTcp),
}

macro_rules! dispatch {
    ($device:expr, $client:ident, $operation:expr) => {
        dispatch!($device, $client, $operation, $operation)
    };
    ($device:expr, $client:ident, $operation:expr, $cip_operation:expr) => {
        match $device {
            Device::Cip($client) => $cip_operation,
            Device::S7($client) => $operation,
            Device::Mc($client) => $operation,
            Device::McAscii($client) => $operation,
            Device::McUdp($client) => $operation,
            Device::McAsciiUdp($client) => $operation,
            Device::A1e($client) => $operation,
            Device::A1eAscii($client) => $operation,
            Device::McR($client) => $operation,
            Device::FinsTcp($client) => $operation,
            Device::FinsUdp($client) => $operation,
            Device::ModbusTcp($client) => $operation,
            Device::ModbusUdp($client) => $operation,
            Device::ModbusRtu($client) => $operation,
            Device::ModbusAscii($client) => $operation,
            Device::Inovance($client) => $operation,
        }
    };
}

macro_rules! dispatch_bytes {
    ($device:expr, $client:ident, $operation:expr) => {
        match $device {
            Device::Cip($client) => $operation,
            Device::S7($client) => $operation,
            Device::Mc($client) => $operation,
            Device::McAscii($client) => $operation,
            Device::McUdp($client) => $operation,
            Device::McAsciiUdp($client) => $operation,
            Device::A1e($client) => $operation,
            Device::A1eAscii($client) => $operation,
            Device::McR($client) => $operation,
            Device::FinsTcp($client) => $operation,
            Device::FinsUdp($client) => $operation,
            Device::Inovance($client) => $operation,
            _ => {
                return Err(FfiError(
                    PLC_NOT_SUPPORTED,
                    "Modbus does not support u8/i8; use u16 or strings".into(),
                ))
            }
        }
    };
}

#[derive(Debug)]
struct FfiError(i32, String);
type FfiResult<Value> = Result<Value, FfiError>;
type Session = Arc<Mutex<Option<Device>>>;
static SESSIONS: OnceLock<Mutex<HashMap<u64, Session>>> = OnceLock::new();
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);
thread_local! { static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) }; }

fn invalid(message: &str) -> FfiError {
    FfiError(PLC_INVALID_ARGUMENT, message.into())
}

fn require(condition: bool, message: &str) -> FfiResult<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}

fn operation<Value>(result: Operator<Value>) -> FfiResult<Value> {
    if result.is_success {
        result.content.ok_or_else(|| {
            FfiError(
                PLC_INTERNAL_ERROR,
                "Successful operation returned no content".into(),
            )
        })
    } else {
        Err(FfiError(PLC_OPERATION_FAILED, result.msg))
    }
}

fn ffi_call(action: impl FnOnce() -> FfiResult<()>) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(action)).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("Rust panic");
        Err(FfiError(PLC_INTERNAL_ERROR, message.into()))
    });
    match result {
        Ok(()) => {
            LAST_ERROR.with(|error| error.borrow_mut().clear());
            PLC_OK
        }
        Err(FfiError(code, message)) => {
            LAST_ERROR.with(|error| *error.borrow_mut() = message.replace('\0', "\\0"));
            code
        }
    }
}

fn register(device: Device) -> FfiResult<u64> {
    let handle = NEXT_HANDLE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| FfiError(PLC_INTERNAL_ERROR, "Handle space exhausted".into()))?;
    SESSIONS
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| FfiError(PLC_INTERNAL_ERROR, "Handle registry is poisoned".into()))?
        .insert(handle, Arc::new(Mutex::new(Some(device))));
    Ok(handle)
}

fn with_device<Value>(
    handle: u64,
    action: impl FnOnce(&mut Device) -> FfiResult<Value>,
) -> FfiResult<Value> {
    let session = SESSIONS
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| FfiError(PLC_INTERNAL_ERROR, "Handle registry is poisoned".into()))?
        .get(&handle)
        .cloned()
        .ok_or_else(|| FfiError(PLC_INVALID_HANDLE, "Unknown or released handle".into()))?;
    let mut guard = session.lock().map_err(|_| {
        FfiError(
            PLC_INTERNAL_ERROR,
            "Session is poisoned; destroy this handle".into(),
        )
    })?;
    action(
        guard
            .as_mut()
            .ok_or_else(|| FfiError(PLC_INVALID_HANDLE, "Handle was released".into()))?,
    )
}

unsafe fn text<'value>(value: *const c_char) -> FfiResult<&'value str> {
    require(!value.is_null(), "Text pointer is null")?;
    let text = unsafe { CStr::from_ptr(value) }
        .to_str()
        .map_err(|_| invalid("Text must be UTF-8"))?;
    require(!text.is_empty(), "Text must not be empty")?;
    Ok(text)
}

unsafe fn options(value: *const PlcOptions) -> FfiResult<PlcOptions> {
    require(!value.is_null(), "Options pointer is null")?;
    require(
        unsafe { ptr::read_unaligned(value.cast::<u32>()) } == size_of::<PlcOptions>() as u32,
        "Options size does not match this DLL ABI",
    )?;
    let options = unsafe { ptr::read_unaligned(value) };
    require((1..=18).contains(&options.protocol), "Unknown protocol")?;
    options.timeout()?;
    options.byte_order()?;
    Ok(options)
}

fn byte(value: u32) -> FfiResult<u8> {
    u8::try_from(value).map_err(|_| invalid("An 8-bit option is outside 0..=255"))
}

fn word(value: u32) -> FfiResult<u16> {
    u16::try_from(value).map_err(|_| invalid("A 16-bit option is outside 0..=65535"))
}

fn configure_modbus<Transport: ModbusTransport>(
    mut client: ModbusClient<Transport>,
    options: PlcOptions,
) -> FfiResult<ModbusClient<Transport>> {
    operation(client.set_unit_id(byte(options.unit_id)?))?;
    client.set_byte_order(options.byte_order()?);
    Ok(client)
}

fn network_device(options: PlcOptions, address: IpAddr) -> FfiResult<Device> {
    let port = word(options.port)?;
    require(port != 0, "Port must be greater than zero")?;
    let timeout = options.timeout()?;
    macro_rules! melsec {
        ($kind:ident, $client:ty) => {
            Device::$kind(
                <$client>::new(address, port, timeout)
                    .with_route(
                        byte(options.melsec_network)?,
                        byte(options.melsec_pc)?,
                        word(options.melsec_io)?,
                        byte(options.melsec_station)?,
                    )
                    .with_monitoring_timer(word(options.monitoring_timer)?),
            )
        };
    }
    macro_rules! fins {
        ($kind:ident, $client:ty) => {{
            let mut client = <$client>::new(address, port, timeout);
            operation(client.set_route(FinsRoute {
                destination_network: byte(options.fins_destination_network)?,
                destination_node: byte(options.fins_destination_node)?,
                destination_unit: byte(options.fins_destination_unit)?,
                source_network: byte(options.fins_source_network)?,
                source_node: byte(options.fins_source_node)?,
                source_unit: byte(options.fins_source_unit)?,
                gateway_count: byte(options.fins_gateway_count)?,
            }))?;
            client.set_byte_order(match options.byte_order()? {
                ByteOrder::ABCD => FinsByteOrder::ABCD,
                ByteOrder::BADC => FinsByteOrder::BADC,
                ByteOrder::CDAB => FinsByteOrder::CDAB,
                ByteOrder::DCBA => FinsByteOrder::DCBA,
            });
            Device::$kind(client)
        }};
    }
    Ok(match options.protocol {
        1 => {
            require(
                options.rack <= 7 && options.slot <= 31,
                "Rack must be 0..=7; slot must be 0..=31",
            )?;
            require(options.use_tsap <= 1, "use_tsap must be 0 or 1")?;
            let kind = match options.s7_type {
                0 => S7Type::S1200,
                1 => S7Type::S1500,
                2 => S7Type::S300,
                3 => S7Type::S400,
                4 => S7Type::S200,
                5 => S7Type::S200Smart,
                _ => return Err(invalid("Unknown S7 CPU type")),
            };
            let mut client = S7Net::new(
                address,
                port,
                kind,
                timeout,
                options.rack as usize,
                options.slot as usize,
            );
            if options.use_tsap == 1 {
                client = client.with_tsap(word(options.local_tsap)?, word(options.remote_tsap)?);
            }
            Device::S7(client)
        }
        2 => melsec!(Mc, MelsecMcNet),
        3 => melsec!(McAscii, MelsecMcAsciiNet),
        4 => melsec!(McUdp, MelsecMcUdp),
        5 => melsec!(McAsciiUdp, MelsecMcAsciiUdp),
        6 => melsec!(A1e, MelsecA1ENet),
        7 => melsec!(A1eAscii, MelsecA1EAsciiNet),
        8 => melsec!(McR, MelsecMcRNet),
        9 => fins!(FinsTcp, OmronFinsTcp),
        10 => fins!(FinsUdp, OmronFinsUdp),
        11 => Device::ModbusTcp(configure_modbus(
            ModbusTcp::new(address, port, timeout),
            options,
        )?),
        12 => Device::ModbusUdp(configure_modbus(
            ModbusUdp::new(address, port, timeout),
            options,
        )?),
        15 => {
            let series = match options.inovance_type {
                0 => InovanceType::AM,
                1 => InovanceType::AC,
                2 => InovanceType::AP,
                3 => InovanceType::EVO,
                4 => InovanceType::H5U,
                5 => InovanceType::H3U,
                6 => InovanceType::Easy,
                _ => return Err(invalid("Unknown Inovance series")),
            };
            let mut client = InovanceModbusTcp::new(address, port, series, timeout);
            operation(client.set_unit_id(byte(options.unit_id)?))?;
            client.set_byte_order(options.byte_order()?);
            Device::Inovance(client)
        }
        16..=18 => Device::Cip(CipClient::new(
            address,
            port,
            cip_vendor(options.protocol - 15)?,
            timeout,
        )),
        _ => return Err(invalid("RTU/ASCII require plc_create_serial")),
    })
}

trait AbiValue: Sized + CipValue {
    const SIZE: usize;
    fn decode(bytes: &[u8]) -> FfiResult<Self>;
    fn encode(&self, output: &mut [MaybeUninit<u8>]);
    fn read(device: &mut Device, address: &str, count: usize) -> FfiResult<Box<[Self]>>;
    fn write(device: &mut Device, address: &str, values: &[Self]) -> FfiResult<()>;
}

macro_rules! abi_number {
    ($value:ty, $dispatch:ident) => {
        impl AbiValue for $value {
            const SIZE: usize = size_of::<Self>();
            fn decode(bytes: &[u8]) -> FfiResult<Self> {
                Ok(Self::from_le_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| invalid("Invalid numeric byte length"))?,
                ))
            }
            fn encode(&self, output: &mut [MaybeUninit<u8>]) {
                for (destination, byte) in output.iter_mut().zip(self.to_le_bytes()) {
                    destination.write(byte);
                }
            }
            fn read(device: &mut Device, address: &str, count: usize) -> FfiResult<Box<[Self]>> {
                operation($dispatch!(
                    device,
                    client,
                    client.read::<Self>(address, count)
                ))
            }
            fn write(device: &mut Device, address: &str, values: &[Self]) -> FfiResult<()> {
                if values.len() == 1 {
                    operation($dispatch!(
                        device,
                        client,
                        client.write::<Self>(address, values[0])
                    ))?;
                } else {
                    operation($dispatch!(
                        device,
                        client,
                        client.write_all::<Self>(address, values)
                    ))?;
                }
                Ok(())
            }
        }
    };
}

abi_number!(u8, dispatch_bytes);
abi_number!(i8, dispatch_bytes);
abi_number!(u16, dispatch);
abi_number!(i16, dispatch);
abi_number!(u32, dispatch);
abi_number!(i32, dispatch);
abi_number!(u64, dispatch);
abi_number!(i64, dispatch);
abi_number!(f32, dispatch);
abi_number!(f64, dispatch);

impl AbiValue for bool {
    const SIZE: usize = 1;
    fn decode(bytes: &[u8]) -> FfiResult<Self> {
        match bytes {
            [0] => Ok(false),
            [1] => Ok(true),
            _ => Err(invalid("Bool payload must contain only 0 or 1")),
        }
    }
    fn encode(&self, output: &mut [MaybeUninit<u8>]) {
        output[0].write(u8::from(*self));
    }
    fn read(device: &mut Device, address: &str, count: usize) -> FfiResult<Box<[Self]>> {
        operation(dispatch!(
            device,
            client,
            client.read::<bool>(address, count)
        ))
    }
    fn write(device: &mut Device, address: &str, values: &[Self]) -> FfiResult<()> {
        if values.len() == 1 {
            operation(dispatch!(device, client, client.write(address, values[0])))?;
        } else {
            operation(dispatch!(device, client, client.write_all(address, values)))?;
        }
        Ok(())
    }
}

fn data_length(data_type: u32, count: u32) -> FfiResult<u32> {
    let width = match data_type {
        1..=3 => 1,
        4 | 5 => 2,
        6 | 7 | 10 => 4,
        8 | 9 | 11 => 8,
        _ => return Err(invalid("Unknown data type")),
    };
    let length = count
        .checked_mul(width)
        .ok_or_else(|| invalid("Element count overflow"))?;
    require(
        length > 0 && length <= PLC_MAX_BUFFER_BYTES,
        "Payload must be 1..=1048576 bytes",
    )?;
    Ok(length)
}

fn read_values<Value: AbiValue>(
    device: &mut Device,
    address: &str,
    output: &mut [MaybeUninit<u8>],
) -> FfiResult<()> {
    let count = output.len() / Value::SIZE;
    if let Device::Cip(client) = device {
        let bytes = client
            .read_raw::<Value>(address, count)
            .map_err(|error| FfiError(PLC_OPERATION_FAILED, error))?;
        for (source, destination) in bytes.iter().zip(output) {
            destination.write(if Value::TYPE_CODE == 0xc1 {
                u8::from(*source != 0)
            } else {
                *source
            });
        }
        return Ok(());
    }
    let values = Value::read(device, address, count)?;
    if values.len() != count {
        return Err(FfiError(
            PLC_INTERNAL_ERROR,
            "Protocol returned an unexpected element count".into(),
        ));
    }
    for (value, destination) in values.iter().zip(output.chunks_exact_mut(Value::SIZE)) {
        value.encode(destination);
    }
    Ok(())
}

fn write_values<Value: AbiValue>(
    device: &mut Device,
    address: &str,
    bytes: &[u8],
) -> FfiResult<()> {
    if let Device::Cip(client) = device {
        if Value::TYPE_CODE == 0xc1 && bytes.iter().any(|value| *value > 1) {
            return Err(invalid("Bool payload must contain only 0 or 1"));
        }
        return client
            .write_raw::<Value>(address, bytes)
            .map(|_| ())
            .map_err(|error| FfiError(PLC_OPERATION_FAILED, error));
    }
    if bytes.len() == Value::SIZE {
        let value = <Value as AbiValue>::decode(bytes)?;
        return Value::write(device, address, std::slice::from_ref(&value));
    }
    let values = bytes
        .chunks_exact(Value::SIZE)
        .map(<Value as AbiValue>::decode)
        .collect::<FfiResult<Vec<_>>>()?;
    Value::write(device, address, &values)
}

macro_rules! data_dispatch {
    ($kind:expr, $function:ident, $device:expr, $address:expr, $argument:expr) => {
        match $kind {
            1 => $function::<bool>($device, $address, $argument),
            2 => $function::<u8>($device, $address, $argument),
            3 => $function::<i8>($device, $address, $argument),
            4 => $function::<u16>($device, $address, $argument),
            5 => $function::<i16>($device, $address, $argument),
            6 => $function::<u32>($device, $address, $argument),
            7 => $function::<i32>($device, $address, $argument),
            8 => $function::<u64>($device, $address, $argument),
            9 => $function::<i64>($device, $address, $argument),
            10 => $function::<f32>($device, $address, $argument),
            11 => $function::<f64>($device, $address, $argument),
            _ => Err(invalid("Unknown data type")),
        }
    };
}

unsafe fn prepare_output(
    output: *mut u8,
    capacity: u32,
    written: *mut u32,
    required: u32,
) -> FfiResult<()> {
    require(!written.is_null(), "Byte count output is null")?;
    unsafe { ptr::write_unaligned(written, 0) };
    if output.is_null() || capacity < required {
        unsafe { ptr::write_unaligned(written, required) };
        return Err(FfiError(
            PLC_BUFFER_TOO_SMALL,
            format!("Output requires {required} bytes; no PLC request was sent"),
        ));
    }
    Ok(())
}

unsafe fn input_bytes<'value>(input: *const u8, length: u32) -> FfiResult<&'value [u8]> {
    require(
        length <= PLC_MAX_BUFFER_BYTES,
        "Payload exceeds 1048576 bytes",
    )?;
    if length == 0 {
        return Ok(&[]);
    }
    require(!input.is_null(), "Input buffer is null")?;
    Ok(unsafe { std::slice::from_raw_parts(input, length as usize) })
}

#[unsafe(no_mangle)]
pub extern "C" fn plc_abi_version() -> u32 {
    1
}

/// # Safety
/// `output` must point to writable storage for one `PlcOptions`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_default_options(
    protocol: u32,
    output: *mut PlcOptions,
    output_size: u32,
) -> i32 {
    ffi_call(|| {
        require(!output.is_null(), "Options output is null")?;
        require(
            output_size == size_of::<PlcOptions>() as u32,
            "Options output size does not match this DLL ABI",
        )?;
        unsafe { ptr::write_unaligned(output, PlcOptions::for_protocol(protocol)?) };
        Ok(())
    })
}

/// # Safety
/// Options/host must be readable; host is NUL-terminated UTF-8. Handle output is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_create(
    config: *const PlcOptions,
    host: *const c_char,
    output: *mut u64,
) -> i32 {
    ffi_call(|| {
        require(!output.is_null(), "Handle output is null")?;
        unsafe { ptr::write_unaligned(output, 0) };
        let config = unsafe { options(config)? };
        let address = unsafe { text(host)? }
            .parse::<IpAddr>()
            .map_err(|_| invalid("Host must be an IP address"))?;
        let handle = register(network_device(config, address)?)?;
        unsafe { ptr::write_unaligned(output, handle) };
        Ok(())
    })
}

/// # Safety
/// All pointers must be valid. Callback/context lifetime extends until destruction returns.
/// The caller configures and closes the serial stream and guarantees callback thread safety.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_create_serial(
    config: *const PlcOptions,
    callbacks: *const PlcSerialCallbacks,
    output: *mut u64,
) -> i32 {
    ffi_call(|| {
        require(!output.is_null(), "Handle output is null")?;
        unsafe { ptr::write_unaligned(output, 0) };
        let config = unsafe { options(config)? };
        require(
            matches!(config.protocol, 13 | 14),
            "Serial creation requires RTU or ASCII",
        )?;
        require(!callbacks.is_null(), "Serial callbacks are null")?;
        let callbacks = unsafe { ptr::read_unaligned(callbacks) };
        require(
            callbacks.read.is_some() && callbacks.write.is_some(),
            "Read and write callbacks are required",
        )?;
        require(
            config.protocol != 13 || config.baud_rate != 0,
            "RTU baud rate must be greater than zero",
        )?;
        let unit = byte(config.unit_id)?;
        let stream = CallbackStream(callbacks);
        let device = if config.protocol == 13 {
            Device::ModbusRtu(configure_modbus(
                ModbusRtu::new(stream, unit, config.baud_rate),
                config,
            )?)
        } else {
            Device::ModbusAscii(configure_modbus(ModbusAscii::new(stream, unit), config)?)
        };
        unsafe { ptr::write_unaligned(output, register(device)?) };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn plc_connect(handle: u64) -> i32 {
    ffi_call(|| {
        with_device(handle, |device| {
            operation(dispatch!(device, client, client.connect())).map(|_| ())
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn plc_disconnect(handle: u64) -> i32 {
    ffi_call(|| {
        with_device(handle, |device| {
            operation(dispatch!(device, client, client.disconnect())).map(|_| ())
        })
    })
}

/// # Safety
/// `output` must point to one writable byte. UDP status indicates only local readiness.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_is_connected(handle: u64, output: *mut u8) -> i32 {
    ffi_call(|| {
        require(!output.is_null(), "Status output is null")?;
        unsafe { ptr::write(output, 0) };
        let connected = with_device(handle, |device| {
            Ok(dispatch!(device, client, client.is_connected()))
        })?;
        unsafe { ptr::write(output, u8::from(connected)) };
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn plc_destroy(handle: u64) -> i32 {
    ffi_call(|| {
        let session = SESSIONS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| FfiError(PLC_INTERNAL_ERROR, "Handle registry is poisoned".into()))?
            .remove(&handle)
            .ok_or_else(|| FfiError(PLC_INVALID_HANDLE, "Unknown or released handle".into()))?;
        let mut guard = session.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(mut device) = guard.take() {
            operation(dispatch!(&mut device, client, client.disconnect()))?;
        }
        Ok(())
    })
}

/// Reads numeric elements into a little-endian byte buffer; no terminating NUL is added.
/// # Safety
/// Address must be NUL-terminated UTF-8. Output/written must be valid, writable and non-overlapping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_read(
    handle: u64,
    address: *const c_char,
    data_type: u32,
    count: u32,
    output: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> i32 {
    ffi_call(|| {
        require(!written.is_null(), "Byte count output is null")?;
        unsafe { ptr::write_unaligned(written, 0) };
        let required = data_length(data_type, count)?;
        let address = unsafe { text(address)? };
        unsafe { prepare_output(output, capacity, written, required)? };
        let destination = unsafe {
            std::slice::from_raw_parts_mut(output.cast::<MaybeUninit<u8>>(), required as usize)
        };
        with_device(handle, |device| {
            data_dispatch!(data_type, read_values, device, address, destination)
        })?;
        unsafe {
            ptr::write_unaligned(written, required);
        }
        Ok(())
    })
}

/// Writes a single value or array. No retry is performed; failure may mean a partial write.
/// # Safety
/// Address must be NUL-terminated UTF-8. Input must reference exactly `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_write(
    handle: u64,
    address: *const c_char,
    data_type: u32,
    count: u32,
    input: *const u8,
    length: u32,
) -> i32 {
    ffi_call(|| {
        require(
            data_length(data_type, count)? == length,
            "Input byte length does not match type and count",
        )?;
        let address = unsafe { text(address)? };
        let bytes = unsafe { input_bytes(input, length)? };
        with_device(handle, |device| {
            data_dispatch!(data_type, write_values, device, address, bytes)
        })
    })
}

/// Kind 0 reads raw UTF-8, or one Omron CIP STRING with `byte_length` as its output limit.
/// Kind 1 reads S7 STRING (byte_length must be zero).
/// S7 STRING requires a 254-byte output buffer before any PLC I/O; written is its actual UTF-8 size.
/// # Safety
/// Address/output/written follow the same pointer contract as `plc_read`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_read_string(
    handle: u64,
    address: *const c_char,
    kind: u32,
    byte_length: u32,
    output: *mut u8,
    capacity: u32,
    written: *mut u32,
) -> i32 {
    ffi_call(|| {
        require(!written.is_null(), "Byte count output is null")?;
        unsafe { ptr::write_unaligned(written, 0) };
        let required = match kind {
            0 => {
                require(
                    byte_length > 0 && byte_length <= PLC_MAX_BUFFER_BYTES,
                    "Raw string length must be 1..=1048576 bytes",
                )?;
                byte_length
            }
            1 => {
                require(
                    byte_length == 0,
                    "S7 STRING length is determined by its header; pass zero",
                )?;
                254
            }
            _ => return Err(invalid("Unknown string kind")),
        };
        let address = unsafe { text(address)? };
        unsafe { prepare_output(output, capacity, written, required)? };
        let value = with_device(handle, |device| {
            if let Device::Cip(client) = device {
                if kind != 0 || client.vendor() != CipVendor::Omron {
                    return Err(FfiError(
                        PLC_NOT_SUPPORTED,
                        "CIP string reading requires Omron STRING (0x00D0); S7 STRING and UDT layouts are not supported".into(),
                    ));
                }
                return operation(client.read_string(address, byte_length as usize));
            }
            if kind == 0 {
                operation(dispatch!(
                    device,
                    client,
                    client.read_string(address, byte_length as usize)
                ))
            } else {
                match device {
                    Device::S7(client) => operation(client.read_s7_string(address)),
                    _ => Err(FfiError(
                        PLC_NOT_SUPPORTED,
                        "S7 STRING requires an S7 client".into(),
                    )),
                }
            }
        })?;
        if value.len() > required as usize {
            return Err(FfiError(
                PLC_INTERNAL_ERROR,
                "Protocol string exceeded the validated output size".into(),
            ));
        }
        unsafe {
            ptr::copy_nonoverlapping(value.as_ptr(), output, value.len());
            ptr::write_unaligned(written, value.len() as u32);
        }
        Ok(())
    })
}

/// Kind 0 writes raw UTF-8; kind 1 writes an S7 length-prefixed STRING. Embedded NUL is preserved.
/// # Safety
/// Address is NUL-terminated UTF-8; input points to `byte_length` UTF-8 bytes (null is allowed for zero).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_write_string(
    handle: u64,
    address: *const c_char,
    kind: u32,
    input: *const u8,
    byte_length: u32,
) -> i32 {
    ffi_call(|| {
        require(kind <= 1, "Unknown string kind")?;
        require(
            kind != 0 || byte_length > 0,
            "Raw string writes must not be empty",
        )?;
        require(
            kind != 1 || byte_length <= 254,
            "S7 STRING supports at most 254 UTF-8 bytes",
        )?;
        let address = unsafe { text(address)? };
        let bytes = unsafe { input_bytes(input, byte_length)? };
        let value =
            std::str::from_utf8(bytes).map_err(|_| invalid("String payload must be UTF-8"))?;
        with_device(handle, |device| {
            if matches!(device, Device::Cip(_)) {
                return Err(FfiError(
                    PLC_NOT_SUPPORTED,
                    "CIP STRING/UDT layout is not supported; use typed atomic tags".into(),
                ));
            }
            if kind == 1 {
                match device {
                    Device::S7(client) => {
                        operation(client.write::<String>(address, value)).map(|_| ())
                    }
                    _ => Err(FfiError(
                        PLC_NOT_SUPPORTED,
                        "S7 STRING requires an S7 client".into(),
                    )),
                }
            } else if let Device::S7(client) = device {
                operation(client.write_all(address, bytes)).map(|_| ())
            } else {
                operation(dispatch!(
                    device,
                    client,
                    client.write::<String>(address, value.to_owned()),
                    client.write_string(address, value)
                ))
                .map(|_| ())
            }
        })
    })
}

/// Returns required UTF-8 capacity including NUL without clearing the thread-local error.
/// # Safety
/// A non-null output must reference `capacity` writable bytes. Null queries the required size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plc_last_error(output: *mut u8, capacity: u32) -> u32 {
    catch_unwind(AssertUnwindSafe(|| {
        LAST_ERROR.with(|error| {
            let error = error.borrow();
            let required = (error.len() + 1) as u32;
            if !output.is_null() && capacity >= required {
                unsafe {
                    ptr::copy_nonoverlapping(error.as_ptr(), output, error.len());
                    ptr::write(output.add(error.len()), 0);
                }
            }
            required
        })
    }))
    .unwrap_or(0)
}

#[cfg(test)]
#[path = "csharp_dll_tests.rs"]
mod tests;
