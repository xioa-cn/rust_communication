use rs_appliaction::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use rs_appliaction::communication::omron::{
    ByteOrder, FinsRoute, FinsTransport, OmronClient, OmronValue,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

type Requests = Arc<Mutex<Vec<Vec<u8>>>>;

enum Reply {
    Data(Vec<u8>),
    EndCode(u16),
    Invalid(usize, u8),
    Failure,
}

struct TestTransport {
    replies: VecDeque<Reply>,
    requests: Requests,
    connected: bool,
}

impl FinsTransport for TestTransport {
    fn connect(&mut self, route: FinsRoute) -> Result<FinsRoute, String> {
        self.connected = true;
        Ok(FinsRoute {
            source_node: if route.source_node == 0 {
                20
            } else {
                route.source_node
            },
            destination_node: if route.destination_node == 0 {
                10
            } else {
                route.destination_node
            },
            ..route
        })
    }
    fn disconnect(&mut self) {
        self.connected = false;
    }
    fn is_connected(&self) -> bool {
        self.connected
    }
    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, String> {
        self.requests.lock().unwrap().push(request.to_vec());
        let mut reply = vec![0xc0, 0, 2];
        reply.extend_from_slice(&request[6..9]);
        reply.extend_from_slice(&request[3..6]);
        reply.extend_from_slice(&request[9..12]);
        reply.extend_from_slice(&[0, 0]);
        match self.replies.pop_front().expect("Unexpected FINS request") {
            Reply::Data(data) => reply.extend_from_slice(&data),
            Reply::EndCode(code) => reply[12..14].copy_from_slice(&code.to_be_bytes()),
            Reply::Invalid(offset, value) => {
                reply[offset] = value;
                reply.extend_from_slice(&[0, 1]);
            }
            Reply::Failure => return Err("Mock I/O failure".into()),
        }
        Ok(reply)
    }
}

fn client(replies: Vec<Reply>) -> (OmronClient<TestTransport>, Requests) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let transport = TestTransport {
        replies: replies.into(),
        requests: Arc::clone(&requests),
        connected: false,
    };
    let mut client = OmronClient::from_transport(transport);
    assert!(client.connect().to_result().unwrap());
    (client, requests)
}

#[test]
fn shared_traits_and_inherent_methods_match_the_existing_api() {
    let (mut plc, requests) = client(vec![
        Reply::Data(vec![0x12, 0x34]),
        Reply::Data(vec![]),
        Reply::Data(vec![]),
    ]);
    assert!(DeviceBase::connect(&mut plc).to_result().unwrap());
    assert_eq!(
        &*ReadBase::<u16>::read(&mut plc, "D100", 1)
            .to_result()
            .unwrap(),
        &[0x1234]
    );
    assert_eq!(
        WriteBase::<i16>::write(&mut plc, "D100", -2)
            .to_result()
            .unwrap(),
        -2
    );
    assert_eq!(
        WriteBase::<u16>::write_all(&mut plc, "D200", &[1, 2])
            .to_result()
            .unwrap(),
        2
    );
    let recorded = requests.lock().unwrap();
    assert_eq!(
        recorded[0],
        vec![
            0x80, 0, 2, 0, 10, 0, 0, 20, 0, 1, 1, 1, 0x82, 0, 100, 0, 0, 1
        ]
    );
    assert_eq!(
        &recorded[1][10..],
        &[1, 2, 0x82, 0, 100, 0, 0, 1, 0xff, 0xfe]
    );
    assert_eq!(
        &recorded[2][10..],
        &[1, 2, 0x82, 0, 200, 0, 0, 2, 0, 1, 0, 2]
    );
    assert!(DeviceBase::disconnect(&mut plc).to_result().unwrap());
    assert!(DeviceBase::disconnect(&mut plc).to_result().unwrap());
}

