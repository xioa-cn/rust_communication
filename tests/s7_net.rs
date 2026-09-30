use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rs_appliaction::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use rs_appliaction::communication::s7::s7_net::S7Net;
use rs_appliaction::communication::s7::s7_type::S7Type;
use rs_appliaction::communication::s7::s7_value::S7Value;
use rs_appliaction::communication::timeout::Timeout;

fn client(port: u16, timeout: Timeout, rack: usize, slot: usize) -> S7Net {
    S7Net::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        port,
        S7Type::S1200,
        timeout,
        rack,
        slot,
    )
}

fn server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let thread = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream.set_nodelay(true).unwrap();
        handler(stream);
    });
    (port, thread)
}

fn receive(stream: &mut TcpStream) -> Vec<u8> {
    let mut header = [0; 4];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(header[..2], [3, 0]);
    let length = usize::from(u16::from_be_bytes([header[2], header[3]]));
    assert!((7..=1028).contains(&length));
    let mut payload = vec![0; length - 4];
    stream.read_exact(&mut payload).unwrap();
    payload
}

fn send(stream: &mut TcpStream, payload: &[u8]) {
    try_send(stream, payload).unwrap();
}

fn try_send(stream: &mut TcpStream, payload: &[u8]) -> std::io::Result<()> {
    let mut packet = vec![3, 0];
    packet.extend_from_slice(&((payload.len() + 4) as u16).to_be_bytes());
    packet.extend_from_slice(payload);
    for chunk in packet.chunks(3) {
        stream.write_all(chunk)?;
    }
    Ok(())
}

fn response(request: &[u8], parameters: &[u8], data: &[u8]) -> Vec<u8> {
    let mut packet = vec![2, 0xf0, 0x80, 0x32, 3, 0, 0, request[7], request[8]];
    packet.extend_from_slice(&(parameters.len() as u16).to_be_bytes());
    packet.extend_from_slice(&(data.len() as u16).to_be_bytes());
    packet.extend_from_slice(&[0, 0]);
    packet.extend_from_slice(parameters);
    packet.extend_from_slice(data);
    packet
}

fn handshake(stream: &mut TcpStream, rack: usize, slot: usize) {
    let request = receive(stream);
    assert_eq!(
        request,
        [
            17,
            0xe0,
            0,
            0,
            1,
            0,
            0,
            0xc1,
            2,
            1,
            0,
            0xc2,
            2,
            3,
            (rack * 32 + slot) as u8,
            0xc0,
            1,
            10
        ]
    );
    send(
        stream,
        &[
            17,
            0xd0,
            1,
            0,
            0,
            1,
            0,
            0xc1,
            2,
            1,
            0,
            0xc2,
            2,
            3,
            (rack * 32 + slot) as u8,
            0xc0,
            1,
            10,
        ],
    );
    let setup = receive(stream);
    assert_eq!(
        setup,
        [
            2, 0xf0, 0x80, 0x32, 1, 0, 0, 0, 1, 0, 8, 0, 0, 0xf0, 0, 0, 1, 0, 1, 3, 0xc0
        ]
    );
    send(
        stream,
        &response(&setup, &[0xf0, 0, 0, 1, 0, 1, 0, 240], &[]),
    );
}

