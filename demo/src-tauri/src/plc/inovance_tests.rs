use super::{PlcClient, create_client, read, result, write};
use crate::models::{ConnectRequest, DataType, PlcProtocol, ReadRequest, WriteMode, WriteRequest};
use serde_json::json;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::thread;
use std::time::Duration;

fn config(series: &str, port: u16) -> ConnectRequest {
    serde_json::from_value(json!({
        "protocol": "inovance_modbus_tcp", "host": "127.0.0.1", "port": port,
        "connectTimeoutMs": 1000, "receiveTimeoutMs": 1000,
        "inovance": { "series": series, "unitId": 9, "byteOrder": "BADC" },
        "rack": 999, "slot": 999, "localTsap": "invalid", "modbus": { "unitId": 0 },
    }))
    .unwrap()
}

fn binary(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn write_request(address: &str, data_type: DataType, values: &[&str]) -> WriteRequest {
    WriteRequest {
        address: address.into(),
        data_type,
        mode: if values.len() == 1 {
            WriteMode::Single
        } else {
            WriteMode::Array
        },
        values: values.iter().map(|value| (*value).to_owned()).collect(),
        confirmed: true,
    }
}

#[test]
fn inovance_ipc_models_defaults_validation_and_session_metadata() {
    for series in ["AM", "AC", "AP", "EVO", "H3U", "H5U", "Easy"] {
        let client = create_client(config(series, 502)).unwrap();
        assert!(!client.is_connected());
        assert_eq!(client.protocol(), PlcProtocol::InovanceModbusTcp);
        assert_eq!(client.inovance_series().as_deref(), Some(series));
        assert!(client.cpu().is_none());
        assert!(client.negotiated_pdu_length().is_none());
        let snapshot = serde_json::to_value(crate::state::snapshot(&Some(client))).unwrap();
        assert_eq!(snapshot["inovanceSeries"], series);
        assert_eq!(snapshot["protocol"], "inovance_modbus_tcp");
    }
    let default: ConnectRequest = serde_json::from_value(json!({ "protocol": "inovance_modbus_tcp", "host": "127.0.0.1", "port": 502, "connectTimeoutMs": 1, "receiveTimeoutMs": 1 })).unwrap();
    match create_client(default).unwrap() {
        PlcClient::InovanceModbusTcp(client) => {
            assert_eq!(format!("{:?}", client.series()), "AM");
            assert_eq!(client.unit_id(), 1);
            assert_eq!(format!("{:?}", client.byte_order()), "CDAB");
        }
        _ => panic!("Wrong client"),
    }
    for options in [
        json!({"series":"unknown"}),
        json!({"unitId":256}),
        json!({"byteOrder":"unknown"}),
    ] {
        assert!(serde_json::from_value::<crate::models::InovanceOptions>(options).is_err());
    }
    let mut invalid = config("H3U", 502);
    invalid.inovance.unit_id = 0;
    assert!(create_client(invalid).is_err());
    assert!(create_client(config("H3U", 0)).is_err());
}

#[test]
fn all_inovance_models_dispatch_demo_reads_writes_and_preserve_neighbor_bytes() {
    for series in ["AM", "AC", "AP", "EVO", "H3U", "H5U", "Easy"] {
        let iec = ["AM", "AC", "AP", "EVO"].contains(&series);
        let steps = vec![
            (
                if iec {
                    "03 00 00 00 01"
                } else {
                    "01 F8 08 00 01"
                },
                if iec { "03 02 80 00" } else { "01 01 01" },
            ),
            ("03 00 64 00 02", "03 04 34 12 78 56"),
            ("10 00 C8 00 02 04 01 00 02 00", "10 00 C8 00 02"),
            ("03 01 2C 00 02", "03 04 42 41 00 43"),
            ("03 01 91 00 01", "03 02 58 59"),
            ("10 01 90 00 02 04 42 41 58 43", "10 01 90 00 02"),
        ];
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
                let mut header = [0; 7];
                stream.read_exact(&mut header).unwrap();
                assert_eq!(header[6], 9);
                let mut pdu = vec![0; usize::from(u16::from_be_bytes([header[4], header[5]])) - 1];
                stream.read_exact(&mut pdu).unwrap();
                assert_eq!(pdu, binary(expected));
                let response = binary(response);
                header[4..6].copy_from_slice(&((response.len() + 1) as u16).to_be_bytes());
                stream.write_all(&header).unwrap();
                stream.write_all(&response).unwrap();
            }
        });
        let mut client = create_client(config(series, port)).unwrap();
        result(client.connect()).unwrap();
        let word = |offset| format!("{}{offset}", if iec { "MW" } else { "D" });
        let mut unconfirmed = write_request(&word(0), DataType::U16, &["1"]);
        unconfirmed.confirmed = false;
        assert!(write(&mut client, &unconfirmed).is_err());
        assert!(
            write(
                &mut client,
                &write_request(&word(0), DataType::U16, &["1", "bad"])
            )
            .is_err()
        );
        assert!(
            write(
                &mut client,
                &write_request(if iec { "IX0.0" } else { "X0" }, DataType::Bool, &["true"])
            )
            .is_err()
        );
        assert!(
            read(
                &mut client,
                &ReadRequest {
                    address: word(0),
                    data_type: DataType::S7String,
                    length: None
                }
            )
            .is_err()
        );
        assert_eq!(
            &*read(
                &mut client,
                &ReadRequest {
                    address: if iec { "MX1.7" } else { "X10" }.into(),
                    data_type: DataType::Bool,
                    length: Some(1)
                }
            )
            .unwrap(),
            &["true"]
        );
        assert_eq!(
            &*read(
                &mut client,
                &ReadRequest {
                    address: word(100),
                    data_type: DataType::U32,
                    length: Some(1)
                }
            )
            .unwrap(),
            &["305419896"]
        );
        assert_eq!(
            write(
                &mut client,
                &write_request(&word(200), DataType::U16, &["1", "2"])
            )
            .unwrap()
            .0,
            2
        );
        assert_eq!(
            &*read(
                &mut client,
                &ReadRequest {
                    address: word(300),
                    data_type: DataType::U8,
                    length: Some(3)
                }
            )
            .unwrap(),
            &["65", "66", "67"]
        );
        assert_eq!(
            write(
                &mut client,
                &write_request(&word(400), DataType::RawString, &["ABC"])
            )
            .unwrap()
            .0,
            3
        );
        result(client.disconnect()).unwrap();
        worker.join().unwrap();
    }
}

#[test]
fn inovance_invalid_response_marks_demo_session_disconnected() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 12];
        stream.read_exact(&mut request).unwrap();
        let mut response = request[..7].to_vec();
        response[4..6].copy_from_slice(&5_u16.to_be_bytes());
        response.extend_from_slice(&[4, 2, 0, 1]);
        stream.write_all(&response).unwrap();
    });
    let mut client = create_client(config("EVO", port)).unwrap();
    result(client.connect()).unwrap();
    assert!(
        read(
            &mut client,
            &ReadRequest {
                address: "MW0".into(),
                data_type: DataType::U16,
                length: Some(1)
            }
        )
        .is_err()
    );
    assert!(!crate::state::snapshot(&Some(client)).connected);
    worker.join().unwrap();
}
