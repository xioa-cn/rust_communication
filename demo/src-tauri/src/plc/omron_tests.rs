use super::{PlcClient, create_client, read, result, write};
use crate::models::{
    ConnectRequest, CpuModel, DataType, ModbusByteOrder, OmronOptions, PlcProtocol, ReadRequest,
    WriteMode, WriteRequest,
};
use rs_appliaction::communication::omron::ByteOrder;
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn config(protocol: PlcProtocol) -> ConnectRequest {
    ConnectRequest {
        protocol,
        host: "127.0.0.1".into(),
        port: 9600,
        cpu: CpuModel::S1200,
        rack: 0,
        slot: 1,
        connect_timeout_ms: 1000,
        receive_timeout_ms: 1000,
        local_tsap: None,
        remote_tsap: None,
        melsec: Default::default(),
        modbus: Default::default(),
        inovance: Default::default(),
        omron: OmronOptions {
            source_node: 20,
            destination_node: 10,
            source_network: 1,
            destination_network: 2,
            source_unit: 254,
            destination_unit: 0,
            gateway_count: 5,
            ..Default::default()
        },
        serial: Default::default(),
    }
}

fn binary(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|part| u8::from_str_radix(part, 16).unwrap())
        .collect()
}

fn frame(command: u32, payload: &[u8]) -> Vec<u8> {
    let mut data = b"FINS".to_vec();
    data.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
    data.extend_from_slice(&command.to_be_bytes());
    data.extend_from_slice(&[0; 4]);
    data.extend_from_slice(payload);
    data
}

fn read_frame(stream: &mut TcpStream) -> (u32, Vec<u8>) {
    let mut header = [0; 16];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(&header[..4], b"FINS");
    let length = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
    assert!((8..=2020).contains(&length));
    let mut data = vec![0; length - 8];
    stream.read_exact(&mut data).unwrap();
    (u32::from_be_bytes(header[8..12].try_into().unwrap()), data)
}

fn steps() -> Vec<(Vec<u8>, Vec<u8>)> {
    [
        ("01 01 30 00 64 0F 00 02", "01 00"),
        ("01 01 82 00 64 00 00 02", "12 34 AB CD"),
        ("01 01 82 00 66 00 00 02", "00 00 3F 80"),
        ("01 01 82 00 68 00 00 04", "FF FF FF FF FF FF FF FF"),
        ("01 01 82 00 6A 00 00 02", "41 42 43 44"),
        ("01 01 82 00 6E 00 00 02", "E4 BD A0 58"),
        ("01 02 30 00 64 0F 00 01 01", ""),
        ("01 02 82 00 64 00 00 02 00 01 00 02", ""),
        ("01 02 82 00 C8 00 00 02 FF FE FF FF", ""),
        ("01 01 82 01 2D 00 00 01", "58 5A"),
        ("01 02 82 01 2C 00 00 02 41 42 43 5A", ""),
        ("01 01 82 01 91 00 00 01", "00 99"),
        ("01 02 82 01 90 00 00 02 01 02 03 99", ""),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (body, payload))| {
        let mut request = vec![0x80, 0, 5, 2, 10, 0, 1, 20, 254, index as u8 + 1];
        request.extend_from_slice(&binary(body));
        let mut reply = vec![0xc0, 0, 5, 1, 20, 254, 2, 10, 0, index as u8 + 1];
        reply.extend_from_slice(&request[10..12]);
        reply.extend_from_slice(&[0, 0]);
        reply.extend_from_slice(&binary(payload));
        (request, reply)
    })
    .collect()
}

fn tcp_server(steps: Vec<(Vec<u8>, Vec<u8>)>) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream.set_nodelay(true).unwrap();
        assert_eq!(read_frame(&mut stream), (0, vec![0, 0, 0, 20]));
        stream
            .write_all(&frame(1, &[0, 0, 0, 20, 0, 0, 0, 10]))
            .unwrap();
        for (expected, reply) in steps {
            assert_eq!(read_frame(&mut stream), (2, expected));
            stream.write_all(&frame(2, &reply)).unwrap();
        }
    });
    (port, worker)
}

