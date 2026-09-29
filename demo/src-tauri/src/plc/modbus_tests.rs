use super::{PlcClient, create_client, read, result, write};
use crate::models::{
    ConnectRequest, CpuModel, DataType, ModbusByteOrder, PlcProtocol, ReadRequest, WriteMode,
    WriteRequest,
};
use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, UdpSocket};
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn config(protocol: PlcProtocol) -> ConnectRequest {
    ConnectRequest {
        protocol,
        host: "127.0.0.1".into(),
        port: 502,
        cpu: CpuModel::S1200,
        rack: 0,
        slot: 1,
        connect_timeout_ms: 1000,
        receive_timeout_ms: 1000,
        local_tsap: None,
        remote_tsap: None,
        melsec: Default::default(),
        modbus: Default::default(),
        omron: Default::default(),
        inovance: Default::default(),
        serial: Default::default(),
    }
}

fn read_request(address: &str, data_type: DataType, length: usize) -> ReadRequest {
    ReadRequest {
        address: address.into(),
        data_type,
        length: Some(length),
    }
}

fn write_request(address: &str, data_type: DataType, values: Vec<String>) -> WriteRequest {
    WriteRequest {
        address: address.into(),
        data_type,
        mode: if values.len() == 1 {
            WriteMode::Single
        } else {
            WriteMode::Array
        },
        values,
        confirmed: true,
    }
}

fn binary(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|part| u8::from_str_radix(part, 16).unwrap())
        .collect()
}

fn frame(transaction: u16, pdu: &[u8]) -> Vec<u8> {
    frame_for_unit(transaction, 7, pdu)
}

fn frame_for_unit(transaction: u16, unit: u8, pdu: &[u8]) -> Vec<u8> {
    let mut frame = transaction.to_be_bytes().to_vec();
    frame.extend_from_slice(&[0, 0, 0, (pdu.len() + 1) as u8, unit]);
    frame.extend_from_slice(pdu);
    frame
}

fn steps() -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut steps: Vec<_> = [
        ("02 00 00 00 03", "02 01 05"),
        ("03 00 0A 00 02", "03 04 33 44 11 22"),
        ("03 00 32 00 04", "03 08 FF FF FF FF FF FF FF FF"),
        ("06 00 14 FF FF", "06 00 14 FF FF"),
        (
            "10 00 1E 00 04 08 33 44 11 22 77 88 55 66",
            "10 00 1E 00 04",
        ),
        (
            "10 00 28 00 04 08 FF FF FF FF FF FF FF FF",
            "10 00 28 00 04",
        ),
        ("03 00 64 00 03", "03 06 E4 BD A0 E5 A5 BD"),
        ("03 00 65 00 01", "03 02 58 5A"),
        ("10 00 64 00 02 04 E4 BD A0 5A", "10 00 64 00 02"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (request, response))| {
        (
            frame(index as u16 + 1, &binary(request)),
            frame(index as u16 + 1, &binary(response)),
        )
    })
    .collect();
    steps.push((
        frame_for_unit(10, 2, &binary("03 00 64 00 03")),
        frame_for_unit(10, 2, &binary("03 06 68 65 6C 6C 6F 21")),
    ));
    steps.push((
        frame(11, &binary("03 00 64 00 01")),
        frame(11, &binary("03 02 00 07")),
    ));
    steps
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
        for (expected, response) in steps {
            let mut actual = vec![0; expected.len()];
            stream.read_exact(&mut actual).unwrap();
            assert_eq!(actual, expected);
            stream.write_all(&response).unwrap();
        }
    });
    (port, worker)
}