#[test]
fn address_areas_and_em_banks_use_the_documented_codes() {
    for (area, word_code, bit_code) in [
        ("D100", 0x82, 0x02),
        ("DM100", 0x82, 0x02),
        ("CIO100", 0xb0, 0x30),
        ("C100", 0xb0, 0x30),
        ("W100", 0xb1, 0x31),
        ("WR100", 0xb1, 0x31),
        ("H100", 0xb2, 0x32),
        ("HR100", 0xb2, 0x32),
        ("A100", 0xb3, 0x33),
        ("AR100", 0xb3, 0x33),
        ("E0.100", 0xa0, 0x20),
        ("EF.100", 0xaf, 0x2f),
        ("EM10.100", 0x60, 0xe0),
        ("E18.100", 0x68, 0xe8),
    ] {
        let (mut plc, requests) = client(vec![Reply::Data(vec![0, 5]), Reply::Data(vec![1])]);
        assert_eq!(
            &*plc
                .read::<u16>(&area.to_lowercase(), 1)
                .to_result()
                .unwrap(),
            &[5]
        );
        assert_eq!(
            &*plc
                .read::<bool>(&format!("{area}.15"), 1)
                .to_result()
                .unwrap(),
            &[true]
        );
        let recorded = requests.lock().unwrap();
        assert_eq!(&recorded[0][12..], &[word_code, 0, 100, 0, 0, 1]);
        assert_eq!(&recorded[1][12..], &[bit_code, 0, 100, 15, 0, 1]);
    }
}

#[test]
fn numeric_byte_orders_have_known_wire_values() {
    for (order, bytes) in [
        (ByteOrder::ABCD, vec![0x12, 0x34, 0x56, 0x78]),
        (ByteOrder::BADC, vec![0x34, 0x12, 0x78, 0x56]),
        (ByteOrder::CDAB, vec![0x56, 0x78, 0x12, 0x34]),
        (ByteOrder::DCBA, vec![0x78, 0x56, 0x34, 0x12]),
    ] {
        let (mut plc, requests) = client(vec![Reply::Data(bytes.clone()), Reply::Data(vec![])]);
        assert_eq!(plc.byte_order(), ByteOrder::CDAB);
        plc.set_byte_order(order);
        assert_eq!(
            &*plc.read::<u32>("D100", 1).to_result().unwrap(),
            &[0x12345678]
        );
        assert_eq!(
            plc.write("D100", 0x12345678_u32).to_result().unwrap(),
            0x12345678
        );
        assert_eq!(&requests.lock().unwrap()[1][18..], &bytes);
    }
}

#[test]
fn all_supported_numeric_types_round_trip() {
    macro_rules! round_trip {
        ($value_type:ty, $value:expr) => {{
            let value = $value;
            let mut bytes = value.to_be_bytes().to_vec();
            if bytes.len() == 1 {
                bytes.push(0);
            }
            let (mut plc, requests) = client(vec![Reply::Data(bytes.clone()), Reply::Data(vec![])]);
            plc.set_byte_order(ByteOrder::ABCD);
            let values = plc
                .read::<$value_type>(
                    "D0",
                    if std::mem::size_of_val(&value) == 1 {
                        2
                    } else {
                        1
                    },
                )
                .to_result()
                .unwrap();
            assert_eq!(values[0], value);
            if std::mem::size_of_val(&value) == 1 {
                plc.write_all("D0", &[value, value]).to_result().unwrap();
            } else {
                assert_eq!(plc.write("D0", value).to_result().unwrap(), value);
            }
            assert_eq!(requests.lock().unwrap().len(), 2);
        }};
    }
    round_trip!(u8, 0xab_u8);
    round_trip!(i8, -12_i8);
    round_trip!(u16, 60000_u16);
    round_trip!(i16, -20000_i16);
    round_trip!(u32, 0x12345678_u32);
    round_trip!(i32, -12345678_i32);
    round_trip!(u64, u64::MAX);
    round_trip!(i64, i64::MIN);
    round_trip!(f32, 1.25_f32);
    round_trip!(f64, -42.5_f64);
}

