use rs_appliaction::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use rs_appliaction::communication::inovace::{
    ByteOrder, InovanceClient, InovanceType, InovanceValue,
};
use rs_appliaction::communication::modbus::ModbusTransport;
use rs_appliaction::entity::operate::Operator;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

type Requests = Arc<Mutex<Vec<(u8, Vec<u8>)>>>;

enum Reply {
    Data(Vec<u8>),
    Echo,
    Exception(u8),
    Wire(Vec<u8>),
    Failure,
}

struct MockTransport {
    requests: Requests,
    replies: VecDeque<Reply>,
    connected: bool,
}

impl ModbusTransport for MockTransport {
    fn connect(&mut self) -> Result<(), String> {
        self.connected = true;
        Ok(())
    }
    fn disconnect(&mut self) {
        self.connected = false;
    }
    fn is_connected(&self) -> bool {
        self.connected
    }
    fn validate_unit(&self, unit: u8) -> Result<(), String> {
        if unit == 0 {
            Err("No broadcasts".into())
        } else {
            Ok(())
        }
    }
    fn exchange(&mut self, unit: u8, request: &[u8]) -> Result<Vec<u8>, String> {
        self.requests.lock().unwrap().push((unit, request.to_vec()));
        match self
            .replies
            .pop_front()
            .expect("Unexpected Inovance request")
        {
            Reply::Data(bytes) => {
                let mut reply = vec![request[0], bytes.len() as u8];
                reply.extend(bytes);
                Ok(reply)
            }
            Reply::Echo => Ok(request[..if matches!(request[0], 15 | 16 | 63 | 64) {
                5
            } else {
                request.len()
            }]
                .to_vec()),
            Reply::Exception(code) => Ok(vec![request[0] | 0x80, code]),
            Reply::Wire(bytes) => Ok(bytes),
            Reply::Failure => Err("Simulated I/O failure".into()),
        }
    }
}

fn client(series: InovanceType, replies: Vec<Reply>) -> (InovanceClient<MockTransport>, Requests) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut client = InovanceClient::from_transport(
        MockTransport {
            requests: Arc::clone(&requests),
            replies: replies.into(),
            connected: false,
        },
        series,
    );
    ok(client.connect());
    (client, requests)
}

fn ok<Value>(operator: Operator<Value>) -> Value {
    operator.to_result().unwrap()
}
fn binary(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}
fn requests_match(requests: &Requests, expected: &[&str]) {
    let actual = requests.lock().unwrap();
    assert_eq!(actual.len(), expected.len());
    for ((unit, request), expected) in actual.iter().zip(expected) {
        assert_eq!(*unit, 1);
        assert_eq!(*request, binary(expected));
    }
}

#[test]
fn series_bit_maps_use_real_octal_and_distinct_banks() {
    use InovanceType::*;
    for (series, address, expected) in [
        (H3U, "M7679", "01 1D FF 00 01"),
        (H3U, "M8000", "01 1F 40 00 01"),
        (H3U, "SM1023", "01 27 FF 00 01"),
        (H3U, "S4095", "01 EF FF 00 01"),
        (H3U, "T511", "01 F1 FF 00 01"),
        (H3U, "C255", "01 F4 FF 00 01"),
        (H3U, "X10", "01 F8 08 00 01"),
        (H3U, "Y1.0", "01 FC 08 00 01"),
        (H5U, "X1777", "01 FB FF 00 01"),
        (H5U, "B32767", "01 AF FF 00 01"),
        (H5U, "M7999", "01 1F 3F 00 01"),
        (Easy, "Y177.7", "01 FF FF 00 01"),
        (Easy, "B0", "01 30 00 00 01"),
        (AM, "%QX1.7", "01 00 0F 00 01"),
        (AM, "Q15", "01 00 0F 00 01"),
    ] {
        let (mut plc, requests) = client(series, vec![Reply::Data(vec![1])]);
        assert_eq!(
            &*ok(plc.read::<bool>(address, 1)),
            &[true],
            "{series:?} {address}"
        );
        requests_match(&requests, &[expected]);
    }
}