#[test]
fn reads_and_writes_scalars_bits_and_bytes() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 1, 2);
        let request = receive(&mut stream);
        assert_eq!(
            &request[13..],
            &[4, 1, 0x12, 10, 0x10, 2, 0, 2, 0, 1, 0x84, 0, 0, 16]
        );
        let packet = response(&request, &[4, 1], &[0xff, 4, 0, 16, 0x12, 0x34]);
        let mut first = vec![2, 0xf0, 0];
        first.extend_from_slice(&packet[3..10]);
        let mut second = vec![2, 0xf0, 0x80];
        second.extend_from_slice(&packet[10..]);
        send(&mut stream, &first);
        send(&mut stream, &second);

        let request = receive(&mut stream);
        assert_eq!(
            &request[13..],
            &[4, 1, 0x12, 10, 0x10, 1, 0, 1, 0, 0, 0x83, 0, 0, 83]
        );
        send(
            &mut stream,
            &response(&request, &[4, 1], &[0xff, 3, 0, 1, 1]),
        );

        let request = receive(&mut stream);
        assert_eq!(
            &request[13..],
            &[
                5, 1, 0x12, 10, 0x10, 1, 0, 1, 0, 0, 0x83, 0, 0, 83, 0, 3, 0, 1, 1
            ]
        );
        send(&mut stream, &response(&request, &[5, 1], &[0xff]));

        let request = receive(&mut stream);
        assert_eq!(
            &request[13..],
            &[
                5, 1, 0x12, 10, 0x10, 2, 0, 4, 0, 1, 0x84, 0, 0, 32, 0, 4, 0, 32, 0x12, 0x34, 0x56,
                0x78
            ]
        );
        send(&mut stream, &response(&request, &[5, 1], &[0xff]));

        let request = receive(&mut stream);
        assert_eq!(request[23], 0x81);
        send(
            &mut stream,
            &response(&request, &[4, 1], &[0xff, 4, 0, 24, 1, 2, 3]),
        );
    });
    let mut plc = client(port, Timeout::default(), 1, 2);
    assert!(!plc.is_connected());
    assert!(DeviceBase::connect(&mut plc).is_success);
    assert!(plc.connect().is_success);
    assert_eq!(plc.negotiated_pdu_length(), Some(240));
    assert_eq!(plc.s7_type(), S7Type::S1200);
    assert_eq!(
        &*ReadBase::<u16>::read(&mut plc, "DB1.DBW2", 1)
            .to_result()
            .unwrap(),
        &[0x1234]
    );
    assert_eq!(&*plc.read::<bool>("M10.3", 1).to_result().unwrap(), &[true]);
    assert!(
        WriteBase::write(&mut plc, "M10.3", true)
            .to_result()
            .unwrap()
    );
    assert_eq!(
        plc.write("DB1.DBD4", 0x12345678_u32).to_result().unwrap(),
        0x12345678
    );
    assert_eq!(&*plc.read::<u8>("I0", 3).to_result().unwrap(), &[1, 2, 3]);
    assert!(DeviceBase::disconnect(&mut plc).is_success);
    assert!(plc.disconnect().is_success);
    assert!(!plc.is_connected());
    worker.join().unwrap();
}

#[test]
#[ignore = "local loopback throughput measurement"]
fn loopback_io_benchmark() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 1);
        for _ in 0..6200 {
            let request = receive(&mut stream);
            assert_eq!(request[13], 4);
            let reply = response(&request, &[4, 1], &[0xff, 4, 0, 32, 0x12, 0x34, 0x56, 0x78]);
            let mut frame = vec![3, 0];
            frame.extend_from_slice(&((reply.len() + 4) as u16).to_be_bytes());
            frame.extend_from_slice(&reply);
            stream.write_all(&frame).unwrap();
            let request = receive(&mut stream);
            assert_eq!(request[13], 5);
            assert_eq!(&request[request.len() - 2..], &[0x12, 0x34]);
            let reply = response(&request, &[5, 1], &[0xff]);
            let mut frame = vec![3, 0];
            frame.extend_from_slice(&((reply.len() + 4) as u16).to_be_bytes());
            frame.extend_from_slice(&reply);
            stream.write_all(&frame).unwrap();
        }
    });
    let mut plc = client(port, Timeout::new(1000, 1000), 0, 1);
    plc.connect().to_result().unwrap();
    for round in 0..4 {
        let iterations = if round == 0 { 200 } else { 2000 };
        let mut read_time = Duration::ZERO;
        let mut write_time = Duration::ZERO;
        for _ in 0..iterations {
            let started = std::time::Instant::now();
            assert_eq!(
                &*plc.read::<u16>("DB1.0", 2).to_result().unwrap(),
                &[0x1234, 0x5678]
            );
            read_time += started.elapsed();
            let started = std::time::Instant::now();
            plc.write::<u16>("DB1.0", 0x1234).to_result().unwrap();
            write_time += started.elapsed();
        }
        if round > 0 {
            println!(
                "S7 round={round} read_us={:.2} write_us={:.2}",
                read_time.as_secs_f64() * 1e6 / 2000.0,
                write_time.as_secs_f64() * 1e6 / 2000.0
            );
        }
    }
    plc.disconnect().to_result().unwrap();
    worker.join().unwrap();
}

