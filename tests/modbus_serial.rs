use rs_appliaction::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use rs_appliaction::communication::modbus::{ModbusAscii, ModbusRtu};
use std::io::{self, Cursor, Read, Write};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct StreamState {
    written: Vec<u8>,
    dropped: bool,
}

struct TestStream {
    reply: Cursor<Vec<u8>>,
    state: Arc<Mutex<StreamState>>,
}

impl Read for TestStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let length = buffer.len().min(2);
        self.reply.read(&mut buffer[..length])
    }
}

impl Write for TestStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let length = buffer.len().min(2);
        self.state
            .lock()
            .unwrap()
            .written
            .extend_from_slice(&buffer[..length]);
        Ok(length)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for TestStream {
    fn drop(&mut self) {
        self.state.lock().unwrap().dropped = true;
    }
}

fn stream(reply: Vec<u8>) -> (TestStream, Arc<Mutex<StreamState>>) {
    let state = Arc::new(Mutex::new(StreamState::default()));
    (
        TestStream {
            reply: Cursor::new(reply),
            state: Arc::clone(&state),
        },
        state,
    )
}

fn binary(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|part| u8::from_str_radix(part, 16).unwrap())
        .collect()
}

fn exercise<Client>(client: &mut Client)
where
    Client: DeviceBase
        + ReadBase<bool>
        + ReadBase<u16>
        + ReadBase<i16>
        + WriteBase<bool>
        + WriteBase<u16>,
{
    assert!(client.connect().to_result().unwrap());
    assert!(client.connect().to_result().unwrap());
    assert_eq!(
        &*ReadBase::<u16>::read(client, "HR107", 3)
            .to_result()
            .unwrap(),
        &[0xae41, 0x5652, 0x4340]
    );
    assert_eq!(
        &*ReadBase::<bool>::read(client, "C19", 10)
            .to_result()
            .unwrap(),
        &[
            true, false, true, true, false, false, true, true, true, false
        ]
    );
    assert_eq!(
        &*ReadBase::<bool>::read(client, "DI10", 9)
            .to_result()
            .unwrap(),
        &[true, false, true, false, true, false, true, false, true]
    );
    assert_eq!(
        &*ReadBase::<i16>::read(client, "IR8", 1).to_result().unwrap(),
        &[-2]
    );
    assert!(
        WriteBase::<bool>::write(client, "C19", true)
            .to_result()
            .unwrap()
    );
    assert_eq!(
        WriteBase::<u16>::write(client, "HR100", 0x1234)
            .to_result()
            .unwrap(),
        0x1234
    );
    assert_eq!(
        WriteBase::<bool>::write_all(
            client,
            "C19",
            &[true, false, true, false, true, false, true, false, true]
        )
        .to_result()
        .unwrap(),
        9
    );
    assert_eq!(
        WriteBase::<u16>::write_all(client, "HR100", &[0x1234, 0x5678])
            .to_result()
            .unwrap(),
        2
    );
}

#[test]
fn rtu_all_functions_match_fixed_crc_vectors_with_short_io() {
    let requests = binary(
        "11 03 00 6B 00 03 76 87 11 01 00 13 00 0A 4F 58 11 02 00 0A 00 09 9A 9E 11 04 00 08 00 01 B2 98 11 05 00 13 FF 00 7F 6F 11 06 00 64 12 34 C7 F2 11 0F 00 13 00 09 02 55 01 D4 8F 11 10 00 64 00 02 04 12 34 56 78 DB 80",
    );
    let replies = binary(
        "11 03 06 AE 41 56 52 43 40 49 AD 11 01 02 CD 01 ED 6F 11 02 02 55 01 86 EB 11 04 02 FF FE B8 83 11 05 00 13 FF 00 7F 6F 11 06 00 64 12 34 C7 F2 11 0F 00 13 00 09 66 98 11 10 00 64 00 02 02 87",
    );
    let (stream, state) = stream(replies);
    let mut client = ModbusRtu::new(stream, 17, 9600);
    exercise(&mut client);
    assert_eq!(state.lock().unwrap().written, requests);
    assert!(client.disconnect().is_success);
    assert!(state.lock().unwrap().dropped);
    assert!(client.disconnect().is_success);
    assert!(!client.connect().is_success);
}

