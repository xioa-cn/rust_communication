use rs_appliaction::communication::modbus::{
    ByteOrder, ModbusClient, ModbusTransport, ModbusValue,
};
use std::collections::VecDeque;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

type Requests = Arc<Mutex<Vec<Vec<u8>>>>;

struct TestTransport {
    replies: VecDeque<Result<Vec<u8>, String>>,
    requests: Requests,
    connected: bool,
}

impl ModbusTransport for TestTransport {
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
    fn exchange(&mut self, unit: u8, request: &[u8]) -> Result<Vec<u8>, String> {
        assert_eq!(unit, 1);
        self.requests.lock().unwrap().push(request.to_vec());
        self.replies
            .pop_front()
            .unwrap_or_else(|| Err("Unexpected request".into()))
    }
}

fn client(replies: Vec<Vec<u8>>) -> (ModbusClient<TestTransport>, Requests) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let transport = TestTransport {
        replies: replies.into_iter().map(Ok).collect(),
        requests: Arc::clone(&requests),
        connected: false,
    };
    let mut client = ModbusClient::from_transport(transport, 1);
    client.connect().to_result().unwrap();
    (client, requests)
}

fn registers(values: &[u16]) -> Vec<u8> {
    let mut reply = vec![3, (values.len() * 2) as u8];
    for value in values {
        reply.extend_from_slice(&value.to_be_bytes());
    }
    reply
}

#[test]
fn register_reads_split_at_125_and_advance_addresses() {
    let expected: Vec<u16> = (0..126).collect();
    let (mut client, requests) = client(vec![
        registers(&expected[..125]),
        registers(&expected[125..]),
    ]);
    assert_eq!(
        &*client.read::<u16>("HR100", 126).to_result().unwrap(),
        expected
    );
    assert_eq!(
        *requests.lock().unwrap(),
        vec![vec![3, 0, 100, 0, 125], vec![3, 0, 225, 0, 1]]
    );
}

#[test]
fn bit_reads_split_at_2000_and_unpack_low_bits_first() {
    let mut first_reply = vec![1, 250];
    first_reply.extend_from_slice(&[0x55; 250]);
    let (mut client, requests) = client(vec![first_reply, vec![1, 1, 1]]);
    let values = client.read::<bool>("0", 2001).to_result().unwrap();
    assert_eq!(values.len(), 2001);
    for (index, value) in values.iter().enumerate() {
        assert_eq!(*value, index % 2 == 0);
    }
    assert_eq!(
        *requests.lock().unwrap(),
        vec![vec![1, 0, 0, 7, 208], vec![1, 7, 208, 0, 1]]
    );
}

#[test]
fn register_writes_split_at_123_and_report_element_count() {
    let (mut client, requests) = client(vec![vec![16, 0, 100, 0, 123], vec![16, 0, 223, 0, 1]]);
    let values: Vec<u16> = (0..124).collect();
    assert_eq!(client.write_all("100", &values).to_result().unwrap(), 124);
    let requests = requests.lock().unwrap();
    assert_eq!(&requests[0][..6], &[16, 0, 100, 0, 123, 246]);
    assert_eq!(&requests[0][6..], &registers(&values[..123])[2..]);
    assert_eq!(&requests[1], &[16, 0, 223, 0, 1, 2, 0, 123]);
}

#[test]
fn coil_writes_split_at_1968_and_clear_unused_bits() {
    let (mut client, requests) = client(vec![vec![15, 0, 0, 7, 176], vec![15, 7, 176, 0, 1]]);
    let values = vec![true; 1969];
    assert_eq!(client.write_all("C0", &values).to_result().unwrap(), 1969);
    let requests = requests.lock().unwrap();
    assert_eq!(&requests[0][..6], &[15, 0, 0, 7, 176, 246]);
    assert_eq!(&requests[0][6..], &[255; 246]);
    assert_eq!(requests[1], [15, 7, 176, 0, 1, 1, 1]);
}