#[test]
fn splits_large_reads_and_writes_at_negotiated_pdu_size() {
    let bytes: Vec<u8> = (0..600).map(|index| (index % 251) as u8).collect();
    let expected = bytes.clone();
    let (port, worker) = server(move |mut stream| {
        handshake(&mut stream, 0, 0);
        for (function, counts) in [(4, vec![222, 222, 156]), (5, vec![212, 212, 176])] {
            let mut offset = 0;
            for count in counts {
                let request = receive(&mut stream);
                assert_eq!(request[13], function);
                assert_eq!(
                    u16::from_be_bytes([request[19], request[20]]) as usize,
                    count
                );
                assert_eq!(
                    &request[24..27],
                    &(((100 + offset) * 8) as u32).to_be_bytes()[1..]
                );
                assert_eq!(&request[21..24], &[0, 2, 0x84]);
                if function == 4 {
                    let mut data = vec![0xff, 4];
                    data.extend_from_slice(&((count * 8) as u16).to_be_bytes());
                    data.extend_from_slice(&expected[offset..offset + count]);
                    send(&mut stream, &response(&request, &[4, 1], &data));
                } else {
                    assert!(request.len() - 3 <= 240);
                    assert_eq!(&request[31..], &expected[offset..offset + count]);
                    send(&mut stream, &response(&request, &[5, 1], &[0xff]));
                }
                offset += count;
            }
        }
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    assert_eq!(
        &*plc.read::<u8>("DB2.100", 600).to_result().unwrap(),
        bytes.as_slice()
    );
    assert_eq!(
        WriteBase::write_all(&mut plc, "DB2.100", &bytes)
            .to_result()
            .unwrap(),
        600
    );
    worker.join().unwrap();
}

#[test]
fn reports_plc_errors_without_losing_connection() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        let request = receive(&mut stream);
        send(&mut stream, &response(&request, &[4, 1], &[5, 0, 0, 0]));
        let request = receive(&mut stream);
        let mut packet = response(&request, &[], &[]);
        packet[13] = 0x81;
        packet[14] = 4;
        send(&mut stream, &packet);
        let request = receive(&mut stream);
        send(
            &mut stream,
            &response(&request, &[4, 1], &[0xff, 4, 0, 8, 42]),
        );
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    assert!(plc.read::<u8>("M0", 1).msg.contains("0x05"));
    assert!(plc.is_connected());
    assert!(plc.read::<u8>("M0", 1).msg.contains("0x8104"));
    assert!(plc.is_connected());
    assert_eq!(&*plc.read::<u8>("M0", 1).to_result().unwrap(), &[42]);
    worker.join().unwrap();
}

#[test]
fn reports_partial_write_without_retry() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        let request = receive(&mut stream);
        send(&mut stream, &response(&request, &[5, 1], &[0xff]));
        let _request = receive(&mut stream);
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    let result = plc.write_all("DB1.0", &[1_u8; 500]);
    assert!(!result.is_success);
    assert!(result.msg.contains("212 acknowledged bytes"));
    assert!(result.msg.contains("no automatic retry"));
    assert!(!plc.is_connected());
    worker.join().unwrap();
}

#[test]
fn invalid_configuration_is_rejected_before_connecting() {
    for (timeout, rack, slot) in [
        (Timeout::new(0, 1), 0, 0),
        (Timeout::new(1, -1), 0, 0),
        (Timeout::default(), 8, 0),
        (Timeout::default(), 0, 32),
        (Timeout::default(), usize::MAX, usize::MAX),
    ] {
        let mut plc = client(0, timeout, rack, slot);
        assert!(!plc.connect().is_success);
        assert!(!plc.is_connected());
    }
    assert_eq!(Timeout::default().connect_time_out(), 5_000);
    assert_eq!(Timeout::new(100, 200).receive_time_out(), 200);
}

