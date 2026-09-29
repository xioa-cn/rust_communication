//! 各 CPU 的握手和内存布局回归测试；仅使用回环模拟服务，不连接实体 PLC。

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rs_appliaction::communication::s7::s7_net::S7Net;
use rs_appliaction::communication::s7::s7_type::S7Type;
use rs_appliaction::communication::timeout::Timeout;

fn server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
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
    (port, worker)
}

fn receive(stream: &mut TcpStream) -> Vec<u8> {
    let mut header = [0; 4];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(&header[..2], &[3, 0]);
    let length = usize::from(u16::from_be_bytes([header[2], header[3]]));
    assert!((7..=1028).contains(&length));
    let mut data = vec![0; length - 4];
    stream.read_exact(&mut data).unwrap();
    data
}

fn send(stream: &mut TcpStream, data: &[u8]) {
    let mut packet = vec![3, 0];
    packet.extend_from_slice(&((data.len() + 4) as u16).to_be_bytes());
    packet.extend_from_slice(data);
    stream.write_all(&packet).unwrap();
}

fn respond(stream: &mut TcpStream, request: &[u8], parameters: &[u8], data: &[u8]) {
    let mut packet = vec![2, 0xf0, 0x80, 0x32, 3, 0, 0, request[7], request[8]];
    packet.extend_from_slice(&(parameters.len() as u16).to_be_bytes());
    packet.extend_from_slice(&(data.len() as u16).to_be_bytes());
    packet.extend_from_slice(&[0, 0]);
    packet.extend_from_slice(parameters);
    packet.extend_from_slice(data);
    send(stream, &packet);
}

fn handshake(stream: &mut TcpStream, local: u16, remote: u16, pdu: u16) {
    let request = receive(stream);
    assert_eq!(&request[..9], &[17, 0xe0, 0, 0, 1, 0, 0, 0xc1, 2]);
    assert_eq!(&request[9..11], &local.to_be_bytes());
    assert_eq!(&request[11..13], &[0xc2, 2]);
    assert_eq!(&request[13..15], &remote.to_be_bytes());
    assert_eq!(&request[15..], &[0xc0, 1, 10]);
    let mut confirmation = request;
    confirmation[1] = 0xd0;
    confirmation[2..4].copy_from_slice(&[1, 0]);
    confirmation[4..6].copy_from_slice(&[0, 1]);
    send(stream, &confirmation);
    let setup = receive(stream);
    assert_eq!(&setup[13..], &[0xf0, 0, 0, 1, 0, 1, 3, 0xc0]);
    let mut parameters = vec![0xf0, 0, 0, 1, 0, 1];
    parameters.extend_from_slice(&pdu.to_be_bytes());
    respond(stream, &setup, &parameters, &[]);
}

fn check_db1_request(request: &[u8], function: u8, byte_offset: u32, length: usize) {
    assert_eq!(request[13], function);
    assert_eq!(request[18], 2);
    assert_eq!(&request[19..21], &(length as u16).to_be_bytes());
    assert_eq!(&request[21..24], &[0, 1, 0x84]);
    assert_eq!(&request[24..27], &(byte_offset * 8).to_be_bytes()[1..]);
}

fn read_reply(stream: &mut TcpStream, offset: u32, bytes: &[u8]) {
    let request = receive(stream);
    check_db1_request(&request, 4, offset, bytes.len());
    let mut data = vec![0xff, 4];
    data.extend_from_slice(&((bytes.len() * 8) as u16).to_be_bytes());
    data.extend_from_slice(bytes);
    respond(stream, &request, &[4, 1], &data);
}

fn write_reply(stream: &mut TcpStream, offset: u32, bytes: &[u8]) {
    let request = receive(stream);
    check_db1_request(&request, 5, offset, bytes.len());
    assert_eq!(&request[31..], bytes);
    respond(stream, &request, &[5, 1], &[0xff]);
}

fn client(port: u16, model: S7Type, rack: usize, slot: usize) -> S7Net {
    S7Net::new(
        Ipv4Addr::LOCALHOST.into(),
        port,
        model,
        Timeout::default(),
        rack,
        slot,
    )
}

#[test]
fn all_cpu_types_negotiate_and_read_write_numeric_values() {
    for (model, rack, slot, local, remote, pdu) in [
        (S7Type::S1200, 0, 0, 0x0100, 0x0300, 240),
        (S7Type::S1500, 0, 0, 0x0100, 0x0300, 960),
        (S7Type::S300, 0, 2, 0x0100, 0x0302, 240),
        (S7Type::S400, 1, 3, 0x0100, 0x0323, 480),
        (S7Type::S200, 0, 0, 0x1000, 0x1001, 240),
        (S7Type::S200Smart, 0, 0, 0x0100, 0x0300, 960),
    ] {
        let (port, worker) = server(move |mut stream| {
            handshake(&mut stream, local, remote, pdu);
            read_reply(&mut stream, 100, &[0x12, 0x34]);
            write_reply(&mut stream, 100, &[0x56, 0x78]);
        });
        let mut plc = client(port, model, rack, slot);
        plc.connect().to_result().unwrap();
        assert_eq!(plc.s7_type(), model);
        assert_eq!(plc.negotiated_pdu_length(), Some(usize::from(pdu)));
        assert_eq!(
            &*plc.read::<u16>("DB1.100", 1).to_result().unwrap(),
            &[0x1234]
        );
        assert_eq!(
            plc.write::<u16>("DB1.100", 0x5678).to_result().unwrap(),
            0x5678
        );
        worker.join().unwrap();
    }
}