fn udp_server(steps: Vec<(Vec<u8>, Vec<u8>)>) -> (u16, JoinHandle<()>) {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = socket.local_addr().unwrap().port();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let worker = thread::spawn(move || {
        let mut buffer = [0; 2048];
        for (expected, reply) in steps {
            let (count, peer) = socket.recv_from(&mut buffer).unwrap();
            assert_eq!(&buffer[..count], &expected);
            socket.send_to(&reply, peer).unwrap();
        }
    });
    (port, worker)
}

fn read_request(address: &str, data_type: DataType, length: usize) -> ReadRequest {
    ReadRequest {
        address: address.into(),
        data_type,
        length: Some(length),
    }
}

fn write_request(address: &str, data_type: DataType, values: &[&str]) -> WriteRequest {
    WriteRequest {
        address: address.into(),
        data_type,
        mode: if values.len() > 1 {
            WriteMode::Array
        } else {
            WriteMode::Single
        },
        values: values.iter().map(|value| (*value).to_owned()).collect(),
        confirmed: true,
    }
}

fn exercise(client: &mut PlcClient) {
    result(client.connect()).unwrap();
    assert!(client.is_connected());
    assert!(client.cpu().is_none());
    assert!(client.negotiated_pdu_length().is_none());
    let mut unconfirmed = write_request("D100", DataType::U16, &["1"]);
    unconfirmed.confirmed = false;
    assert!(write(client, &unconfirmed).is_err());
    assert!(
        write(
            client,
            &write_request("D100", DataType::U16, &["1", "invalid"])
        )
        .is_err()
    );
    assert!(read(client, &read_request("D100", DataType::S7String, 1)).is_err());
    assert!(write(client, &write_request("D100", DataType::S7String, &["ABC"])).is_err());
    assert!(read(client, &read_request("DB1.0", DataType::U16, 1)).is_err());
    assert_eq!(
        &*read(client, &read_request("CIO100.15", DataType::Bool, 2)).unwrap(),
        &["true", "false"]
    );
    assert_eq!(
        &*read(client, &read_request("D100", DataType::U16, 2)).unwrap(),
        &["4660", "43981"]
    );
    assert_eq!(
        &*read(client, &read_request("D102", DataType::F32, 1)).unwrap(),
        &["1"]
    );
    assert_eq!(
        &*read(client, &read_request("D104", DataType::U64, 1)).unwrap(),
        &["18446744073709551615"]
    );
    assert_eq!(
        &*read(client, &read_request("D106", DataType::U8, 3)).unwrap(),
        &["65", "66", "67"]
    );
    assert_eq!(
        &*read(client, &read_request("D110", DataType::RawString, 3)).unwrap(),
        &["你"]
    );
    assert_eq!(
        write(
            client,
            &write_request("CIO100.15", DataType::Bool, &["true"])
        )
        .unwrap(),
        (1, "元素")
    );
    assert_eq!(
        write(client, &write_request("D100", DataType::U16, &["1", "2"])).unwrap(),
        (2, "元素")
    );
    assert_eq!(
        write(client, &write_request("D200", DataType::I32, &["-2"])).unwrap(),
        (1, "元素")
    );
    assert_eq!(
        write(
            client,
            &write_request("D300", DataType::RawString, &["ABC"])
        )
        .unwrap(),
        (3, "字节")
    );
    assert_eq!(
        write(
            client,
            &write_request("D400", DataType::U8, &["1", "2", "3"])
        )
        .unwrap(),
        (3, "元素")
    );
    result(client.disconnect()).unwrap();
    assert!(!client.is_connected());
    result(client.disconnect()).unwrap();
}

#[test]
fn tcp_dispatches_configured_routes_reads_writes_bytes_and_text() {
    let (port, worker) = tcp_server(steps());
    let mut request = config(PlcProtocol::OmronFinsTcp);
    request.port = port;
    let mut client = create_client(request).unwrap();
    assert!(!client.is_connected());
    assert_eq!(client.protocol(), PlcProtocol::OmronFinsTcp);
    exercise(&mut client);
    worker.join().unwrap();
}