#[test]
fn validates_addresses_lengths_and_connection_state() {
    let mut plc = client(0, Timeout::default(), 0, 0);
    for address in [
        "",
        "DB1",
        "DB-1.0",
        "DB65536.0",
        "DB1.DBX0",
        "DB1.DBB0.1",
        "M-1",
        "M0.8",
        "M0.1.2",
        "M2097152",
        "T1",
        "é0",
    ] {
        let result = plc.read::<u8>(address, 1);
        assert!(!result.is_success, "{address}");
        assert!(!result.msg.contains("not connected"), "{address}");
    }
    for address in [
        "db1.dbb0",
        "DB65535.0",
        "MB1",
        "MW2",
        "MD4",
        "I0",
        "E0",
        "QB0",
        "A0",
    ] {
        assert!(
            plc.read::<u8>(address, 1).msg.contains("not connected"),
            "{address}"
        );
    }
    for address in [
        "DB1.DBX0.1",
        "DB1.0.1",
        "MX0.1",
        "M2097151.7",
        "DB1.100",
        "M100",
        "I0",
        "Q0",
    ] {
        assert!(
            plc.read::<bool>(address, 1).msg.contains("not connected"),
            "{address}"
        );
    }
    assert!(plc.read::<u16>("M2097151", 1).msg.contains("address range"));
    assert!(
        plc.read::<u8>("M0", usize::MAX)
            .msg
            .contains("address range")
    );
    assert!(plc.read::<bool>("M0", 1).msg.contains("not connected"));
    assert!(plc.read::<u8>("M0.0", 1).msg.contains("not connected"));
    assert!(plc.read::<u8>("M0.1", 1).msg.contains("bit addresses"));
    assert!(plc.read::<u16>("M0", 2).msg.contains("not connected"));
    assert!(
        plc.read::<u64>("M0", usize::MAX)
            .msg
            .contains("address range")
    );
    assert!(!plc.read::<u8>("M0", 0).is_success);
    assert!(!plc.write_all::<u16>("M0", &[]).is_success);
    assert!(
        plc.write_all("M2097151", &[1_u16])
            .msg
            .contains("address range")
    );
    assert!(
        plc.write_all("M2097151.7", &[true, false])
            .msg
            .contains("address range")
    );
}

#[test]
fn numeric_codecs_use_big_endian() {
    assert_eq!(S7Value::to_be_bytes(&0x1234_u16), [0x12, 0x34]);
    assert_eq!(<i32 as S7Value>::from_be_bytes(&[0xff; 4]).unwrap(), -1);
    assert_eq!(S7Value::to_be_bytes(&1.0_f32), [0x3f, 0x80, 0, 0]);
    assert_eq!(
        <f64 as S7Value>::from_be_bytes(&1.5_f64.to_be_bytes()).unwrap(),
        1.5
    );
    assert_eq!(S7Value::to_be_bytes(&true), [1]);
    assert!(<bool as S7Value>::from_be_bytes(&[2]).is_err());
    assert!(<u32 as S7Value>::from_be_bytes(&[0; 3]).is_err());
}

/// 模拟一个 DB1 字节读取响应，同时验证字符串头和内容的请求偏移及长度。
fn answer_string_read(stream: &mut TcpStream, offset: u32, bytes: &[u8]) {
    let request = receive(stream);
    assert_eq!(request[13], 4);
    assert_eq!(request[18], 2);
    assert_eq!(&request[19..21], &(bytes.len() as u16).to_be_bytes());
    assert_eq!(&request[21..24], &[0, 1, 0x84]);
    assert_eq!(&request[24..27], &(offset * 8).to_be_bytes()[1..]);
    let mut data = vec![0xff, 4];
    data.extend_from_slice(&((bytes.len() * 8) as u16).to_be_bytes());
    data.extend_from_slice(bytes);
    send(stream, &response(&request, &[4, 1], &data));
}

/// 验证写请求只覆盖预期正文/长度位置，不触碰最大容量或后续字段。
fn answer_string_write(stream: &mut TcpStream, offset: u32, bytes: &[u8]) {
    let request = receive(stream);
    assert_eq!(request[13], 5);
    assert_eq!(request[18], 2);
    assert_eq!(&request[19..21], &(bytes.len() as u16).to_be_bytes());
    assert_eq!(&request[21..24], &[0, 1, 0x84]);
    assert_eq!(&request[24..27], &(offset * 8).to_be_bytes()[1..]);
    assert_eq!(&request[27..29], &[0, 4]);
    assert_eq!(&request[29..31], &((bytes.len() * 8) as u16).to_be_bytes());
    assert_eq!(&request[31..], bytes);
    assert!(request.len() - 3 <= 240);
    send(stream, &response(&request, &[5, 1], &[0xff]));
}

#[test]
fn writes_string_literal_without_changing_declared_capacity() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 10, &[254, 3]);
        answer_string_write(&mut stream, 11, &[5, b'n', b'i', b'h', b'a', b'o']);
        answer_string_read(&mut stream, 10, &[254, 5]);
        answer_string_read(&mut stream, 12, b"nihao");
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    let result = plc.write::<String>("DB1.10", "nihao");
    assert_eq!(result.to_result().unwrap(), "nihao");
    assert_eq!(plc.read_s7_string("DB1.10").to_result().unwrap(), "nihao");
    worker.join().unwrap();
}