#[test]
fn multiregister_values_are_not_split_across_requests() {
    let values: Vec<u64> = (0..32).collect();
    let bytes: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect();
    let mut first_reply = vec![3, 248];
    first_reply.extend_from_slice(&bytes[..248]);
    let mut second_reply = vec![3, 8];
    second_reply.extend_from_slice(&bytes[248..]);
    let (mut client, requests) = client(vec![
        first_reply,
        second_reply,
        vec![16, 0, 0, 0, 120],
        vec![16, 0, 120, 0, 4],
    ]);
    assert_eq!(&*client.read::<u64>("0", 32).to_result().unwrap(), &values);
    assert_eq!(
        client.write_all("0", &values[..31]).to_result().unwrap(),
        31
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests[0], [3, 0, 0, 0, 124]);
    assert_eq!(requests[1], [3, 0, 124, 0, 4]);
    assert_eq!(&requests[2][..6], &[16, 0, 0, 0, 120, 240]);
    assert_eq!(&requests[3][..6], &[16, 0, 120, 0, 4, 8]);
    assert_eq!(&requests[3][6..], &30_u64.to_be_bytes());
}

#[test]
fn invalid_addresses_lengths_and_read_only_writes_do_not_send() {
    let (mut client, requests) = client(Vec::new());
    for address in [
        "", "-1", "+1", "65536", "HR65536", "D100", "HR1.0", "s=1;0", "C0", "DI0", "HR 0",
    ] {
        assert!(!client.read::<u16>(address, 1).is_success, "{address}");
    }
    assert!(!client.read::<u16>("HR0", 0).is_success);
    assert!(!client.read::<u16>("HR0", usize::MAX).is_success);
    assert!(!client.read::<u32>("HR65535", 1).is_success);
    assert!(!client.read::<bool>("C65535", 2).is_success);
    assert!(!client.read::<bool>("HR0", 1).is_success);
    assert!(!client.write("IR0", 1_u16).is_success);
    assert!(!client.write("DI0", true).is_success);
    assert!(!client.write("65535", 1_u64).is_success);
    assert!(!client.write_all::<u16>("0", &[]).is_success);
    assert!(!client.write_all("65535", &[1_u16, 2]).is_success);
    assert!(requests.lock().unwrap().is_empty());
    assert!(client.is_connected());
}

#[test]
fn highest_address_and_decimal_addresses_are_not_normalized() {
    let (mut client, requests) = client(vec![registers(&[7]), registers(&[8]), registers(&[9])]);
    assert_eq!(&*client.read::<u16>("65535", 1).to_result().unwrap(), &[7]);
    assert_eq!(&*client.read::<u16>("40001", 1).to_result().unwrap(), &[8]);
    assert_eq!(&*client.read::<u16>(" hr0 ", 1).to_result().unwrap(), &[9]);
    assert_eq!(
        *requests.lock().unwrap(),
        vec![
            vec![3, 255, 255, 0, 1],
            vec![3, 0x9c, 0x41, 0, 1],
            vec![3, 0, 0, 0, 1]
        ]
    );
}

#[test]
fn all_byte_orders_encode_and_decode_per_value() {
    for (order, expected) in [
        (ByteOrder::ABCD, [0x12, 0x34, 0x56, 0x78]),
        (ByteOrder::BADC, [0x34, 0x12, 0x78, 0x56]),
        (ByteOrder::CDAB, [0x56, 0x78, 0x12, 0x34]),
        (ByteOrder::DCBA, [0x78, 0x56, 0x34, 0x12]),
    ] {
        let mut read_reply = vec![3, 8];
        read_reply.extend_from_slice(&expected);
        read_reply.extend_from_slice(&expected);
        let (mut client, requests) = client(vec![read_reply, vec![16, 0, 0, 0, 4]]);
        client.set_byte_order(order);
        assert_eq!(client.byte_order(), order);
        assert_eq!(
            &*client.read::<u32>("0", 2).to_result().unwrap(),
            &[0x12345678, 0x12345678]
        );
        assert_eq!(
            client
                .write_all("0", &[0x12345678_u32, 0x12345678])
                .to_result()
                .unwrap(),
            2
        );
        let requests = requests.lock().unwrap();
        assert_eq!(&requests[1][6..10], &expected);
        assert_eq!(&requests[1][10..14], &expected);
    }
}