#[test]
fn udp_dispatches_configured_routes_reads_writes_bytes_and_text() {
    let (port, worker) = udp_server(steps());
    let mut request = config(PlcProtocol::OmronFinsUdp);
    request.port = port;
    let mut client = create_client(request).unwrap();
    assert!(!client.is_connected());
    assert_eq!(client.protocol(), PlcProtocol::OmronFinsUdp);
    exercise(&mut client);
    worker.join().unwrap();
}

#[test]
fn omron_ipc_names_defaults_and_byte_orders_match_frontend() {
    for (name, protocol) in [
        ("omron_fins_tcp", PlcProtocol::OmronFinsTcp),
        ("omron_fins_udp", PlcProtocol::OmronFinsUdp),
    ] {
        let value = serde::de::value::StrDeserializer::<serde::de::value::Error>::new(name);
        assert_eq!(PlcProtocol::deserialize(value).unwrap(), protocol);
        assert!(protocol.is_omron());
        assert!(!protocol.is_modbus());
        assert!(!protocol.is_serial());
        for (setting, expected) in [
            (ModbusByteOrder::Abcd, ByteOrder::ABCD),
            (ModbusByteOrder::Badc, ByteOrder::BADC),
            (ModbusByteOrder::Cdab, ByteOrder::CDAB),
            (ModbusByteOrder::Dcba, ByteOrder::DCBA),
        ] {
            let mut request = config(protocol);
            request.omron.byte_order = setting;
            let client = create_client(request).unwrap();
            let (order, route) = match client {
                PlcClient::OmronFinsTcp(client) => (client.byte_order(), client.route()),
                PlcClient::OmronFinsUdp(client) => (client.byte_order(), client.route()),
                _ => panic!("wrong protocol"),
            };
            assert_eq!(order, expected);
            assert_eq!(route.source_node, 20);
            assert_eq!(route.destination_node, 10);
            assert_eq!(route.gateway_count, 5);
        }
    }
    let options = OmronOptions::default();
    assert!(matches!(options.byte_order, ModbusByteOrder::Cdab));
    assert_eq!(options.source_node, 0);
    assert_eq!(options.destination_node, 0);
    assert_eq!(options.gateway_count, 2);
}

#[test]
fn omron_configuration_rejects_invalid_ipv4_routes_and_ignores_other_protocol_fields() {
    for protocol in [PlcProtocol::OmronFinsTcp, PlcProtocol::OmronFinsUdp] {
        let mut request = config(protocol);
        request.rack = 99;
        request.slot = 99;
        request.local_tsap = Some("invalid".into());
        request.modbus.unit_id = 0;
        assert!(!create_client(request).unwrap().is_connected());
        for host in [
            "not-an-ip",
            "::1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
        ] {
            let mut request = config(protocol);
            request.host = host.into();
            assert!(create_client(request).is_err());
        }
        let invalid: [fn(&mut ConnectRequest); 7] = [
            |request| request.omron.source_node = 255,
            |request| request.omron.destination_node = 255,
            |request| request.omron.source_network = 128,
            |request| request.omron.destination_network = 128,
            |request| request.omron.destination_node = 0,
            |request| request.omron.gateway_count = 8,
            |request| request.receive_timeout_ms = 0,
        ];
        for change in invalid {
            let mut request = config(protocol);
            change(&mut request);
            assert!(create_client(request).is_err());
        }
    }
}

#[test]
fn bad_fins_response_updates_the_demo_session_status() {
    let mut exchange = steps().remove(0);
    exchange.1[9] = 99;
    let (port, worker) = tcp_server(vec![exchange]);
    let mut request = config(PlcProtocol::OmronFinsTcp);
    request.port = port;
    let mut client = create_client(request).unwrap();
    result(client.connect()).unwrap();
    assert!(read(&mut client, &read_request("CIO100.15", DataType::Bool, 2)).is_err());
    let status = crate::state::snapshot(&Some(client));
    assert!(!status.connected);
    assert_eq!(status.protocol, Some(PlcProtocol::OmronFinsTcp));
    assert!(status.cpu.is_none());
    assert!(status.pdu_length.is_none());
    worker.join().unwrap();
}