#[test]
fn string_write_accepts_owned_and_borrowed_strings() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        for _ in 0..3 {
            answer_string_read(&mut stream, 10, &[20, 0]);
            answer_string_write(&mut stream, 11, &[3, b'a', b'b', b'c']);
        }
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    let value = String::from("abc");
    assert_eq!(
        plc.write::<String>("DB1.10", &value).to_result().unwrap(),
        value
    );
    assert_eq!(
        plc.write::<String>("DB1.10", value.clone())
            .to_result()
            .unwrap(),
        value
    );
    let result: String = plc.write("DB1.10", value).to_result().unwrap();
    assert_eq!(result, "abc");
    worker.join().unwrap();
}

#[test]
fn string_write_counts_utf8_bytes_and_allows_empty_values() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 10, &[6, 0]);
        let mut encoded = vec![6];
        encoded.extend_from_slice("你好".as_bytes());
        answer_string_write(&mut stream, 11, &encoded);
        answer_string_read(&mut stream, 10, &[6, 6]);
        answer_string_write(&mut stream, 11, &[0]);
        answer_string_read(&mut stream, 20, &[0, 0]);
        answer_string_write(&mut stream, 21, &[0]);
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    assert_eq!(
        plc.write::<String>("DB1.10", "你好").to_result().unwrap(),
        "你好"
    );
    assert_eq!(plc.write::<String>("DB1.10", "").to_result().unwrap(), "");
    assert_eq!(plc.write::<String>("DB1.20", "").to_result().unwrap(), "");
    worker.join().unwrap();
}

#[test]
fn string_write_rejects_bad_headers_and_capacity_before_any_write() {
    for (header, text, error) in [
        ([4, 0], "nihao", "capacity is 4"),
        ([5, 0], "你好", "capacity is 5"),
        ([255, 0], "hi", "STRING header"),
        ([3, 4], "hi", "STRING header"),
    ] {
        let (port, worker) = server(move |mut stream| {
            handshake(&mut stream, 0, 0);
            answer_string_read(&mut stream, 10, &header);
            let mut unexpected = [0];
            assert_eq!(
                stream.read(&mut unexpected).unwrap(),
                0,
                "Unexpected write after invalid capacity/header"
            );
        });
        let mut plc = client(port, Timeout::default(), 0, 0);
        plc.connect().to_result().unwrap();
        let result = plc.write::<String>("DB1.10", text);
        assert!(!result.is_success);
        assert!(result.content.is_none());
        assert!(result.msg.contains(error), "{}", result.msg);
        plc.disconnect();
        worker.join().unwrap();
    }
}

#[test]
fn long_string_write_splits_without_overwriting_capacity() {
    let value = "x".repeat(254);
    let text = value.clone();
    let (port, worker) = server(move |mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 10, &[254, 0]);
        let mut data = vec![254];
        data.extend_from_slice(text.as_bytes());
        answer_string_write(&mut stream, 11, &data[..212]);
        answer_string_write(&mut stream, 223, &data[212..]);
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    assert_eq!(
        plc.write::<String>("DB1.10", &value).to_result().unwrap(),
        value
    );
    worker.join().unwrap();
}

#[test]
fn string_write_reports_partial_failure_without_retry() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 10, &[254, 0]);
        let mut data = vec![254];
        data.extend_from_slice(&[b'x'; 211]);
        answer_string_write(&mut stream, 11, &data);
        let request = receive(&mut stream);
        assert_eq!(&request[24..27], &(223_u32 * 8).to_be_bytes()[1..]);
        assert_eq!(&request[31..], &[b'x'; 43]);
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    let result = plc.write::<String>("DB1.10", "x".repeat(254));
    assert!(!result.is_success);
    assert!(result.msg.contains("212 acknowledged bytes"));
    assert!(result.msg.contains("no automatic retry"));
    assert!(!plc.is_connected());
    worker.join().unwrap();
}

#[test]
fn string_writes_reject_invalid_input_without_connection() {
    let mut plc = client(0, Timeout::default(), 0, 0);
    assert!(
        plc.write::<String>("DB1.10", "x".repeat(255))
            .msg
            .contains("254 bytes")
    );
    assert!(
        plc.write::<String>("DB1.10.1", "hi")
            .msg
            .contains("bit addresses")
    );
    assert!(
        plc.write::<String>("DB1.2097151", "hi")
            .msg
            .contains("address range")
    );
    assert!(
        plc.write::<String>("DB1.10", "hi")
            .msg
            .contains("not connected")
    );
}