#[test]
fn series_word_maps_and_32_bit_counters_use_correct_stride() {
    use InovanceType::*;
    for (series, address, expected) in [
        (H3U, "D8511", "03 21 3F 00 01"),
        (H3U, "SD1023", "03 27 FF 00 01"),
        (H3U, "T511", "03 F1 FF 00 01"),
        (H3U, "C199", "03 F4 C7 00 01"),
        (H5U, "D7999", "03 1F 3F 00 01"),
        (Easy, "R32767", "03 AF FF 00 01"),
        (AM, "MD100", "03 00 C8 00 01"),
        (AM, "MW65535", "03 FF FF 00 01"),
    ] {
        let (mut plc, requests) = client(series, vec![Reply::Data(binary("12 34"))]);
        assert_eq!(&*ok(plc.read::<u16>(address, 1)), &[0x1234]);
        requests_match(&requests, &[expected]);
    }
    let (mut plc, requests) = client(
        H3U,
        vec![Reply::Data(binary("00 01 00 00 00 02 00 00")), Reply::Echo],
    );
    assert_eq!(&*ok(plc.read::<u32>("C200", 2)), &[1, 2]);
    assert_eq!(ok(plc.write::<i32>("C255", -2)), -2);
    requests_match(
        &requests,
        &["03 F7 00 00 04", "10 F7 6E 00 02 04 FF FE FF FF"],
    );
}

#[test]
fn am_system_functions_do_not_leak_into_standard_requests() {
    let (mut plc, requests) = client(
        InovanceType::AM,
        vec![
            Reply::Data(vec![1]),
            Reply::Data(binary("12 34")),
            Reply::Echo,
            Reply::Echo,
            Reply::Echo,
            Reply::Echo,
            Reply::Data(binary("00 01")),
        ],
    );
    ok(plc.read::<bool>("SM10", 1));
    ok(plc.read::<u16>("SDW3", 1));
    ok(plc.write::<bool>("SM2", true));
    ok(plc.write_all("SM2", &[false, true]));
    ok(plc.write::<u16>("SD2", 0x1234));
    ok(plc.write_all("SD2", &[1_u16, 2]));
    ok(plc.read::<u16>("MW2", 1));
    requests_match(
        &requests,
        &[
            "31 00 0A 00 01",
            "33 00 03 00 01",
            "35 00 02 FF 00",
            "3F 00 02 00 02 01 02",
            "36 00 02 12 34",
            "40 00 02 00 02 04 00 01 00 02",
            "03 00 02 00 01",
        ],
    );
}

#[test]
fn station_overrides_are_local_and_default_settings_are_preserved() {
    let (mut plc, requests) = client(
        InovanceType::H5U,
        vec![
            Reply::Data(vec![0, 1]),
            Reply::Data(vec![0, 2]),
            Reply::Echo,
        ],
    );
    assert_eq!(plc.byte_order(), ByteOrder::CDAB);
    ok(plc.set_unit_id(7));
    ok(plc.read::<u16>(" s = 255 ; d100 ", 1));
    ok(plc.read::<u16>("x=2;D100", 1));
    ok(plc.write::<u16>("D100", 3));
    assert_eq!(plc.unit_id(), 7);
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|(unit, _)| *unit)
            .collect::<Vec<_>>(),
        [255, 2, 7]
    );
    assert!(!plc.set_unit_id(0).is_success);
    assert_eq!(plc.unit_id(), 7);
    assert!(!plc.set_series(InovanceType::H3U).is_success);
    assert_eq!(plc.series(), InovanceType::H5U);
    ok(plc.disconnect());
    ok(plc.disconnect());
    ok(plc.set_series(InovanceType::H3U));
    assert!(!plc.is_connected());
    assert!(!plc.read::<u16>("D100", 1).is_success);
    assert_eq!(requests.lock().unwrap().len(), 3);
}