#[test]
fn word_and_bit_reads_split_and_advance_in_their_own_units() {
    let words: Vec<u16> = (0..501).collect();
    let (mut plc, requests) = client(vec![
        Reply::Data(
            words[..500]
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .collect(),
        ),
        Reply::Data(vec![1, 244]),
    ]);
    assert_eq!(&*plc.read::<u16>("D100", 501).to_result().unwrap(), &words);
    assert_eq!(&requests.lock().unwrap()[1][12..], &[0x82, 2, 88, 0, 0, 1]);
    let (mut plc, requests) = client(vec![Reply::Data(vec![1; 500]), Reply::Data(vec![0])]);
    let bits = plc.read::<bool>("CIO100.15", 501).to_result().unwrap();
    assert!(bits[..500].iter().all(|value| *value));
    assert!(!bits[500]);
    assert_eq!(&requests.lock().unwrap()[1][12..], &[0x30, 0, 132, 3, 0, 1]);
}

#[test]
fn word_and_bit_writes_split_without_repacking_bits() {
    let (mut plc, requests) = client(vec![Reply::Data(vec![]), Reply::Data(vec![])]);
    assert_eq!(
        plc.write_all("D100", &vec![0x1234_u16; 501])
            .to_result()
            .unwrap(),
        501
    );
    assert_eq!(
        &requests.lock().unwrap()[1][12..],
        &[0x82, 2, 88, 0, 0, 1, 0x12, 0x34]
    );
    let (mut plc, requests) = client(vec![Reply::Data(vec![]), Reply::Data(vec![])]);
    assert_eq!(
        plc.write_all("W100.15", &vec![true; 501])
            .to_result()
            .unwrap(),
        501
    );
    assert_eq!(
        &requests.lock().unwrap()[1][12..],
        &[0x31, 0, 132, 3, 0, 1, 1]
    );
}

#[test]
fn odd_byte_and_string_writes_preserve_the_last_neighbour() {
    for order in [
        ByteOrder::ABCD,
        ByteOrder::CDAB,
        ByteOrder::BADC,
        ByteOrder::DCBA,
    ] {
        let swapped = matches!(order, ByteOrder::BADC | ByteOrder::DCBA);
        let (mut plc, requests) = client(vec![
            Reply::Data(if swapped {
                vec![0xad, 0xde]
            } else {
                vec![0xde, 0xad]
            }),
            Reply::Data(vec![]),
        ]);
        plc.set_byte_order(order);
        assert_eq!(
            plc.write::<String>("D100", "ABC").to_result().unwrap(),
            "ABC"
        );
        let recorded = requests.lock().unwrap();
        assert_eq!(&recorded[0][10..], &[1, 1, 0x82, 0, 101, 0, 0, 1]);
        assert_eq!(
            &recorded[1][18..],
            if swapped {
                &[66, 65, 0xad, 67]
            } else {
                &[65, 66, 67, 0xad]
            }
        );
    }
    let (mut plc, requests) = client(vec![Reply::Data(vec![0xaa, 0xbb]), Reply::Data(vec![])]);
    assert_eq!(plc.write("D65535", 0x12_u8).to_result().unwrap(), 0x12);
    assert_eq!(&requests.lock().unwrap()[1][18..], &[0x12, 0xbb]);
}

#[test]
fn strings_are_raw_utf8_with_explicit_byte_length() {
    let text = "你好\0";
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0xff);
    let (mut plc, _) = client(vec![Reply::Data(bytes.clone()), Reply::Data(bytes)]);
    assert_eq!(
        &*plc.read::<String>("D100", text.len()).to_result().unwrap(),
        &[text.to_owned()]
    );
    assert_eq!(
        plc.read_string("D100", text.len()).to_result().unwrap(),
        text
    );
    let (mut plc, _) = client(vec![Reply::Data(vec![0xff, 0xff])]);
    assert!(!plc.read_string("D0", 2).is_success);
    assert!(plc.is_connected());
}