#[test]
fn ascii_all_functions_match_fixed_lrc_vectors_with_short_io() {
    let requests = ":1103006B00037E\r\n:11010013000AD1\r\n:1102000A0009DA\r\n:110400080001E2\r\n:11050013FF00D8\r\n:1106006412343F\r\n:110F001300090255016C\r\n:111000640002041234567861\r\n";
    let replies = ":110306AE4156524340CC\r\n:110102CD011E\r\n:110202550195\r\n:110402FFFEEC\r\n:11050013FF00D8\r\n:1106006412343F\r\n:110F00130009C4\r\n:11100064000279\r\n";
    let (stream, state) = stream(replies.as_bytes().to_vec());
    let mut client = ModbusAscii::new(stream, 17);
    exercise(&mut client);
    assert_eq!(state.lock().unwrap().written, requests.as_bytes());
    assert!(client.disconnect().is_success);
    assert!(state.lock().unwrap().dropped);
    assert!(client.disconnect().is_success);
    assert!(!client.connect().is_success);
}

#[test]
fn serial_exceptions_preserve_frame_boundaries_and_connection() {
    let (rtu_stream, _) = stream(binary("11 83 02 C1 34 11 03 06 AE 41 56 52 43 40 49 AD"));
    let mut rtu = ModbusRtu::new(rtu_stream, 17, 115200);
    rtu.connect().to_result().unwrap();
    assert!(rtu.read::<u16>("107", 3).msg.contains("0x02"));
    assert!(rtu.is_connected());
    assert_eq!(
        &*rtu.read::<u16>("107", 3).to_result().unwrap(),
        &[0xae41, 0x5652, 0x4340]
    );
    let (ascii_stream, _) = stream(b":1183026A\r\n:110306AE4156524340CC\r\n".to_vec());
    let mut ascii = ModbusAscii::new(ascii_stream, 17);
    ascii.connect().to_result().unwrap();
    assert!(ascii.read::<u16>("107", 3).msg.contains("0x02"));
    assert!(ascii.is_connected());
    assert_eq!(
        &*ascii.read::<u16>("107", 3).to_result().unwrap(),
        &[0xae41, 0x5652, 0x4340]
    );
}

#[test]
fn corrupt_rtu_frames_close_and_drop_stream() {
    let valid = binary("11 03 06 AE 41 56 52 43 40 49 AD");
    let mut replies = Vec::new();
    for (index, byte) in [(0, 18), (1, 4), (2, 251), (10, 0)] {
        let mut reply = valid.clone();
        reply[index] = byte;
        replies.push(reply);
    }
    replies.push(valid[..10].to_vec());
    replies.push(Vec::new());
    for reply in replies {
        let (stream, state) = stream(reply);
        let mut client = ModbusRtu::new(stream, 17, 115200);
        client.connect().to_result().unwrap();
        assert!(!client.read::<u16>("107", 3).is_success);
        assert!(!client.is_connected());
        assert!(state.lock().unwrap().dropped);
        assert!(!client.connect().is_success);
        assert!(client.into_inner().is_none());
    }
}

#[test]
fn invalid_ascii_frames_are_bounded_and_close_stream() {
    let mut replies = [
        "110306AE4156524340CC\r\n",
        ":110306AE415652434000\r\n",
        ":110306AE4156524340CC\rX",
        ":110306AE4156524340CC\n",
        ":110306AE4156524340CC",
        ":120306AE4156524340CB\r\n",
        ":11030ZAE4156524340CC\r\n",
        ":110306AE4156524340C\r\n",
        ":11\r\n",
    ]
    .into_iter()
    .map(|text| text.as_bytes().to_vec())
    .collect::<Vec<_>>();
    replies.push(format!(":{}\r\n", "0".repeat(512)).into_bytes());
    for reply in replies {
        let (stream, state) = stream(reply);
        let mut client = ModbusAscii::new(stream, 17);
        client.connect().to_result().unwrap();
        assert!(!client.read::<u16>("107", 3).is_success);
        assert!(!client.is_connected());
        assert!(state.lock().unwrap().dropped);
    }
}