#[test]
fn every_numeric_width_and_byte_order_round_trips() {
    macro_rules! scalar {
        ($ty:ty, $value:expr, $wire:literal) => {{
            let wire = binary($wire);
            let (mut plc, requests) = client(
                InovanceType::H5U,
                vec![Reply::Data(wire.clone()), Reply::Echo],
            );
            assert_eq!(&*ok(plc.read::<$ty>("D100", 1)), &[$value]);
            assert_eq!(ok(plc.write::<$ty>("D100", $value)), $value);
            let requests = requests.lock().unwrap();
            let request = &requests[1].1;
            assert_eq!(&request[if wire.len() == 2 { 3 } else { 6 }..], wire);
        }};
    }
    scalar!(u16, 0x1234, "12 34");
    scalar!(i16, -2, "FF FE");
    scalar!(u32, 0x12345678, "56 78 12 34");
    scalar!(i32, -2, "FF FE FF FF");
    scalar!(u64, 0x0102030405060708, "07 08 05 06 03 04 01 02");
    scalar!(i64, -2, "FF FE FF FF FF FF FF FF");
    scalar!(f32, 1.0, "00 00 3F 80");
    scalar!(f64, 1.0, "00 00 00 00 00 00 3F F0");
    for (order, wire) in [
        (ByteOrder::ABCD, "12 34 56 78"),
        (ByteOrder::BADC, "34 12 78 56"),
        (ByteOrder::CDAB, "56 78 12 34"),
        (ByteOrder::DCBA, "78 56 34 12"),
    ] {
        let (mut plc, requests) = client(
            InovanceType::H3U,
            vec![Reply::Data(binary(wire)), Reply::Echo],
        );
        plc.set_byte_order(order);
        assert_eq!(&*ok(plc.read::<u32>("D0", 1)), &[0x12345678]);
        ok(plc.write::<u32>("D0", 0x12345678));
        assert_eq!(&requests.lock().unwrap()[1].1[6..], binary(wire));
    }
}

#[test]
fn bytes_and_strings_preserve_neighbors_and_utf8() {
    let (mut plc, requests) = client(
        InovanceType::H5U,
        vec![
            Reply::Data(binary("E4 BD A0 00")),
            Reply::Data(binary("FF 80")),
            Reply::Data(binary("58 5A")),
            Reply::Echo,
            Reply::Data(binary("12 34")),
            Reply::Echo,
        ],
    );
    assert_eq!(ok(plc.read_string("D100", 3)), "你");
    assert_eq!(&*ok(plc.read::<i8>("D100", 2)), &[-1, -128]);
    assert_eq!(ok(plc.write::<String>("D100", "ABC")), "ABC");
    assert_eq!(ok(plc.write::<u8>("D100", 0xab)), 0xab);
    requests_match(
        &requests,
        &[
            "03 00 64 00 02",
            "03 00 64 00 01",
            "03 00 65 00 01",
            "10 00 64 00 02 04 41 42 43 5A",
            "03 00 64 00 01",
            "06 00 64 AB 34",
        ],
    );
    let (mut plc, requests) = client(
        InovanceType::Easy,
        vec![
            Reply::Data(binary("42 41 44 43")),
            Reply::Data(binary("58 59")),
            Reply::Echo,
        ],
    );
    plc.set_byte_order(ByteOrder::BADC);
    assert_eq!(&*ok(plc.read::<u8>("D0", 3)), b"ABC");
    ok(plc.write_all("D0", b"ABC"));
    requests_match(
        &requests,
        &[
            "03 00 00 00 02",
            "03 00 01 00 01",
            "10 00 00 00 02 04 42 41 58 43",
        ],
    );
}

