use super::*;
use std::{io::Cursor, net::TcpListener, thread, time::Duration};

fn create(protocol: u32) -> u64 {
    let options = PlcOptions::for_protocol(protocol).unwrap();
    let mut handle = 0;
    assert_eq!(
        unsafe { plc_create(&options, c"127.0.0.1".as_ptr(), &mut handle) },
        PLC_OK
    );
    handle
}

fn error_text() -> String {
    let length = unsafe { plc_last_error(ptr::null_mut(), 0) };
    let mut bytes = vec![0; length as usize];
    assert_eq!(
        unsafe { plc_last_error(bytes.as_mut_ptr(), length) },
        length
    );
    bytes.pop();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn options_layout_and_network_protocols_are_available_without_connecting() {
    assert_eq!(plc_abi_version(), 1);
    assert_eq!(size_of::<PlcOptions>(), 108);
    assert_eq!(size_of::<PlcSerialCallbacks>(), 4 * size_of::<usize>());
    for protocol in (1..=12).chain([15]) {
        let handle = create(protocol);
        let mut connected = 1;
        assert_eq!(unsafe { plc_is_connected(handle, &mut connected) }, PLC_OK);
        assert_eq!(connected, 0);
        assert_eq!(plc_disconnect(handle), PLC_OK);
        assert_eq!(plc_destroy(handle), PLC_OK);
        assert_eq!(plc_destroy(handle), PLC_INVALID_HANDLE);
        assert_eq!(plc_connect(handle), PLC_INVALID_HANDLE);
    }
}

#[test]
fn all_cpu_models_are_constructible() {
    for s7_type in 0..=5 {
        let mut options = PlcOptions::for_protocol(1).unwrap();
        options.s7_type = s7_type;
        options.use_tsap = 1;
        options.local_tsap = 0x100;
        options.remote_tsap = 0x300;
        let mut handle = 0;
        assert_eq!(
            unsafe { plc_create(&options, c"127.0.0.1".as_ptr(), &mut handle) },
            PLC_OK
        );
        assert_eq!(plc_destroy(handle), PLC_OK);
    }
    for series in 0..=6 {
        let mut options = PlcOptions::for_protocol(15).unwrap();
        options.inovance_type = series;
        let mut handle = 0;
        assert_eq!(
            unsafe { plc_create(&options, c"127.0.0.1".as_ptr(), &mut handle) },
            PLC_OK
        );
        assert_eq!(plc_destroy(handle), PLC_OK);
    }
}

#[test]
fn invalid_configuration_and_null_pointers_are_rejected() {
    let mut options = PlcOptions::for_protocol(1).unwrap();
    let mut handle = 42;
    unsafe {
        assert_eq!(
            plc_default_options(1, ptr::null_mut(), 108),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_default_options(1, &mut options, 4),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_default_options(99, &mut options, 108),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_create(ptr::null(), c"127.0.0.1".as_ptr(), &mut handle),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(handle, 0);
        assert_eq!(
            plc_create(&options, ptr::null(), &mut handle),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_create(&options, c"not-an-ip".as_ptr(), &mut handle),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_create(&options, c"127.0.0.1".as_ptr(), ptr::null_mut()),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_create(&options, [0xff_u8, 0].as_ptr().cast(), &mut handle),
            PLC_INVALID_ARGUMENT
        );
        options.struct_size = 0;
        assert_eq!(
            plc_create(&options, c"127.0.0.1".as_ptr(), &mut handle),
            PLC_INVALID_ARGUMENT
        );
        options.struct_size = 108;
        options.rack = 8;
        assert_eq!(
            plc_create(&options, c"127.0.0.1".as_ptr(), &mut handle),
            PLC_INVALID_ARGUMENT
        );
        options.rack = 0;
        options.connect_timeout_ms = u32::MAX;
        assert_eq!(
            plc_create(&options, c"127.0.0.1".as_ptr(), &mut handle),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(plc_is_connected(0, ptr::null_mut()), PLC_INVALID_ARGUMENT);
    }
}

#[test]
fn buffer_queries_and_invalid_payloads_do_not_access_the_plc() {
    let handle = create(11);
    let mut written = 99;
    let mut output = [0xaa_u8; 8];
    unsafe {
        assert_eq!(
            plc_read(
                handle,
                c"HR0".as_ptr(),
                4,
                2,
                output.as_mut_ptr(),
                1,
                &mut written
            ),
            PLC_BUFFER_TOO_SMALL
        );
        assert_eq!(written, 4);
        assert_eq!(output, [0xaa; 8]);
        assert_eq!(
            plc_read(
                handle,
                c"HR0".as_ptr(),
                4,
                2,
                ptr::null_mut(),
                0,
                &mut written
            ),
            PLC_BUFFER_TOO_SMALL
        );
        assert_eq!(written, 4);
        assert_eq!(
            plc_read(
                handle,
                c"HR0".as_ptr(),
                4,
                2,
                output.as_mut_ptr(),
                8,
                &mut written
            ),
            PLC_OPERATION_FAILED
        );
        assert_eq!(written, 0);
        assert_eq!(output, [0xaa; 8]);
        assert_eq!(
            plc_read(
                handle,
                c"HR0".as_ptr(),
                8,
                u32::MAX,
                output.as_mut_ptr(),
                8,
                &mut written
            ),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_read(
                handle,
                c"HR0".as_ptr(),
                99,
                1,
                output.as_mut_ptr(),
                8,
                &mut written
            ),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_read(
                handle,
                c"HR0".as_ptr(),
                2,
                1,
                output.as_mut_ptr(),
                8,
                &mut written
            ),
            PLC_NOT_SUPPORTED
        );
        assert_eq!(
            plc_write(handle, c"C0".as_ptr(), 1, 1, [2].as_ptr(), 1),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_write(handle, c"HR0".as_ptr(), 4, 2, output.as_ptr(), 1),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_write(handle, c"HR0".as_ptr(), 4, 1, ptr::null(), 2),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_write_string(handle, c"HR0".as_ptr(), 0, [0xff].as_ptr(), 1),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_write_string(handle, c"HR0".as_ptr(), 0, ptr::null(), 0),
            PLC_INVALID_ARGUMENT
        );
        assert_eq!(
            plc_write_string(handle, c"HR0".as_ptr(), 1, ptr::null(), 0),
            PLC_NOT_SUPPORTED
        );
        assert_eq!(
            plc_read_string(
                handle,
                c"HR0".as_ptr(),
                1,
                0,
                output.as_mut_ptr(),
                8,
                &mut written
            ),
            PLC_BUFFER_TOO_SMALL
        );
        assert_eq!(written, 254);
    }
    assert_eq!(plc_destroy(handle), PLC_OK);
}

#[test]
fn numeric_payloads_are_little_endian_and_preserve_precision() {
    macro_rules! roundtrip {
        ($value:expr) => {{
            let value = $value;
            let mut bytes = Vec::new();
            AbiValue::encode(&value, &mut bytes);
            assert_eq!(bytes, value.to_le_bytes());
            let decoded = <_ as AbiValue>::decode(&bytes).unwrap();
            assert_eq!(value, decoded);
        }};
    }
    roundtrip!(u8::MAX);
    roundtrip!(i8::MIN);
    roundtrip!(u16::MAX);
    roundtrip!(i16::MIN);
    roundtrip!(u32::MAX);
    roundtrip!(i32::MIN);
    roundtrip!(u64::MAX);
    roundtrip!(i64::MIN);
    roundtrip!(123.25_f32);
    roundtrip!(-123.25_f64);
    assert!(<bool as AbiValue>::decode(&[1]).unwrap());
    assert!(!<bool as AbiValue>::decode(&[0]).unwrap());
    assert!(<bool as AbiValue>::decode(&[2]).is_err());
}

#[test]
fn errors_are_thread_local_and_small_error_buffers_remain_unchanged() {
    assert_eq!(
        ffi_call(|| Err(invalid("main error"))),
        PLC_INVALID_ARGUMENT
    );
    let mut small = [0xaa_u8; 2];
    assert_eq!(unsafe { plc_last_error(small.as_mut_ptr(), 2) }, 11);
    assert_eq!(small, [0xaa; 2]);
    let other = thread::spawn(|| {
        assert_eq!(
            ffi_call(|| Err(invalid("worker error"))),
            PLC_INVALID_ARGUMENT
        );
        error_text()
    });
    assert_eq!(other.join().unwrap(), "worker error");
    assert_eq!(error_text(), "main error");
    assert_eq!(ffi_call(|| Ok(())), PLC_OK);
    assert_eq!(error_text(), "");
}

#[test]
fn panic_is_contained_and_poisoned_handles_can_be_destroyed() {
    let handle = create(11);
    assert_eq!(
        ffi_call(|| with_device(handle, |_| panic!("simulated failure"))),
        PLC_INTERNAL_ERROR
    );
    assert!(error_text().contains("simulated failure"));
    assert_eq!(plc_connect(handle), PLC_INTERNAL_ERROR);
    assert_eq!(plc_destroy(handle), PLC_OK);
    assert_eq!(plc_destroy(handle), PLC_INVALID_HANDLE);
}

#[test]
fn shared_handles_serialize_and_reject_use_after_destroy() {
    let handle = create(11);
    let workers: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(move || {
                for _ in 0..50 {
                    let mut connected = 1;
                    assert_eq!(unsafe { plc_is_connected(handle, &mut connected) }, PLC_OK);
                    assert_eq!(connected, 0);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let session = SESSIONS
        .get()
        .unwrap()
        .lock()
        .unwrap()
        .get(&handle)
        .unwrap()
        .clone();
    assert_eq!(plc_destroy(handle), PLC_OK);
    assert!(session.lock().unwrap().is_none());
    assert_eq!(plc_connect(handle), PLC_INVALID_HANDLE);
}

#[test]
fn modbus_tcp_ffi_reads_and_writes_arrays_strings_and_station_overrides() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let exchanges = [
            (
                1,
                vec![3, 0, 0, 0, 4],
                vec![3, 8, 255, 255, 255, 255, 255, 255, 255, 255],
            ),
            (1, vec![6, 0, 4, 0x12, 0x34], vec![6, 0, 4, 0x12, 0x34]),
            (1, vec![16, 0, 5, 0, 2, 4, 0, 3, 0, 4], vec![16, 0, 5, 0, 2]),
            (
                1,
                vec![16, 0, 10, 0, 2, 4, 0xe4, 0xb8, 0xad, 0],
                vec![16, 0, 10, 0, 2],
            ),
            (2, vec![3, 0, 10, 0, 2], vec![3, 4, 0xe4, 0xb8, 0xad, 0]),
            (1, vec![1, 0, 0, 0, 3], vec![1, 1, 5]),
        ];
        for (unit, expected, reply) in exchanges {
            let mut header = [0; 7];
            stream.read_exact(&mut header).unwrap();
            assert_eq!(header[6], unit);
            let mut request = vec![0; u16::from_be_bytes([header[4], header[5]]) as usize - 1];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(request, expected);
            header[4..6].copy_from_slice(&((reply.len() + 1) as u16).to_be_bytes());
            stream.write_all(&header).unwrap();
            stream.write_all(&reply).unwrap();
        }
    });
    let mut options = PlcOptions::for_protocol(11).unwrap();
    options.port = u32::from(port);
    let mut handle = 0;
    assert_eq!(
        unsafe { plc_create(&options, c"127.0.0.1".as_ptr(), &mut handle) },
        PLC_OK
    );
    assert_eq!(plc_connect(handle), PLC_OK);
    let mut output = [0; 8];
    let mut written = 0;
    unsafe {
        assert_eq!(
            plc_read(
                handle,
                c"HR0".as_ptr(),
                8,
                1,
                output.as_mut_ptr(),
                8,
                &mut written
            ),
            PLC_OK
        );
        assert_eq!(u64::from_le_bytes(output), u64::MAX);
        assert_eq!(written, 8);
        assert_eq!(
            plc_write(handle, c"HR4".as_ptr(), 4, 1, [0x34, 0x12].as_ptr(), 2),
            PLC_OK
        );
        assert_eq!(
            plc_write(handle, c"HR5".as_ptr(), 4, 2, [3, 0, 4, 0].as_ptr(), 4),
            PLC_OK
        );
        assert_eq!(
            plc_write_string(handle, c"HR10".as_ptr(), 0, "中\0".as_ptr(), 4),
            PLC_OK
        );
        assert_eq!(
            plc_read_string(
                handle,
                c"x=2;HR10".as_ptr(),
                0,
                4,
                output.as_mut_ptr(),
                8,
                &mut written
            ),
            PLC_OK
        );
        assert_eq!(written, 4);
        assert_eq!(&output[..4], "中\0".as_bytes());
        assert_eq!(
            plc_read(
                handle,
                c"C0".as_ptr(),
                1,
                3,
                output.as_mut_ptr(),
                8,
                &mut written
            ),
            PLC_OK
        );
        assert_eq!(&output[..3], &[1, 0, 1]);
    }
    assert_eq!(plc_destroy(handle), PLC_OK);
    server.join().unwrap();
}

struct SerialState {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
}

unsafe extern "C" fn serial_read(context: usize, output: *mut u8, capacity: u32) -> i32 {
    let state = unsafe { &mut *(context as *mut SerialState) };
    let buffer = unsafe { std::slice::from_raw_parts_mut(output, capacity as usize) };
    state.input.read(buffer).unwrap() as i32
}

unsafe extern "C" fn serial_write(context: usize, input: *const u8, length: u32) -> i32 {
    let state = unsafe { &mut *(context as *mut SerialState) };
    state
        .output
        .extend_from_slice(unsafe { std::slice::from_raw_parts(input, length as usize) });
    length as i32
}

fn rtu_reply() -> Vec<u8> {
    let mut reply = vec![1, 3, 2, 0, 42];
    let mut crc = 0xffff_u16;
    for value in &reply {
        crc ^= u16::from(*value);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
    }
    reply.extend_from_slice(&crc.to_le_bytes());
    reply
}

#[test]
fn serial_callbacks_support_rtu_and_ascii_without_opening_a_port() {
    for protocol in [13, 14] {
        let reply = if protocol == 13 {
            rtu_reply()
        } else {
            b":010302002AD0\r\n".to_vec()
        };
        let mut state = Box::new(SerialState {
            input: Cursor::new(reply),
            output: Vec::new(),
        });
        let callbacks = PlcSerialCallbacks {
            context: (&mut *state as *mut SerialState) as usize,
            read: Some(serial_read),
            write: Some(serial_write),
            flush: None,
        };
        let options = PlcOptions::for_protocol(protocol).unwrap();
        let mut handle = 0;
        let mut output = [0; 2];
        let mut written = 0;
        unsafe {
            assert_eq!(plc_create_serial(&options, &callbacks, &mut handle), PLC_OK);
            assert_eq!(plc_connect(handle), PLC_OK);
            assert_eq!(
                plc_read(
                    handle,
                    c"HR0".as_ptr(),
                    4,
                    1,
                    output.as_mut_ptr(),
                    2,
                    &mut written
                ),
                PLC_OK
            );
        }
        assert_eq!(output, [42, 0]);
        assert_eq!(written, 2);
        let expected = if protocol == 13 {
            vec![1, 3, 0, 0, 0, 1, 0x84, 0x0a]
        } else {
            b":010300000001FB\r\n".to_vec()
        };
        assert_eq!(state.output, expected);
        assert_eq!(plc_disconnect(handle), PLC_OK);
        assert_eq!(plc_connect(handle), PLC_OPERATION_FAILED);
        assert_eq!(plc_destroy(handle), PLC_OK);
    }
}

#[test]
fn callback_errors_are_bounded_and_do_not_escape() {
    assert_eq!(
        callback_result(-2, 10).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert!(callback_result(-1, 10).is_err());
    assert!(callback_result(11, 10).is_err());
    assert_eq!(callback_result(3, 10).unwrap(), 3);
    let options = PlcOptions::for_protocol(13).unwrap();
    let callbacks = PlcSerialCallbacks {
        context: 0,
        read: None,
        write: None,
        flush: None,
    };
    let mut handle = 1;
    assert_eq!(
        unsafe { plc_create_serial(&options, &callbacks, &mut handle) },
        PLC_INVALID_ARGUMENT
    );
    assert_eq!(handle, 0);
}
