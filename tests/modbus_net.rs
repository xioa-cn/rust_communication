use rs_appliaction::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use rs_appliaction::communication::modbus::{ModbusTcp, ModbusUdp};
use rs_appliaction::communication::timeout::Timeout;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, UdpSocket};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

type Step = (Vec<u8>, Vec<u8>);

fn binary(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|part| u8::from_str_radix(part, 16).unwrap())
        .collect()
}

fn frame(transaction: u16, pdu: &[u8]) -> Vec<u8> {
    let mut result = transaction.to_be_bytes().to_vec();
    result.extend_from_slice(&[0, 0, 0, (pdu.len() + 1) as u8, 1]);
    result.extend_from_slice(pdu);
    result
}

fn tcp_script(steps: Vec<Step>) -> (u16, JoinHandle<()>) {
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
        for (expected, reply) in steps {
            let mut request = vec![0; expected.len()];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(request, expected);
            for chunk in reply.chunks(2) {
                if stream.write_all(chunk).is_err() {
                    break;
                }
            }
        }
    });
    (port, worker)
}

fn udp_script(steps: Vec<Step>) -> (u16, JoinHandle<()>) {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut buffer = [0; 1024];
        for (expected, reply) in steps {
            let (length, peer) = socket.recv_from(&mut buffer).unwrap();
            assert_eq!(&buffer[..length], expected);
            socket.send_to(&reply, peer).unwrap();
        }
    });
    (port, worker)
}

fn common_steps() -> Vec<Step> {
    [
        ("01 00 13 00 0A", "01 02 CD 01"),
        ("02 00 0A 00 09", "02 02 55 01"),
        ("03 00 6B 00 03", "03 06 AE 41 56 52 43 40"),
        ("04 00 08 00 01", "04 02 FF FE"),
        ("05 00 13 FF 00", "05 00 13 FF 00"),
        ("06 00 64 12 34", "06 00 64 12 34"),
        ("0F 00 13 00 09 02 55 01", "0F 00 13 00 09"),
        ("10 00 64 00 02 04 12 34 56 78", "10 00 64 00 02"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (request, reply))| {
        (
            frame(index as u16 + 1, &binary(request)),
            frame(index as u16 + 1, &binary(reply)),
        )
    })
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
    let coils = ReadBase::<bool>::read(client, "C19", 10)
        .to_result()
        .unwrap();
    assert_eq!(
        &*coils,
        &[
            true, false, true, true, false, false, true, true, true, false
        ]
    );
    let inputs = ReadBase::<bool>::read(client, "DI10", 9)
        .to_result()
        .unwrap();
    assert_eq!(
        &*inputs,
        &[true, false, true, false, true, false, true, false, true]
    );
    let registers = ReadBase::<u16>::read(client, "HR107", 3)
        .to_result()
        .unwrap();
    assert_eq!(&*registers, &[0xae41, 0x5652, 0x4340]);
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
    assert!(client.disconnect().to_result().unwrap());
    assert!(client.disconnect().to_result().unwrap());
}

#[test]
fn tcp_supports_all_eight_functions_and_device_traits() {
    let (port, worker) = tcp_script(common_steps());
    let mut client = ModbusTcp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    exercise(&mut client);
    assert!(!client.is_connected());
    worker.join().unwrap();
}

#[test]
fn udp_supports_all_eight_functions_and_device_traits() {
    let (port, worker) = udp_script(common_steps());
    let mut client = ModbusUdp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    exercise(&mut client);
    assert!(!client.is_connected());
    worker.join().unwrap();
}

#[test]
fn tcp_fixed_frame_matches_wire_bytes() {
    let (port, worker) = tcp_script(vec![(
        binary("00 01 00 00 00 06 01 03 00 00 00 01"),
        binary("00 01 00 00 00 05 01 03 02 12 34"),
    )]);
    let mut client = ModbusTcp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    assert_eq!(&*client.read::<u16>("0", 1).to_result().unwrap(), &[0x1234]);
    worker.join().unwrap();
}

#[test]
fn tcp_accepts_fragmented_headers_and_payloads_at_every_boundary() {
    for split in 1..11 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_nodelay(true).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 12];
            stream.read_exact(&mut request).unwrap();
            assert_eq!(request.as_slice(), frame(1, &binary("03 00 00 00 01")));
            let reply = frame(1, &binary("03 02 12 34"));
            stream.write_all(&reply[..split]).unwrap();
            thread::sleep(Duration::from_millis(5));
            stream.write_all(&reply[split..]).unwrap();
        });
        let mut client = ModbusTcp::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port, Timeout::default());
        assert!(client.connect().is_success);
        assert_eq!(&*client.read::<u16>("0", 1).to_result().unwrap(), &[0x1234]);
        worker.join().unwrap();
    }
}

#[test]
fn exception_keeps_tcp_connection_usable() {
    let (port, worker) = tcp_script(vec![
        (frame(1, &binary("03 00 00 00 01")), frame(1, &[0x83, 2])),
        (
            frame(2, &binary("03 00 00 00 01")),
            frame(2, &binary("03 02 00 07")),
        ),
    ]);
    let mut client = ModbusTcp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    let result = client.read::<u16>("0", 1);
    assert!(!result.is_success);
    assert!(result.msg.contains("0x02"));
    assert!(client.is_connected());
    assert_eq!(&*client.read::<u16>("0", 1).to_result().unwrap(), &[7]);
    worker.join().unwrap();
}