#[test]
fn lowercase_ascii_and_returning_owned_stream_are_supported() {
    let (stream, state) = stream(b":110306ae4156524340cc\r\n".to_vec());
    let mut client = ModbusAscii::new(stream, 17);
    client.connect().to_result().unwrap();
    assert_eq!(
        &*client.read::<u16>("107", 3).to_result().unwrap(),
        &[0xae41, 0x5652, 0x4340]
    );
    let stream = client.into_inner().unwrap();
    assert!(!state.lock().unwrap().dropped);
    drop(stream);
    assert!(state.lock().unwrap().dropped);
}

#[test]
fn serial_configuration_and_broadcasts_are_validated_before_io() {
    for unit in [0, 248, 255] {
        let (rtu_stream, state) = stream(Vec::new());
        let mut rtu = ModbusRtu::new(rtu_stream, unit, 9600);
        assert!(!rtu.connect().is_success);
        assert!(state.lock().unwrap().written.is_empty());
        let (ascii_stream, state) = stream(Vec::new());
        let mut ascii = ModbusAscii::new(ascii_stream, unit);
        assert!(!ascii.connect().is_success);
        assert!(state.lock().unwrap().written.is_empty());
    }
    let (stream, state) = stream(Vec::new());
    let mut rtu = ModbusRtu::new(stream, 1, 0);
    assert!(!rtu.connect().is_success);
    assert!(!rtu.set_unit_id(248).is_success);
    assert_eq!(rtu.unit_id(), 1);
    assert!(!rtu.read::<u16>("0", 1).is_success);
    assert!(state.lock().unwrap().written.is_empty());
}

#[test]
fn rtu_strings_and_station_override_match_fixed_frames() {
    let (stream, state) = stream(binary(
        "02 03 06 68 65 6C 6C 6F 21 41 C0 11 03 06 68 65 6C 6C 6F 21 98 F0",
    ));
    let mut client = ModbusRtu::new(stream, 17, 115200);
    client.connect().to_result().unwrap();
    assert_eq!(
        &*client.read::<String>("x=2;0", 5).to_result().unwrap(),
        &["hello"]
    );
    assert_eq!(client.read_string("0", 5).to_result().unwrap(), "hello");
    assert_eq!(client.unit_id(), 17);
    assert_eq!(
        state.lock().unwrap().written,
        binary("02 03 00 00 00 03 05 F8 11 03 00 00 00 03 07 5B")
    );
}

#[test]
fn ascii_strings_and_station_override_match_fixed_frames() {
    let (stream, state) = stream(b":02030668656C6C6F21C0\r\n:11030668656C6C6F21B1\r\n".to_vec());
    let mut client = ModbusAscii::new(stream, 17);
    client.connect().to_result().unwrap();
    assert_eq!(
        &*client.read::<String>("x=2;0", 5).to_result().unwrap(),
        &["hello"]
    );
    assert_eq!(client.read_string("0", 5).to_result().unwrap(), "hello");
    assert_eq!(client.unit_id(), 17);
    assert_eq!(
        state.lock().unwrap().written,
        b":020300000003F8\r\n:110300000003E9\r\n"
    );
}

#[test]
fn serial_station_overrides_are_validated_before_any_bytes_are_sent() {
    let (rtu_stream, rtu_state) = stream(Vec::new());
    let (ascii_stream, ascii_state) = stream(Vec::new());
    let mut rtu = ModbusRtu::new(rtu_stream, 17, 9600);
    let mut ascii = ModbusAscii::new(ascii_stream, 17);
    rtu.connect().to_result().unwrap();
    ascii.connect().to_result().unwrap();
    for address in ["x=0;0", "x=248;0", "x=255;0"] {
        assert!(!rtu.read::<String>(address, 2).is_success);
        assert!(!ascii.read::<String>(address, 2).is_success);
    }
    assert!(rtu_state.lock().unwrap().written.is_empty());
    assert!(ascii_state.lock().unwrap().written.is_empty());
    assert_eq!(rtu.unit_id(), 17);
    assert_eq!(ascii.unit_id(), 17);
}
