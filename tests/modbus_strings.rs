use rs_appliaction::communication::device_base::{ReadBase, WriteBase};
use rs_appliaction::communication::modbus::{ByteOrder, ModbusClient, ModbusTransport};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

type Requests = Arc<Mutex<Vec<(u8, Vec<u8>)>>>;

struct MemoryTransport {
    replies: VecDeque<Vec<u8>>,
    requests: Requests,
    connected: bool,
}

impl ModbusTransport for MemoryTransport {
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
        self.requests.lock().unwrap().push((unit, request.to_vec()));
        self.replies
            .pop_front()
            .ok_or_else(|| "No scripted response".into())
    }
}

fn client(replies: Vec<Vec<u8>>) -> (ModbusClient<MemoryTransport>, Requests) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let transport = MemoryTransport {
        replies: replies.into(),
        requests: Arc::clone(&requests),
        connected: false,
    };
    let mut client = ModbusClient::from_transport(transport, 1);
    client.connect().to_result().unwrap();
    (client, requests)
}

#[test]
fn strings_support_generic_reads_utf8_and_per_call_station() {
    let (mut client, requests) = client(vec![
        vec![3, 6, b'H', b'e', b'l', b'l', b'o', b'!'],
        vec![3, 4, 0xe4, 0xbd, 0xa0, b'Z'],
    ]);
    let text = ReadBase::<String>::read(&mut client, "x=2;100", 5)
        .to_result()
        .unwrap();
    assert_eq!(&*text, &["Hello"]);
    assert_eq!(client.read_string("100", 3).to_result().unwrap(), "你");
    assert_eq!(client.unit_id(), 1);
    assert_eq!(
        *requests.lock().unwrap(),
        vec![(2, vec![3, 0, 100, 0, 3]), (1, vec![3, 0, 100, 0, 2])]
    );
}

#[test]
fn input_register_strings_preserve_embedded_and_trailing_nuls() {
    let (mut client, requests) = client(vec![vec![4, 6, b'A', 0, b'B', 0, 0, b'Z']]);
    assert_eq!(
        client
            .read_string(" X = 2 ; ir100 ", 5)
            .to_result()
            .unwrap(),
        "A\0B\0\0"
    );
    assert_eq!(requests.lock().unwrap()[0], (2, vec![4, 0, 100, 0, 3]));
}

#[test]
fn odd_string_write_preserves_adjacent_byte_on_the_overridden_station() {
    let (mut client, requests) = client(vec![
        vec![3, 2, b'X', b'Z'],
        vec![16, 0, 100, 0, 2],
        vec![3, 2, 0, 7],
    ]);
    assert_eq!(
        client.write_string("x=2;100", "ABC").to_result().unwrap(),
        "ABC"
    );
    assert_eq!(&*client.read::<u16>("100", 1).to_result().unwrap(), &[7]);
    assert_eq!(client.unit_id(), 1);
    assert_eq!(
        *requests.lock().unwrap(),
        vec![
            (2, vec![3, 0, 101, 0, 1]),
            (2, vec![16, 0, 100, 0, 2, 4, b'A', b'B', b'C', b'Z']),
            (1, vec![3, 0, 100, 0, 1])
        ]
    );
}

#[test]
fn even_utf8_string_writes_need_no_tail_read() {
    let (mut client, requests) = client(vec![vec![16, 0, 100, 0, 3]]);
    let value = "你好".to_owned();
    assert_eq!(
        WriteBase::<String>::write(&mut client, "x=2;HR100", value.clone())
            .to_result()
            .unwrap(),
        value
    );
    assert_eq!(
        *requests.lock().unwrap(),
        vec![(
            2,
            vec![16, 0, 100, 0, 3, 6, 0xe4, 0xbd, 0xa0, 0xe5, 0xa5, 0xbd]
        )]
    );
}

#[test]
fn string_byte_order_swaps_within_words_without_reversing_text() {
    for order in [
        ByteOrder::ABCD,
        ByteOrder::BADC,
        ByteOrder::CDAB,
        ByteOrder::DCBA,
    ] {
        let swapped = matches!(order, ByteOrder::BADC | ByteOrder::DCBA);
        let bytes = if swapped {
            vec![b'B', b'A', b'!', b'C']
        } else {
            b"ABC!".to_vec()
        };
        let mut read_reply = vec![3, 4];
        read_reply.extend_from_slice(&bytes);
        let tail_reply = if swapped {
            vec![3, 2, b'Z', b'X']
        } else {
            vec![3, 2, b'X', b'Z']
        };
        let (mut client, requests) = client(vec![read_reply, tail_reply, vec![16, 0, 0, 0, 2]]);
        client.set_byte_order(order);
        assert_eq!(client.read_string("0", 3).to_result().unwrap(), "ABC");
        client.write_string("0", "ABC").to_result().unwrap();
        let expected = if swapped { b"BAZC" } else { b"ABCZ" };
        assert_eq!(&requests.lock().unwrap()[2].1[6..], expected);
    }
}