fn exercise(client: &mut PlcClient) {
    assert!(result(client.connect()).unwrap());
    for data_type in [DataType::U8, DataType::I8, DataType::S7String] {
        assert!(
            read(client, &read_request("HR0", data_type, 1))
                .unwrap_err()
                .contains("Modbus")
        );
        assert!(
            write(client, &write_request("HR0", data_type, vec!["1".into()]))
                .unwrap_err()
                .contains("Modbus")
        );
    }
    assert!(
        write(
            client,
            &write_request("IR0", DataType::U16, vec!["1".into()])
        )
        .is_err()
    );
    assert!(
        write(
            client,
            &write_request("DI0", DataType::Bool, vec!["true".into()])
        )
        .is_err()
    );
    assert!(
        write(
            client,
            &write_request("x=2;IR0", DataType::RawString, vec!["AB".into()])
        )
        .is_err()
    );
    let mut unconfirmed = write_request("HR0", DataType::U16, vec!["1".into()]);
    unconfirmed.confirmed = false;
    assert!(write(client, &unconfirmed).unwrap_err().contains("确认"));
    assert!(
        write(
            client,
            &write_request("HR0", DataType::U16, vec!["1".into(), "invalid".into()])
        )
        .is_err()
    );
    assert_eq!(
        &*read(client, &read_request("DI0", DataType::Bool, 3)).unwrap(),
        &["true", "false", "true"]
    );
    assert_eq!(
        &*read(client, &read_request("HR10", DataType::U32, 1)).unwrap(),
        &[0x11223344_u32.to_string()]
    );
    assert_eq!(
        &*read(client, &read_request("HR50", DataType::U64, 1)).unwrap(),
        &[u64::MAX.to_string()]
    );
    assert_eq!(
        write(
            client,
            &write_request("HR20", DataType::U16, vec![u16::MAX.to_string()])
        )
        .unwrap(),
        (1, "元素")
    );
    assert_eq!(
        write(
            client,
            &write_request(
                "HR30",
                DataType::U32,
                vec![0x11223344_u32.to_string(), 0x55667788_u32.to_string()]
            )
        )
        .unwrap(),
        (2, "元素")
    );
    assert_eq!(
        write(
            client,
            &write_request("HR40", DataType::U64, vec![u64::MAX.to_string()])
        )
        .unwrap(),
        (1, "元素")
    );
    assert_eq!(
        &*read(client, &read_request("HR100", DataType::RawString, 6)).unwrap(),
        &["你好"]
    );
    assert_eq!(
        write(
            client,
            &write_request("HR100", DataType::RawString, vec!["你".into()])
        )
        .unwrap(),
        (3, "字节")
    );
    assert_eq!(
        &*read(client, &read_request("x=2;100", DataType::RawString, 5)).unwrap(),
        &["hello"]
    );
    assert_eq!(
        &*read(client, &read_request("HR100", DataType::U16, 1)).unwrap(),
        &["7"]
    );
    assert!(client.is_connected());
    assert!(client.cpu().is_none());
    assert!(client.negotiated_pdu_length().is_none());
    result(client.disconnect()).unwrap();
    assert!(!client.is_connected());
}

#[test]
fn tcp_demo_reads_writes_and_honors_unit_and_byte_order() {
    let (port, worker) = tcp_server(steps());
    let mut request = config(PlcProtocol::ModbusTcp);
    request.port = port;
    request.modbus.unit_id = 7;
    request.modbus.byte_order = ModbusByteOrder::Cdab;
    exercise(&mut create_client(request).unwrap());
    worker.join().unwrap();
}

#[test]
fn udp_demo_reads_writes_and_honors_unit_and_byte_order() {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut buffer = [0; 260];
        for (expected, response) in steps() {
            let (count, peer) = socket.recv_from(&mut buffer).unwrap();
            assert_eq!(&buffer[..count], expected);
            socket.send_to(&response, peer).unwrap();
        }
    });
    let mut request = config(PlcProtocol::ModbusUdp);
    request.port = port;
    request.modbus.unit_id = 7;
    request.modbus.byte_order = ModbusByteOrder::Cdab;
    exercise(&mut create_client(request).unwrap());
    worker.join().unwrap();
}

#[test]
fn all_twelve_protocols_construct_without_opening_resources() {
    for protocol in [
        PlcProtocol::S7,
        PlcProtocol::McBinary,
        PlcProtocol::McAscii,
        PlcProtocol::McUdpBinary,
        PlcProtocol::McUdpAscii,
        PlcProtocol::A1eBinary,
        PlcProtocol::A1eAscii,
        PlcProtocol::McRBinary,
        PlcProtocol::ModbusTcp,
        PlcProtocol::ModbusUdp,
        PlcProtocol::ModbusRtu,
        PlcProtocol::ModbusAscii,
    ] {
        let mut request = config(protocol);
        request.serial.path = "__nonexistent_demo_test_port__".into();
        let mut client = create_client(request).unwrap();
        assert_eq!(client.protocol(), protocol);
        assert!(!client.is_connected());
        result(client.disconnect()).unwrap();
    }
}