#[test]
fn malformed_addresses_counts_and_routes_never_send_requests() {
    let (mut plc, requests) = client(vec![]);
    for address in [
        "", "D", "D-1", "D+1", "D65536", "D1.0", "D1.16", "M100", "T0", "CNT0", "E19.0", "E100.0",
        "E0", "E0.-1", "E0.1.0", "D1;D2", "D1 2",
    ] {
        assert!(!plc.read::<u16>(address, 1).is_success, "{address}");
        assert!(!plc.write(address, 1_u16).is_success, "{address}");
    }
    for address in ["D1.16", "D1.-1", "D1.0.1", "E0.1.16", "D0."] {
        assert!(!plc.read::<bool>(address, 1).is_success);
    }
    assert!(!plc.read::<u16>("D0", 0).is_success);
    assert!(!plc.read::<u64>("D0", usize::MAX).is_success);
    assert!(!plc.read::<u32>("D65535", 1).is_success);
    assert!(!plc.read::<bool>("D65535.15", 2).is_success);
    assert!(!plc.write_all::<u16>("D0", &[]).is_success);
    assert!(!plc.write_string("D0", "").is_success);
    assert!(!plc.set_route(FinsRoute::default()).is_success);
    assert!(requests.lock().unwrap().is_empty());
    plc.disconnect().to_result().unwrap();
    for route in [
        FinsRoute {
            source_node: 255,
            ..Default::default()
        },
        FinsRoute {
            destination_network: 128,
            ..Default::default()
        },
        FinsRoute {
            destination_node: 255,
            ..Default::default()
        },
        FinsRoute {
            gateway_count: 8,
            ..Default::default()
        },
        FinsRoute {
            destination_network: 1,
            ..Default::default()
        },
    ] {
        assert!(!plc.set_route(route).is_success);
    }
    assert!(!plc.read::<u16>("D0", 1).is_success);
    assert!(!plc.write("D0", 1_u16).is_success);
}

#[test]
fn routed_commands_preserve_all_address_fields() {
    let (mut plc, requests) = client(vec![Reply::Data(vec![0, 42])]);
    plc.disconnect().to_result().unwrap();
    let route = FinsRoute {
        destination_network: 2,
        destination_node: 8,
        destination_unit: 0x10,
        source_network: 1,
        source_node: 15,
        source_unit: 0xfe,
        gateway_count: 3,
    };
    plc.set_route(route).to_result().unwrap();
    plc.connect().to_result().unwrap();
    assert_eq!(plc.route(), route);
    plc.read::<u16>("D0", 1).to_result().unwrap();
    assert_eq!(
        &requests.lock().unwrap()[0][..10],
        &[0x80, 0, 3, 2, 8, 0x10, 1, 15, 0xfe, 1]
    );
}

#[test]
fn invalid_reply_headers_and_payloads_disconnect() {
    for (offset, value) in [
        (0, 0x80),
        (0, 0x40),
        (3, 1),
        (4, 21),
        (5, 1),
        (6, 1),
        (7, 11),
        (8, 1),
        (9, 99),
        (10, 2),
        (11, 2),
    ] {
        let (mut plc, requests) = client(vec![Reply::Invalid(offset, value)]);
        assert!(!plc.read::<u16>("D0", 1).is_success);
        assert!(!plc.is_connected());
        assert!(!plc.read::<u16>("D0", 1).is_success);
        assert_eq!(requests.lock().unwrap().len(), 1);
    }
    for payload in [vec![], vec![0], vec![0, 1, 2]] {
        let (mut plc, _) = client(vec![Reply::Data(payload)]);
        assert!(!plc.read::<u16>("D0", 1).is_success);
        assert!(!plc.is_connected());
    }
    let (mut plc, _) = client(vec![Reply::Data(vec![2])]);
    assert!(!plc.read::<bool>("D0", 1).is_success);
    assert!(!plc.is_connected());
    let (mut plc, _) = client(vec![Reply::Data(vec![0])]);
    assert!(!plc.write("D0", 1_u16).is_success);
    assert!(!plc.is_connected());
}

#[test]
fn controller_errors_keep_the_session_but_failed_writes_never_retry() {
    let (mut plc, requests) = client(vec![Reply::EndCode(0x1103), Reply::Data(vec![0, 1])]);
    let error = plc.read::<u16>("D0", 1);
    assert!(!error.is_success);
    assert!(error.msg.contains("0x1103"));
    assert!(plc.is_connected());
    assert_eq!(&*plc.read::<u16>("D0", 1).to_result().unwrap(), &[1]);
    assert_eq!(requests.lock().unwrap().len(), 2);
    let (mut plc, requests) = client(vec![Reply::Data(vec![]), Reply::Failure]);
    let result = plc.write_all("D0", &vec![1_u16; 1001]);
    assert!(!result.is_success);
    assert!(result.msg.contains("500 earlier words confirmed"));
    assert!(result.msg.contains("not retried"));
    assert!(!plc.is_connected());
    assert_eq!(requests.lock().unwrap().len(), 2);
}