#[test]
fn malformed_tcp_responses_close_connection() {
    let valid = frame(1, &binary("03 02 00 07"));
    let mut replies = Vec::new();
    for (position, value) in [(1, 2), (3, 1), (6, 2), (5, 0), (5, 255)] {
        let mut reply = valid.clone();
        reply[position] = value;
        replies.push(reply);
    }
    replies.push(frame(1, &binary("04 02 00 07")));
    replies.push(frame(1, &binary("03 03 00 07")));
    replies.push(frame(1, &[3]));
    replies.push(frame(1, &[0x83, 2, 0]));
    replies.push(binary("00 01 00 00 00 05 01 03"));
    for reply in replies {
        let (port, worker) = tcp_script(vec![(frame(1, &binary("03 00 00 00 01")), reply)]);
        let mut client = ModbusTcp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(1000, 300));
        client.connect().to_result().unwrap();
        assert!(!client.read::<u16>("0", 1).is_success);
        assert!(!client.is_connected());
        assert!(client.read::<u16>("0", 1).msg.contains("not connected"));
        worker.join().unwrap();
    }
}

#[test]
fn invalid_write_echo_is_not_success() {
    let (port, worker) = tcp_script(vec![(
        frame(1, &binary("06 00 00 00 07")),
        frame(1, &binary("06 00 01 00 07")),
    )]);
    let mut client = ModbusTcp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    let result = client.write("0", 7_u16);
    assert!(!result.is_success);
    assert!(result.msg.contains("unconfirmed"));
    assert!(!client.is_connected());
    worker.join().unwrap();
}

#[test]
fn tcp_deadline_covers_the_whole_response() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = [0; 12];
        stream.read_exact(&mut request).unwrap();
        for byte in frame(1, &binary("03 02 00 07")) {
            thread::sleep(Duration::from_millis(40));
            if stream.write_all(&[byte]).is_err() {
                break;
            }
        }
    });
    let mut client = ModbusTcp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(1000, 120));
    client.connect().to_result().unwrap();
    let start = Instant::now();
    assert!(!client.read::<u16>("0", 1).is_success);
    assert!(start.elapsed() < Duration::from_millis(400));
    assert!(!client.is_connected());
    worker.join().unwrap();
}

#[test]
fn udp_ignores_stale_transaction_then_accepts_current_reply() {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut request = [0; 260];
        let (_, peer) = socket.recv_from(&mut request).unwrap();
        socket
            .send_to(&frame(0, &binary("03 02 00 08")), peer)
            .unwrap();
        socket
            .send_to(&frame(1, &binary("03 02 00 07")), peer)
            .unwrap();
    });
    let mut client = ModbusUdp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    assert_eq!(&*client.read::<u16>("0", 1).to_result().unwrap(), &[7]);
    worker.join().unwrap();
}

#[test]
fn udp_rejects_truncated_trailing_and_oversized_datagrams() {
    let valid = frame(1, &binary("03 02 00 07"));
    let mut trailing = valid.clone();
    trailing.push(0);
    let mut oversized = valid.clone();
    oversized.resize(600, 0);
    for reply in [valid[..10].to_vec(), trailing, oversized, vec![0, 1]] {
        let (port, worker) = udp_script(vec![(frame(1, &binary("03 00 00 00 01")), reply)]);
        let mut client = ModbusUdp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(1000, 300));
        client.connect().to_result().unwrap();
        assert!(!client.read::<u16>("0", 1).is_success);
        assert!(!client.is_connected());
        worker.join().unwrap();
    }
}

#[test]
fn timed_out_write_is_sent_once_and_not_retried() {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut request = [0; 260];
        let (count, _) = socket.recv_from(&mut request).unwrap();
        assert_eq!(&request[..count], &frame(1, &binary("06 00 00 00 07")));
        socket
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        assert!(socket.recv_from(&mut request).is_err());
    });
    let mut client = ModbusUdp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(1000, 80));
    client.connect().to_result().unwrap();
    assert!(!client.write("0", 7_u16).is_success);
    assert!(!client.is_connected());
    worker.join().unwrap();
}

#[test]
fn invalid_configuration_is_rejected_without_network_access() {
    let address = IpAddr::V4(Ipv4Addr::LOCALHOST);
    for (port, timeout) in [
        (0, Timeout::default()),
        (502, Timeout::new(0, 100)),
        (502, Timeout::new(100, -1)),
    ] {
        assert!(!ModbusTcp::new(address, port, timeout).connect().is_success);
        assert!(!ModbusUdp::new(address, port, timeout).connect().is_success);
    }
    let mut client = ModbusTcp::new(address, 502, Timeout::default());
    assert!(!client.is_connected());
    assert!(!client.set_unit_id(0).is_success);
    assert_eq!(client.unit_id(), 1);
    assert!(client.set_unit_id(255).is_success);
    assert_eq!(client.unit_id(), 255);
    assert!(client.read::<u16>("0", 1).msg.contains("not connected"));
}