#[test]
fn serial_modes_ignore_stale_network_and_s7_configuration() {
    for protocol in [PlcProtocol::ModbusRtu, PlcProtocol::ModbusAscii] {
        let mut request = config(protocol);
        request.host = "not an IP".into();
        request.port = 0;
        request.rack = 99;
        request.slot = 99;
        request.local_tsap = Some("invalid".into());
        assert!(!create_client(request).unwrap().is_connected());
    }
}

#[test]
fn invalid_serial_parameters_are_rejected_before_opening() {
    let mut request = config(PlcProtocol::ModbusRtu);
    request.serial.data_bits = 7;
    assert!(create_client(request).is_err());
    let mut request = config(PlcProtocol::ModbusAscii);
    request.serial.data_bits = 7;
    assert!(create_client(request).is_ok());
    for protocol in [PlcProtocol::ModbusRtu, PlcProtocol::ModbusAscii] {
        let mut request = config(protocol);
        request.serial.path = " ".into();
        assert!(create_client(request).is_err());
        let mut request = config(protocol);
        request.serial.baud_rate = 0;
        assert!(create_client(request).is_err());
        let mut request = config(protocol);
        request.serial.stop_bits = 3;
        assert!(create_client(request).is_err());
        let mut request = config(protocol);
        request.modbus.unit_id = 248;
        assert!(create_client(request).is_err());
        let mut request = config(protocol);
        request.receive_timeout_ms = 0;
        assert!(create_client(request).is_err());
    }
}

#[test]
fn broadcasts_and_invalid_network_configuration_are_rejected() {
    for protocol in [
        PlcProtocol::ModbusTcp,
        PlcProtocol::ModbusUdp,
        PlcProtocol::ModbusRtu,
        PlcProtocol::ModbusAscii,
    ] {
        let mut request = config(protocol);
        request.modbus.unit_id = 0;
        assert!(create_client(request).is_err());
    }
    for protocol in [PlcProtocol::ModbusTcp, PlcProtocol::ModbusUdp] {
        let mut request = config(protocol);
        request.modbus.unit_id = 255;
        request.local_tsap = Some("invalid".into());
        request.rack = 99;
        assert!(create_client(request).is_ok());
        let mut request = config(protocol);
        request.port = 0;
        assert!(create_client(request).is_err());
        let mut request = config(protocol);
        request.host = "invalid".into();
        assert!(create_client(request).is_err());
    }
}

#[test]
fn ipc_protocol_names_match_frontend_values() {
    for (name, expected) in [
        ("modbus_tcp", PlcProtocol::ModbusTcp),
        ("modbus_udp", PlcProtocol::ModbusUdp),
        ("modbus_rtu", PlcProtocol::ModbusRtu),
        ("modbus_ascii", PlcProtocol::ModbusAscii),
    ] {
        let value = serde::de::value::StrDeserializer::<serde::de::value::Error>::new(name);
        assert_eq!(PlcProtocol::deserialize(value).unwrap(), expected);
    }
}

#[test]
fn malformed_response_updates_session_status_to_disconnected() {
    let (port, worker) = tcp_server(vec![(
        frame(1, &binary("03 00 00 00 01")),
        frame(2, &binary("03 02 00 01")),
    )]);
    let mut request = config(PlcProtocol::ModbusTcp);
    request.port = port;
    request.modbus.unit_id = 7;
    let mut client = create_client(request).unwrap();
    result(client.connect()).unwrap();
    assert!(read(&mut client, &read_request("HR0", DataType::U16, 1)).is_err());
    let status = crate::state::snapshot(&Some(client));
    assert!(!status.connected);
    assert_eq!(status.protocol, Some(PlcProtocol::ModbusTcp));
    worker.join().unwrap();
}