#[test]
fn custom_tsap_overrides_s200_defaults() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0x1001, 0x1002, 240);
        read_reply(&mut stream, 100, &[7]);
    });
    let mut plc = client(port, S7Type::S200, 0, 0).with_tsap(0x1001, 0x1002);
    plc.connect().to_result().unwrap();
    assert_eq!(&*plc.read::<u8>("VB100", 1).to_result().unwrap(), &[7]);
    worker.join().unwrap();
}

#[test]
fn s200_and_smart_use_single_byte_string_headers() {
    for (model, local, remote) in [
        (S7Type::S200, 0x1000, 0x1001),
        (S7Type::S200Smart, 0x0100, 0x0300),
    ] {
        let (port, worker) = server(move |mut stream| {
            handshake(&mut stream, local, remote, 240);
            read_reply(&mut stream, 10, &[5]);
            read_reply(&mut stream, 11, b"nihao");
            read_reply(&mut stream, 10, &[5]);
            // 单字节长度头无最大容量字段：允许在已确认预留空间内增长正文。
            write_reply(&mut stream, 10, &[6, b'a', b'b', b'c', b'd', b'e', b'f']);
            read_reply(&mut stream, 10, &[6]);
            write_reply(&mut stream, 10, &[0]);
            read_reply(&mut stream, 10, &[0]);
        });
        let mut plc = client(port, model, 0, 0);
        plc.connect().to_result().unwrap();
        assert_eq!(plc.read_s7_string("V10").to_result().unwrap(), "nihao");
        assert_eq!(
            plc.write::<String>("V10", "abcdef").to_result().unwrap(),
            "abcdef"
        );
        assert_eq!(plc.write::<String>("V10", "").to_result().unwrap(), "");
        assert_eq!(&*plc.read_s7_strings("V10").to_result().unwrap(), &[""]);
        worker.join().unwrap();
    }
}

#[test]
fn s300_and_s400_preserve_two_byte_string_capacity() {
    for (model, slot) in [(S7Type::S300, 2), (S7Type::S400, 3)] {
        let (port, worker) = server(move |mut stream| {
            handshake(&mut stream, 0x0100, 0x0300 | slot as u16, 480);
            read_reply(&mut stream, 10, &[10, 5]);
            read_reply(&mut stream, 12, b"nihao");
            read_reply(&mut stream, 10, &[10, 5]);
            write_reply(&mut stream, 11, &[2, b'h', b'i']);
        });
        let mut plc = client(port, model, 0, slot);
        plc.connect().to_result().unwrap();
        assert_eq!(plc.read_s7_string("DB1.10").to_result().unwrap(), "nihao");
        assert_eq!(
            plc.write::<String>("DB1.10", "hi").to_result().unwrap(),
            "hi"
        );
        worker.join().unwrap();
    }
}

#[test]
fn smart_v_area_supports_numeric_aliases_and_cross_byte_bits() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0x0100, 0x0300, 240);
        read_reply(&mut stream, 100, &[1]);
        read_reply(&mut stream, 100, &[0, 2]);
        read_reply(&mut stream, 100, &[0, 0, 0, 3]);
        for bit_address in [807_u32, 808] {
            let request = receive(&mut stream);
            assert_eq!(request[18], 1);
            assert_eq!(&request[21..24], &[0, 1, 0x84]);
            assert_eq!(&request[24..27], &bit_address.to_be_bytes()[1..]);
            respond(&mut stream, &request, &[4, 1], &[0xff, 3, 0, 1, 1]);
        }
        let request = receive(&mut stream);
        assert_eq!(request[13], 5);
        assert_eq!(request[18], 1);
        assert_eq!(&request[24..27], &800_u32.to_be_bytes()[1..]);
        assert_eq!(&request[27..], &[0, 3, 0, 1, 1]);
        respond(&mut stream, &request, &[5, 1], &[0xff]);
    });
    let mut plc = client(port, S7Type::S200Smart, 0, 0);
    plc.connect().to_result().unwrap();
    assert_eq!(&*plc.read::<u8>("VB100", 1).to_result().unwrap(), &[1]);
    assert_eq!(&*plc.read::<u16>("VW100", 1).to_result().unwrap(), &[2]);
    assert_eq!(&*plc.read::<u32>("VD100", 1).to_result().unwrap(), &[3]);
    assert_eq!(
        &*plc.read::<bool>("V100.7", 2).to_result().unwrap(),
        &[true, true]
    );
    assert!(plc.write::<bool>("V100", true).to_result().unwrap());
    worker.join().unwrap();
}

#[test]
fn changing_tsap_closes_existing_connection() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0x0100, 0x0300, 240);
        let mut data = [0];
        assert_eq!(stream.read(&mut data).unwrap(), 0);
    });
    let mut plc = client(port, S7Type::S200Smart, 0, 0);
    plc.connect().to_result().unwrap();
    let plc = plc.with_tsap(0x1000, 0x1001);
    assert!(!plc.is_connected());
    assert_eq!(plc.negotiated_pdu_length(), None);
    worker.join().unwrap();
}

#[test]
fn invalid_single_byte_string_header_is_rejected() {
    let (port, worker) = server(|mut stream| {
        handshake(&mut stream, 0x0100, 0x0300, 240);
        read_reply(&mut stream, 10, &[255]);
    });
    let mut plc = client(port, S7Type::S200Smart, 0, 0);
    plc.connect().to_result().unwrap();
    let result = plc.read_s7_string("V10");
    assert!(!result.is_success);
    assert!(result.msg.contains("STRING header"));
    worker.join().unwrap();
}