#[test]
fn reads_string_using_its_actual_length_header() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 100, &[20, 5]);
        answer_string_read(&mut stream, 102, b"Hello");
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    let text = plc.read_s7_string("DB1.100").to_result().unwrap();
    assert_eq!(text, "Hello");
    worker.join().unwrap();
}

#[test]
fn s7_strings_without_length_handles_short_empty_and_utf8_values() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 0, &[5, 2]);
        answer_string_read(&mut stream, 2, b"Hi");
        answer_string_read(&mut stream, 7, &[10, 0]);
        answer_string_read(&mut stream, 19, &[20, 6]);
        answer_string_read(&mut stream, 21, "你好".as_bytes());
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    for (address, expected) in [("DB1.0", "Hi"), ("DB1.7", ""), ("DB1.19", "你好")] {
        let texts = plc.read_s7_strings(address).to_result().unwrap();
        assert_eq!(&*texts, &[expected]);
    }
    worker.join().unwrap();
}

#[test]
fn maximum_length_string_can_split_a_utf8_character_across_pdus() {
    let expected = format!("{}字{}", "a".repeat(221), "b".repeat(30));
    let bytes = expected.as_bytes().to_vec();
    assert_eq!(bytes.len(), 254);
    let (port, worker) = server(move |mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 0, &[254, 254]);
        answer_string_read(&mut stream, 2, &bytes[..222]);
        answer_string_read(&mut stream, 224, &bytes[222..]);
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    assert_eq!(plc.read_s7_string("DB1.0").to_result().unwrap(), expected);
    worker.join().unwrap();
}

#[test]
fn string_read_rejects_invalid_headers_and_encoding() {
    for header in [[255, 0], [4, 5], [10, 1]] {
        let (port, worker) = server(move |mut stream| {
            handshake(&mut stream, 0, 0);
            answer_string_read(&mut stream, 0, &header);
            if header == [10, 1] {
                answer_string_read(&mut stream, 2, &[0xff]);
            }
        });
        let mut plc = client(port, Timeout::default(), 0, 0);
        plc.connect().to_result().unwrap();
        let result = plc.read_s7_string("DB1.0");
        assert!(!result.is_success);
        assert!(result.content.is_none());
        let expected = if header == [10, 1] {
            "UTF-8"
        } else {
            "STRING header"
        };
        assert!(result.msg.contains(expected), "{}", result.msg);
        worker.join().unwrap();
    }
}

#[test]
fn string_reads_validate_address_before_network_io() {
    let mut plc = client(0, Timeout::default(), 0, 0);
    assert!(
        plc.read_s7_strings("DB1.2097151")
            .msg
            .contains("address range")
    );
    assert!(plc.read_s7_strings("DB1.0.1").msg.contains("bit addresses"));
    assert!(plc.read_s7_strings("DB1.0").msg.contains("not connected"));
    assert!(plc.read_s7_string("DB1.0.1").msg.contains("bit addresses"));
    assert!(plc.read_s7_string("DB1.0").msg.contains("not connected"));
}

#[test]
fn raw_string_read_does_not_strip_a_siemens_length_header() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 100, &[5, 3, b'a', b'b', b'c']);
        answer_string_read(&mut stream, 100, &[5, 3, b'a', b'b', b'c']);
        answer_string_read(&mut stream, 100, &[5, 3]);
        answer_string_read(&mut stream, 102, b"abc");
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    let raw: Box<[String]> = ReadBase::<String>::read(&mut plc, "DB1.100", 5)
        .to_result()
        .unwrap();
    assert_eq!(&*raw, &["\u{5}\u{3}abc"]);
    assert_eq!(
        plc.read_string("DB1.100", 5).to_result().unwrap(),
        "\u{5}\u{3}abc"
    );
    assert_eq!(plc.read_s7_string("DB1.100").to_result().unwrap(), "abc");
    worker.join().unwrap();
}

#[test]
fn raw_string_length_is_byte_count_and_preserves_zero_bytes() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 0, "你好\0".as_bytes());
        answer_string_read(&mut stream, 20, b"Hello\0");
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    let texts = plc.read::<String>("DB1.0", 7).to_result().unwrap();
    assert_eq!(&*texts, &["你好\0"]);
    assert_eq!(plc.read_string("DB1.20", 6).to_result().unwrap(), "Hello\0");
    worker.join().unwrap();
}