#[test]
fn am_byte_and_bit_offsets_match_plc_memory_without_overwriting_neighbors() {
    let (mut plc, requests) = client(
        InovanceType::AM,
        vec![
            Reply::Data(binary("11 22 33 44")),
            Reply::Data(binary("11 22")),
            Reply::Data(binary("33 44")),
            Reply::Echo,
            Reply::Data(binary("80 00")),
            Reply::Data(binary("12 34 AB CD")),
            Reply::Echo,
        ],
    );
    assert_eq!(&*ok(plc.read::<u8>("MB1", 2)), &[0x11, 0x44]);
    ok(plc.write_all("MB1", &[0xaa_u8, 0xbb]));
    assert_eq!(&*ok(plc.read::<bool>("MX1.7", 1)), &[true]);
    ok(plc.write_all("MW0.15", &[true, false]));
    requests_match(
        &requests,
        &[
            "03 00 00 00 02",
            "03 00 00 00 01",
            "03 00 01 00 01",
            "10 00 00 00 02 04 AA 22 33 BB",
            "03 00 00 00 01",
            "03 00 00 00 02",
            "10 00 00 00 02 04 92 34 AB CC",
        ],
    );
}

#[test]
fn generic_device_traits_and_borrowed_strings_match_existing_api() {
    fn exercise<Client: DeviceBase + ReadBase<u16> + WriteBase<u16>>(plc: &mut Client) {
        ok(plc.connect());
        assert_eq!(&*ok(plc.read("D0", 1)), &[1]);
        assert_eq!(ok(plc.write("D1", 2)), 2);
        assert_eq!(ok(plc.write_all("D2", &[3, 4])), 2);
        ok(plc.disconnect());
    }
    let (mut plc, _) = client(
        InovanceType::H5U,
        vec![Reply::Data(vec![0, 1]), Reply::Echo, Reply::Echo],
    );
    exercise(&mut plc);
    let (mut plc, _) = client(
        InovanceType::H5U,
        vec![Reply::Echo, Reply::Echo, Reply::Data(binary("41 00"))],
    );
    ok(plc.write::<String>("D0", &String::from("AB")));
    ok(plc.write_string("D0", "CD"));
    assert_eq!(&*ok(plc.read::<String>("D0", 2)), &[String::from("A\0")]);
}

#[test]
fn invalid_addresses_types_ranges_and_readonly_writes_never_exchange() {
    use InovanceType::*;
    for (series, address, bit) in [
        (H3U, "M7680", true),
        (H3U, "M7999", true),
        (H3U, "M8512", true),
        (H3U, "X400", true),
        (H5U, "X2000", true),
        (H5U, "X8", true),
        (H5U, "Y1.8", true),
        (H5U, "SM0", true),
        (Easy, "T0", true),
        (H5U, "C0", false),
        (H5U, "SD0", false),
        (H3U, "B0", true),
        (H3U, "C256", false),
        (H3U, "D8512", false),
        (H5U, "D8000", false),
        (H5U, "R32768", false),
        (AM, "IX0.8", true),
        (AM, "MX0.8", true),
        (AM, "MW0.16", true),
        (AM, "MW65536", false),
        (AM, "MD32768", false),
        (AM, "SM65536", true),
        (AM, "SD65536", false),
        (H5U, "D-1", false),
        (H5U, "D1.0", false),
        (H5U, "D1", true),
        (H5U, "100", false),
        (H5U, "x=0;D1", false),
        (H5U, "s=256;D1", false),
        (H5U, "s=1;x=2;D1", false),
        (H5U, "x=1;", false),
        (H5U, "D184467440737095516160", false),
        (H5U, "Ｄ1", false),
    ] {
        let (mut plc, requests) = client(series, vec![]);
        let valid = if bit {
            plc.read::<bool>(address, 1).is_success
        } else {
            plc.read::<u16>(address, 1).is_success
        };
        assert!(!valid, "{series:?}: {address}");
        assert!(requests.lock().unwrap().is_empty());
    }
    let (mut plc, requests) = client(H3U, vec![]);
    assert!(!plc.read::<bool>("M7679", 2).is_success);
    assert!(!plc.read::<u32>("C199", 1).is_success);
    assert!(!plc.read::<u16>("C200", 1).is_success);
    assert!(!plc.read::<u64>("C200", 1).is_success);
    assert!(!plc.read::<u32>("C255", 2).is_success);
    assert!(!plc.write_all("X0", &[true]).is_success);
    assert!(!plc.write_all("D8511", &[1_u16, 2]).is_success);
    assert!(!plc.read::<u64>("D0", usize::MAX).is_success);
    assert!(!plc.read::<u8>("D0", 0).is_success);
    assert!(!plc.write_all::<u16>("D0", &[]).is_success);
    assert!(!plc.write_string("D0", "").is_success);
    assert!(requests.lock().unwrap().is_empty());
}