#[test]
fn word_reversal_for_64_bit_values_reverses_all_four_words() {
    let (mut client, requests) = client(vec![
        vec![3, 8, 7, 8, 5, 6, 3, 4, 1, 2],
        vec![16, 0, 0, 0, 4],
    ]);
    client.set_byte_order(ByteOrder::CDAB);
    let value = 0x0102030405060708_u64;
    assert_eq!(&*client.read::<u64>("0", 1).to_result().unwrap(), &[value]);
    assert_eq!(client.write("0", value).to_result().unwrap(), value);
    assert_eq!(&requests.lock().unwrap()[1][6..], &[7, 8, 5, 6, 3, 4, 1, 2]);
}

#[test]
fn partial_write_error_reports_confirmed_units_and_stops() {
    let (mut client, requests) = client(vec![vec![16, 0, 0, 0, 123]]);
    let result = client.write_all("0", &[7_u16; 247]);
    assert!(!result.is_success);
    assert!(result.msg.contains("123 earlier units confirmed"));
    assert!(result.msg.contains("not retried"));
    assert_eq!(requests.lock().unwrap().len(), 2);
    assert!(!client.is_connected());
}

#[test]
fn unused_response_bits_and_multiple_write_echoes_are_checked() {
    let (mut read_client, _) = client(vec![vec![1, 1, 0xff]]);
    assert!(!read_client.read::<bool>("0", 1).is_success);
    assert!(!read_client.is_connected());
    let (mut write_client, _) = client(vec![vec![16, 0, 0, 0, 1]]);
    assert!(!write_client.write_all("0", &[1_u16, 2]).is_success);
    assert!(!write_client.is_connected());
}

fn round_trip<Value: ModbusValue + Debug + PartialEq>(value: Value) {
    let bytes = value.to_be_bytes();
    assert_eq!(bytes.len(), Value::BYTE_LEN);
    assert_eq!(Value::from_be_bytes(&bytes).unwrap(), value);
    assert!(Value::from_be_bytes(&[]).is_err());
}

#[test]
fn every_supported_type_round_trips_without_custom_macros() {
    round_trip(false);
    round_trip(true);
    round_trip(u16::MAX);
    round_trip(i16::MIN);
    round_trip(u32::MAX);
    round_trip(i32::MIN);
    round_trip(u64::MAX);
    round_trip(i64::MIN);
    round_trip(1.25_f32);
    round_trip(-123.5_f64);
    assert!(<bool as ModbusValue>::from_be_bytes(&[2]).is_err());
    assert_eq!(
        <f32 as ModbusValue>::to_be_bytes(&1.0),
        vec![0x3f, 0x80, 0, 0]
    );
    assert_eq!(<i16 as ModbusValue>::to_be_bytes(&-2), vec![0xff, 0xfe]);
}

struct InvalidValue;

impl ModbusValue for InvalidValue {
    const BYTE_LEN: usize = 0;
    fn from_be_bytes(_: &[u8]) -> Result<Self, String> {
        Ok(Self)
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Vec::new()
    }
}

struct InvalidEncoding;

impl ModbusValue for InvalidEncoding {
    const BYTE_LEN: usize = 2;
    fn from_be_bytes(_: &[u8]) -> Result<Self, String> {
        Ok(Self)
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        Vec::new()
    }
}

#[test]
fn custom_value_metadata_and_encoding_are_validated_before_io() {
    let (mut client, requests) = client(Vec::new());
    assert!(!client.read::<InvalidValue>("0", 1).is_success);
    assert!(!client.write("0", InvalidValue).is_success);
    assert!(!client.write("0", InvalidEncoding).is_success);
    assert!(requests.lock().unwrap().is_empty());
}