#[test]
fn raw_string_can_exceed_s7_string_capacity_and_span_pdus() {
    let expected = format!("{}字{}", "a".repeat(221), "b".repeat(276));
    let bytes = expected.as_bytes().to_vec();
    assert_eq!(bytes.len(), 500);
    let (port, worker) = server(move |mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 0, &bytes[..222]);
        answer_string_read(&mut stream, 222, &bytes[222..444]);
        answer_string_read(&mut stream, 444, &bytes[444..]);
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    assert_eq!(plc.read_string("DB1.0", 500).to_result().unwrap(), expected);
    worker.join().unwrap();
}

#[test]
fn raw_string_invalid_encoding_is_an_error_not_replacement_text() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        answer_string_read(&mut stream, 0, &[0xff]);
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    let result = plc.read::<String>("DB1.0", 1);
    assert!(!result.is_success);
    assert!(result.msg.contains("UTF-8"));
    worker.join().unwrap();
}

#[test]
fn raw_string_reads_validate_byte_count_and_address() {
    let mut plc = client(0, Timeout::default(), 0, 0);
    assert!(
        plc.read::<String>("DB1.0", 0)
            .msg
            .contains("greater than zero")
    );
    assert!(
        plc.read_string("DB1.0", 0)
            .msg
            .contains("greater than zero")
    );
    for (address, length) in [("DB1.0", usize::MAX), ("DB1.2097151", 2)] {
        assert!(
            plc.read::<String>(address, length)
                .msg
                .contains("address range")
        );
        assert!(
            plc.read_string(address, length)
                .msg
                .contains("address range")
        );
    }
    assert!(plc.read_string("DB1.0.1", 1).msg.contains("bit addresses"));
    assert!(
        plc.read::<String>("DB1.2097151", 1)
            .msg
            .contains("not connected")
    );
}

#[test]
fn shorthand_addresses_work_for_boolean_reads_and_writes() {
    const CASES: [(&str, u8, u16); 9] = [
        ("DB1.100", 0x84, 1),
        ("DB1.100.0", 0x84, 1),
        ("DB1.DBX100.0", 0x84, 1),
        ("M100", 0x83, 0),
        ("m100.0", 0x83, 0),
        ("I100", 0x81, 0),
        ("E100", 0x81, 0),
        ("Q100", 0x82, 0),
        ("A100", 0x82, 0),
    ];
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        for (_, area, db) in CASES {
            for function in [4, 5] {
                let request = receive(&mut stream);
                assert_eq!(request[13], function);
                assert_eq!(request[18], 1);
                assert_eq!(&request[19..21], &[0, 1]);
                assert_eq!(&request[21..23], &db.to_be_bytes());
                assert_eq!(request[23], area);
                assert_eq!(&request[24..27], &[0, 3, 0x20]);
                if function == 4 {
                    send(
                        &mut stream,
                        &response(&request, &[4, 1], &[0xff, 3, 0, 1, 1]),
                    );
                } else {
                    assert_eq!(&request[27..], &[0, 3, 0, 1, 1]);
                    send(&mut stream, &response(&request, &[5, 1], &[0xff]));
                }
            }
        }
        let request = receive(&mut stream);
        assert_eq!(request[18], 2);
        assert_eq!(&request[19..21], &[0, 2]);
        assert_eq!(&request[24..27], &[0, 3, 0x20]);
        send(
            &mut stream,
            &response(&request, &[4, 1], &[0xff, 4, 0, 16, 0x12, 0x34]),
        );
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    for (address, _, _) in CASES {
        assert_eq!(&*plc.read::<bool>(address, 1).to_result().unwrap(), &[true]);
        assert!(plc.write(address, true).to_result().unwrap());
    }
    assert_eq!(
        &*plc.read::<u16>("DB1.100.0", 1).to_result().unwrap(),
        &[0x1234]
    );
    worker.join().unwrap();
}