#[test]
fn packet_splitting_keeps_scalar_boundaries_and_reports_partial_writes() {
    let (mut plc, requests) = client(
        InovanceType::H5U,
        vec![
            Reply::Data(vec![0; 240]),
            Reply::Data(vec![0; 8]),
            Reply::Echo,
            Reply::Failure,
        ],
    );
    assert_eq!(ok(plc.read::<u64>("D0", 31)).len(), 31);
    let outcome = plc.write_all("D0", &[0_u64; 31]);
    assert!(!outcome.is_success);
    assert!(outcome.msg.contains("120 earlier registers confirmed"));
    assert!(!plc.is_connected());
    let actual = requests.lock().unwrap();
    assert_eq!(actual.len(), 4);
    assert_eq!(actual[0].1, binary("03 00 00 00 78"));
    assert_eq!(actual[1].1, binary("03 00 78 00 04"));
    assert_eq!(&actual[2].1[..6], &binary("10 00 00 00 78 F0"));
    assert_eq!(&actual[3].1[..6], &binary("10 00 78 00 04 08"));
}

#[test]
fn long_coil_requests_split_without_crossing_soft_device_addresses() {
    let (mut plc, requests) = client(
        InovanceType::H5U,
        vec![
            Reply::Data(vec![0; 30]),
            Reply::Data(vec![1]),
            Reply::Echo,
            Reply::Echo,
        ],
    );
    let values = ok(plc.read::<bool>("M0", 241));
    assert_eq!(values.len(), 241);
    assert!(values[240]);
    ok(plc.write_all("M0", &values));
    let actual = requests.lock().unwrap();
    assert_eq!(actual.len(), 4);
    assert_eq!(actual[0].1, binary("01 00 00 00 F0"));
    assert_eq!(actual[1].1, binary("01 00 F0 00 01"));
    assert_eq!(&actual[2].1[..6], &binary("0F 00 00 00 F0 1E"));
    assert_eq!(actual[3].1, binary("0F 00 F0 00 01 01 01"));
}

#[test]
fn exceptions_remain_connected_but_malformed_replies_and_io_failures_disconnect() {
    let (mut plc, requests) = client(
        InovanceType::AM,
        vec![
            Reply::Exception(2),
            Reply::Data(binary("00 01")),
            Reply::Wire(binary("03 02 00 01")),
        ],
    );
    assert!(!plc.read::<u16>("SD0", 1).is_success);
    assert!(plc.is_connected());
    ok(plc.read::<u16>("MW0", 1));
    assert!(!plc.read::<u16>("SD0", 1).is_success);
    assert!(!plc.is_connected());
    assert_eq!(requests.lock().unwrap().len(), 3);
    for reply in [Reply::Wire(binary("03 01 01")), Reply::Failure] {
        let (mut plc, requests) = client(InovanceType::H5U, vec![reply]);
        assert!(!plc.write_string("D0", "ABC").is_success);
        assert!(!plc.is_connected());
        requests_match(&requests, &["03 00 01 00 01"]);
    }
    let (mut plc, _) = client(
        InovanceType::H5U,
        vec![Reply::Wire(binary("06 00 01 00 01"))],
    );
    assert!(!plc.write::<u16>("D0", 1).is_success);
    assert!(!plc.is_connected());
}