#[test]
fn failed_tail_reads_do_not_issue_a_write() {
    let (mut plc, requests) = client(vec![Reply::EndCode(0x1103)]);
    let error = plc.write_string("D0", "ABC");
    assert!(!error.is_success);
    assert!(error.msg.contains("no bytes written"));
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert_eq!(&requests.lock().unwrap()[0][10..12], &[1, 1]);
}

#[test]
fn custom_value_encoding_is_checked_before_any_array_write() {
    struct BadValue(bool);
    impl OmronValue for BadValue {
        const BYTE_LEN: usize = 2;
        fn from_be_bytes(_: &[u8]) -> Result<Self, String> {
            Ok(Self(false))
        }
        fn to_be_bytes(&self) -> Vec<u8> {
            if self.0 { vec![0] } else { vec![0, 1] }
        }
    }
    let (mut plc, requests) = client(vec![]);
    assert!(
        !plc.write_all("D0", &[BadValue(false), BadValue(true)])
            .is_success
    );
    assert!(!plc.write("D0", BadValue(true)).is_success);
    assert!(requests.lock().unwrap().is_empty());
}

#[test]
fn reserved_response_fields_are_ignored_as_required_by_w342() {
    for (offset, value) in [(0, 0xfe), (1, 0xff), (2, 0xff)] {
        let (mut plc, _) = client(vec![Reply::Invalid(offset, value)]);
        assert_eq!(&*plc.read::<u16>("D0", 1).to_result().unwrap(), &[1]);
    }
}

#[test]
fn wide_values_keep_word_order_across_packet_boundaries() {
    let expected: Vec<u64> = (0..126).map(|value| 0x1234567890ab0000 + value).collect();
    let wire: Vec<u8> = expected
        .iter()
        .flat_map(|value| {
            let bytes = value.to_be_bytes();
            [
                bytes[6], bytes[7], bytes[4], bytes[5], bytes[2], bytes[3], bytes[0], bytes[1],
            ]
        })
        .collect();
    let (mut plc, requests) = client(vec![
        Reply::Data(wire[..1000].to_vec()),
        Reply::Data(wire[1000..].to_vec()),
        Reply::Data(vec![]),
        Reply::Data(vec![]),
    ]);
    assert_eq!(
        &*plc.read::<u64>("D100", 126).to_result().unwrap(),
        &expected
    );
    assert_eq!(plc.write_all("D100", &expected).to_result().unwrap(), 126);
    let recorded = requests.lock().unwrap();
    assert_eq!(&recorded[1][12..], &[0x82, 2, 88, 0, 0, 4]);
    assert_eq!(&recorded[2][18..], &wire[..1000]);
    assert_eq!(&recorded[3][18..], &wire[1000..]);
}

#[test]
fn swapped_byte_streams_are_truncated_after_reordering() {
    for order in [ByteOrder::BADC, ByteOrder::DCBA] {
        let (mut plc, _) = client(vec![
            Reply::Data(vec![66, 65, 68, 67]),
            Reply::Data(vec![66, 65, 68, 67]),
        ]);
        plc.set_byte_order(order);
        assert_eq!(
            &*plc.read::<u8>("D0", 3).to_result().unwrap(),
            &[65, 66, 67]
        );
        assert_eq!(plc.read_string("D0", 3).to_result().unwrap(), "ABC");
    }
}

#[test]
fn sid_wraps_without_overflow() {
    let (mut plc, requests) = client((0..257).map(|_| Reply::Data(vec![0, 1])).collect());
    for _ in 0..257 {
        plc.read::<u16>("D0", 1).to_result().unwrap();
    }
    let recorded = requests.lock().unwrap();
    assert_eq!(recorded[254][9], 255);
    assert_eq!(recorded[255][9], 0);
    assert_eq!(recorded[256][9], 1);
}