#[test]
fn reads_and_writes_typed_arrays() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        let request = receive(&mut stream);
        assert_eq!(&request[19..21], &[0, 6]);
        send(
            &mut stream,
            &response(&request, &[4, 1], &[0xff, 4, 0, 48, 0, 1, 0, 2, 0xff, 0xff]),
        );
        let request = receive(&mut stream);
        assert_eq!(&request[19..21], &[0, 6]);
        assert_eq!(&request[27..], &[0, 4, 0, 48, 0, 1, 0, 2, 0xff, 0xff]);
        send(&mut stream, &response(&request, &[5, 1], &[0xff]));
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    let values: Box<[u16]> = plc.read("DB1.0", 3).to_result().unwrap();
    assert_eq!(&*values, &[1, 2, 65535]);
    assert_eq!(plc.write_all("DB1.0", &values).to_result().unwrap(), 3);
    worker.join().unwrap();
}

#[test]
fn boolean_arrays_cross_byte_boundaries_without_overwriting_neighbors() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        for function in [4, 5] {
            for (offset, value) in [1, 0, 1].into_iter().enumerate() {
                let request = receive(&mut stream);
                assert_eq!(request[13], function);
                assert_eq!(request[18], 1);
                assert_eq!(&request[19..21], &[0, 1]);
                assert_eq!(&request[24..27], &[0, 0, 7 + offset as u8]);
                if function == 4 {
                    send(
                        &mut stream,
                        &response(&request, &[4, 1], &[0xff, 3, 0, 1, value]),
                    );
                } else {
                    assert_eq!(&request[27..], &[0, 3, 0, 1, value]);
                    send(&mut stream, &response(&request, &[5, 1], &[0xff]));
                }
            }
        }
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    assert_eq!(
        &*plc.read::<bool>("M0.7", 3).to_result().unwrap(),
        &[true, false, true]
    );
    assert_eq!(
        plc.write_all("M0.7", &[true, false, true])
            .to_result()
            .unwrap(),
        3
    );
    worker.join().unwrap();
}

#[test]
fn typed_arrays_decode_elements_split_across_pdus() {
    let expected: Vec<u32> = (0..80).map(|index| 0x1234_0000 + index).collect();
    let bytes: Vec<u8> = expected
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect();
    let (port, worker) = server(move |mut stream| {
        handshake(&mut stream, 0, 0);
        let mut offset = 0;
        for count in [222, 98] {
            let request = receive(&mut stream);
            assert_eq!(
                u16::from_be_bytes([request[19], request[20]]) as usize,
                count
            );
            let mut data = vec![0xff, 4];
            data.extend_from_slice(&((count * 8) as u16).to_be_bytes());
            data.extend_from_slice(&bytes[offset..offset + count]);
            send(&mut stream, &response(&request, &[4, 1], &data));
            offset += count;
        }
    });
    let mut plc = client(port, Timeout::default(), 0, 0);
    plc.connect().to_result().unwrap();
    assert_eq!(
        &*plc.read::<u32>("DB1.0", 80).to_result().unwrap(),
        expected.as_slice()
    );
    worker.join().unwrap();
}

#[test]
fn malformed_responses_invalidate_connection() {
    for fault in 0..5 {
        let (port, worker) = server(move |mut stream| {
            handshake(&mut stream, 0, 0);
            let request = receive(&mut stream);
            let mut packet = response(&request, &[4, 1], &[0xff, 4, 0, 8, 42]);
            match fault {
                0 => packet[8] = packet[8].wrapping_add(1),
                1 => packet[12] = 6,
                2 => packet[20] = 16,
                3 => packet[1] = 0xe0,
                _ => {
                    stream.write_all(&[3, 0, 0xff, 0xff]).unwrap();
                    return;
                }
            }
            if let Err(error) = try_send(&mut stream, &packet) {
                assert_eq!(fault, 3);
                assert!(matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::BrokenPipe
                ));
            }
        });
        let mut plc = client(port, Timeout::default(), 0, 0);
        plc.connect().to_result().unwrap();
        assert!(!plc.read::<u8>("M0", 1).is_success, "fault {fault}");
        assert!(!plc.is_connected(), "fault {fault}");
        worker.join().unwrap();
    }
}

#[test]
fn receive_timeout_invalidates_connection() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0, 0);
        let _request = receive(&mut stream);
        thread::sleep(Duration::from_millis(600));
    });
    let mut plc = client(port, Timeout::new(2_000, 100), 0, 0);
    plc.connect().to_result().unwrap();
    let result = plc.read::<u8>("M0", 1);
    assert!(!result.is_success);
    assert!(result.msg.contains("socket error"));
    assert!(!plc.is_connected());
    worker.join().unwrap();
}