#[test]
fn invalid_utf8_is_reported_without_dropping_a_valid_connection() {
    let (mut plc, _) = client(InovanceType::H5U, vec![Reply::Data(binary("FF 00"))]);
    let result = plc.read_string("D0", 1);
    assert!(!result.is_success);
    assert!(result.msg.contains("UTF-8"));
    assert!(plc.is_connected());
}

struct BadValue;
impl InovanceValue for BadValue {
    const BYTE_LEN: usize = 2;
    fn from_be_bytes(_: &[u8]) -> Result<Self, String> {
        Ok(Self)
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        vec![1]
    }
}

#[test]
fn invalid_custom_value_encoding_fails_before_any_packet() {
    let (mut plc, requests) = client(InovanceType::H5U, vec![]);
    assert!(!plc.write("D0", BadValue).is_success);
    assert!(!plc.write_all("D0", &[BadValue]).is_success);
    assert!(requests.lock().unwrap().is_empty());
}

#[test]
fn am_ac_ap_share_the_same_protocol_and_address_mapping() {
    for series in [InovanceType::AM, InovanceType::AC, InovanceType::AP] {
        let (mut plc, requests) = client(
            series,
            vec![
                Reply::Data(vec![1]),
                Reply::Data(vec![1]),
                Reply::Data(binary("12 34")),
                Reply::Echo,
            ],
        );
        ok(plc.read::<bool>("QX1.7", 1));
        ok(plc.read::<bool>("IX1.7", 1));
        ok(plc.read::<u16>("MW100", 1));
        ok(plc.write::<u16>("SD1", 2));
        requests_match(
            &requests,
            &[
                "01 00 0F 00 01",
                "02 00 0F 00 01",
                "03 00 64 00 01",
                "36 00 01 00 02",
            ],
        );
        assert_eq!(plc.series(), series);
    }
}

#[test]
fn iec_byte_aliases_allow_aligned_numbers_but_never_misalign_them() {
    for series in [
        InovanceType::AM,
        InovanceType::AC,
        InovanceType::AP,
        InovanceType::EVO,
    ] {
        let (mut plc, requests) = client(
            series,
            vec![Reply::Data(binary("56 78 12 34")), Reply::Echo],
        );
        assert_eq!(&*ok(plc.read::<u32>("MB2", 1)), &[0x12345678]);
        ok(plc.write::<u32>("MB2", 0x12345678));
        assert!(!plc.read::<u16>("MB1", 1).is_success);
        assert!(!plc.write::<u32>("MB1", 1).is_success);
        requests_match(
            &requests,
            &["03 00 01 00 02", "10 00 01 00 02 04 56 78 12 34"],
        );
    }
}

#[test]
fn evo_has_independent_input_and_byte_addressing_without_am_system_functions() {
    let (mut plc, requests) = client(
        InovanceType::EVO,
        vec![
            Reply::Data(vec![1]),
            Reply::Echo,
            Reply::Data(binary("12 34")),
            Reply::Data(binary("12 34")),
            Reply::Echo,
        ],
    );
    assert_eq!(&*ok(plc.read::<bool>("IX1.7", 1)), &[true]);
    ok(plc.write::<bool>("QX1.7", true));
    assert_eq!(&*ok(plc.read::<u8>("MB1", 1)), &[0x12]);
    ok(plc.write::<u8>("MB1", 0xab));
    assert!(!plc.write::<bool>("I15", true).is_success);
    assert!(!plc.read::<bool>("SM0", 1).is_success);
    assert!(!plc.read::<u16>("SD0", 1).is_success);
    requests_match(
        &requests,
        &[
            "02 00 0F 00 01",
            "05 00 0F FF 00",
            "03 00 00 00 01",
            "03 00 00 00 01",
            "06 00 00 AB 34",
        ],
    );
}