#[test]
fn utf8_character_may_cross_read_packet_boundary() {
    let text = "A".repeat(249) + "你";
    let mut first = vec![3, 250];
    first.extend_from_slice(&text.as_bytes()[..250]);
    let mut last = vec![3, 2];
    last.extend_from_slice(&text.as_bytes()[250..]);
    let (mut client, requests) = client(vec![first, last]);
    assert_eq!(
        client
            .read_string("x=2;100", text.len())
            .to_result()
            .unwrap(),
        text
    );
    assert_eq!(
        *requests.lock().unwrap(),
        vec![(2, vec![3, 0, 100, 0, 125]), (2, vec![3, 0, 225, 0, 1])]
    );
}

#[test]
fn string_write_packets_keep_station_and_preserve_tail() {
    let text = "A".repeat(247);
    let (mut client, requests) = client(vec![
        vec![3, 2, b'Y', b'Z'],
        vec![16, 0, 0, 0, 123],
        vec![16, 0, 123, 0, 1],
    ]);
    client.write_string("x=2;0", &text).to_result().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests.iter().all(|(unit, _)| *unit == 2));
    assert_eq!(requests[0].1, [3, 0, 123, 0, 1]);
    assert_eq!(&requests[1].1[..6], &[16, 0, 0, 0, 123, 246]);
    assert_eq!(requests[2].1, [16, 0, 123, 0, 1, 2, b'A', b'Z']);
    assert_eq!(client.unit_id(), 1);
}

#[test]
fn invalid_utf8_is_reported_without_poisoning_connection() {
    let (mut client, requests) = client(vec![vec![3, 2, 0xe4, 0xbd], vec![3, 2, b'O', b'K']]);
    assert!(client.read_string("x=2;0", 2).msg.contains("UTF-8"));
    assert!(client.is_connected());
    assert_eq!(client.read_string("0", 2).to_result().unwrap(), "OK");
    assert_eq!(requests.lock().unwrap()[1].0, 1);
}

#[test]
fn failed_tail_read_never_sends_a_write() {
    let (mut client, requests) = client(vec![vec![0x83, 2]]);
    let result = client.write_string("x=2;0", "ABC");
    assert!(!result.is_success);
    assert!(result.msg.contains("no string bytes written"));
    assert_eq!(*requests.lock().unwrap(), vec![(2, vec![3, 0, 1, 0, 1])]);
    assert!(client.is_connected());
    assert_eq!(client.unit_id(), 1);
}

#[test]
fn empty_overflow_read_only_and_string_arrays_fail_before_io() {
    let (mut client, requests) = client(Vec::new());
    assert!(!client.read_string("0", 0).is_success);
    assert!(!client.read::<String>("0", usize::MAX).is_success);
    assert!(!client.read_string("65535", 3).is_success);
    assert!(!client.read_string("C0", 2).is_success);
    assert!(!client.write_string("0", "").is_success);
    assert!(!client.write_string("65535", "你").is_success);
    assert!(!client.write_string("x=2;IR0", "AB").is_success);
    assert!(
        !client
            .write_all("0", &["AB".to_owned(), "CD".to_owned()])
            .is_success
    );
    assert!(requests.lock().unwrap().is_empty());
}

#[test]
fn invalid_station_prefixes_fail_without_changing_default() {
    let (mut client, requests) = client(Vec::new());
    for address in [
        "x=0;100",
        "x=248;100",
        "x=256;100",
        "x=-2;100",
        "x=+2;100",
        "x=abc;100",
        "x=;100",
        "x=2;",
        "x=2;x=3;100",
        "x=2;;100",
        "s=2;100",
        "2;100",
    ] {
        assert!(!client.read::<u16>(address, 1).is_success, "{address}");
        assert!(!client.write_string(address, "ABC").is_success, "{address}");
    }
    assert!(requests.lock().unwrap().is_empty());
    assert!(client.is_connected());
    assert_eq!(client.unit_id(), 1);
}

#[test]
fn station_override_does_not_leak_after_exception_or_disconnect() {
    let (mut client, requests) = client(vec![
        vec![0x83, 2],
        vec![3, 2, 0, 7],
        vec![4, 2, 0, 8],
        vec![3, 2, 0, 9],
    ]);
    assert!(!client.read::<u16>("x=2;100", 1).is_success);
    assert_eq!(&*client.read::<u16>("100", 1).to_result().unwrap(), &[7]);
    assert!(!client.read::<u16>("x=2;100", 1).is_success);
    assert!(!client.is_connected());
    assert_eq!(client.unit_id(), 1);
    client.connect().to_result().unwrap();
    assert_eq!(&*client.read::<u16>("100", 1).to_result().unwrap(), &[9]);
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|(unit, _)| *unit)
            .collect::<Vec<_>>(),
        vec![2, 1, 2, 1]
    );
}

#[test]
fn bit_and_numeric_writes_accept_station_prefix() {
    let (mut client, requests) = client(vec![
        vec![5, 0, 100, 0xff, 0],
        vec![6, 0, 100, 0x12, 0x34],
        vec![15, 0, 100, 0, 3],
    ]);
    assert!(client.write("x=2;100", true).to_result().unwrap());
    assert_eq!(
        client.write("x=2;100", 0x1234_u16).to_result().unwrap(),
        0x1234
    );
    assert_eq!(
        client
            .write_all("x=2;C100", &[true, false, true])
            .to_result()
            .unwrap(),
        3
    );
    assert!(requests.lock().unwrap().iter().all(|(unit, _)| *unit == 2));
    assert_eq!(client.unit_id(), 1);
}
