use rs_appliaction::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use rs_appliaction::communication::omron::{FinsRoute, OmronFinsTcp, OmronFinsUdp};
use rs_appliaction::communication::timeout::Timeout;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream, UdpSocket};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

fn tcp_frame(command: u32, data: &[u8]) -> Vec<u8> {
    let mut bytes = b"FINS".to_vec();
    bytes.extend_from_slice(&((data.len() + 8) as u32).to_be_bytes());
    bytes.extend_from_slice(&command.to_be_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(data);
    bytes
}

fn read_frame(stream: &mut TcpStream) -> (u32, Vec<u8>) {
    let mut header = [0; 16];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(&header[..4], b"FINS");
    assert_eq!(&header[12..], &[0; 4]);
    let length = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
    assert!((8..=2020).contains(&length));
    let mut payload = vec![0; length - 8];
    stream.read_exact(&mut payload).unwrap();
    (
        u32::from_be_bytes(header[8..12].try_into().unwrap()),
        payload,
    )
}

fn handshake(stream: &mut TcpStream, requested: u8, assigned: u8) {
    assert_eq!(
        read_frame(stream),
        (0, u32::from(requested).to_be_bytes().to_vec())
    );
    stream
        .write_all(&tcp_frame(1, &[0, 0, 0, assigned, 0, 0, 0, 10]))
        .unwrap();
}

fn response(request: &[u8], data: &[u8]) -> Vec<u8> {
    let mut reply = vec![0xc0, 0, 2];
    reply.extend_from_slice(&request[6..9]);
    reply.extend_from_slice(&request[3..6]);
    reply.extend_from_slice(&request[9..12]);
    reply.extend_from_slice(&[0, 0]);
    reply.extend_from_slice(data);
    reply
}

fn tcp_server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
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

fn tcp_client(port: u16) -> OmronFinsTcp {
    OmronFinsTcp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(1000, 1000))
}

fn udp_server(handler: impl FnOnce(UdpSocket) + Send + 'static) -> (u16, JoinHandle<()>) {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = socket.local_addr().unwrap().port();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let worker = thread::spawn(move || handler(socket));
    (port, worker)
}

fn udp_client(port: u16) -> OmronFinsUdp {
    let mut plc = OmronFinsUdp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(1000, 1000));
    plc.set_route(FinsRoute {
        source_node: 20,
        destination_node: 10,
        ..Default::default()
    })
    .to_result()
    .unwrap();
    plc
}

fn exchange_steps() -> Vec<(Vec<u8>, Vec<u8>)> {
    vec![
        (
            vec![1, 1, 0x82, 0, 100, 0, 0, 2],
            vec![0x12, 0x34, 0x56, 0x78],
        ),
        (vec![1, 1, 0x31, 0, 100, 15, 0, 2], vec![1, 0]),
        (
            vec![1, 2, 0x82, 0, 200, 0, 0, 2, 0x56, 0x78, 0x12, 0x34],
            vec![],
        ),
        (vec![1, 2, 0x31, 0, 100, 15, 0, 2, 1, 0], vec![]),
    ]
}

fn exercise<
    Client: DeviceBase + ReadBase<u16> + ReadBase<bool> + WriteBase<u32> + WriteBase<bool>,
>(
    plc: &mut Client,
) {
    assert!(plc.connect().to_result().unwrap());
    assert!(plc.connect().to_result().unwrap());
    assert_eq!(
        &*ReadBase::<u16>::read(plc, "D100", 2).to_result().unwrap(),
        &[0x1234, 0x5678]
    );
    assert_eq!(
        &*ReadBase::<bool>::read(plc, "W100.15", 2)
            .to_result()
            .unwrap(),
        &[true, false]
    );
    assert_eq!(
        WriteBase::<u32>::write(plc, "D200", 0x12345678)
            .to_result()
            .unwrap(),
        0x12345678
    );
    assert_eq!(
        WriteBase::<bool>::write_all(plc, "W100.15", &[true, false])
            .to_result()
            .unwrap(),
        2
    );
    assert!(plc.disconnect().to_result().unwrap());
    assert!(plc.disconnect().to_result().unwrap());
}

#[test]
fn tcp_negotiates_nodes_and_handles_fragmented_frames_with_shared_api() {
    let (port, worker) = tcp_server(|mut stream| {
        handshake(&mut stream, 0, 20);
        for (index, (body, data)) in exchange_steps().into_iter().enumerate() {
            let (command, request) = read_frame(&mut stream);
            assert_eq!(command, 2);
            assert_eq!(
                &request[..10],
                &[0x80, 0, 2, 0, 10, 0, 0, 20, 0, index as u8 + 1]
            );
            assert_eq!(&request[10..], &body);
            for part in tcp_frame(2, &response(&request, &data)).chunks(3) {
                stream.write_all(part).unwrap();
            }
        }
    });
    let mut plc = tcp_client(port);
    assert!(!plc.is_connected());
    exercise(&mut plc);
    assert!(!plc.is_connected());
    worker.join().unwrap();
}

#[test]
fn udp_sends_bare_fins_frames_with_the_same_api() {
    let (port, worker) = udp_server(|socket| {
        let mut buffer = [0; 2048];
        for (index, (body, data)) in exchange_steps().into_iter().enumerate() {
            let (count, peer) = socket.recv_from(&mut buffer).unwrap();
            let request = &buffer[..count];
            assert_eq!(
                &request[..10],
                &[0x80, 0, 2, 0, 10, 0, 0, 20, 0, index as u8 + 1]
            );
            assert_eq!(&request[10..], &body);
            socket.send_to(&response(request, &data), peer).unwrap();
        }
    });
    let mut plc = udp_client(port);
    exercise(&mut plc);
    worker.join().unwrap();
}

#[test]
fn tcp_explicit_nodes_and_reconnect_use_the_configured_not_previous_nodes() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        for assigned in [20, 21] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            handshake(&mut stream, 0, assigned);
        }
    });
    let mut plc = tcp_client(port);
    for assigned in [20, 21] {
        plc.connect().to_result().unwrap();
        assert_eq!(plc.route().source_node, assigned);
        assert_eq!(plc.route().destination_node, 10);
        plc.disconnect().to_result().unwrap();
    }
    worker.join().unwrap();
    let (port, worker) = tcp_server(|mut stream| {
        handshake(&mut stream, 25, 25);
    });
    let mut plc = tcp_client(port);
    plc.set_route(FinsRoute {
        source_node: 25,
        destination_network: 2,
        destination_node: 40,
        ..Default::default()
    })
    .to_result()
    .unwrap();
    plc.connect().to_result().unwrap();
    assert_eq!(plc.route().source_node, 25);
    assert_eq!(plc.route().destination_node, 40);
    worker.join().unwrap();
}

#[test]
fn bad_tcp_handshakes_leave_no_half_connected_session() {
    let mut bad_magic = tcp_frame(1, &[0, 0, 0, 20, 0, 0, 0, 10]);
    bad_magic[0] = b'X';
    let mut bad_length = tcp_frame(1, &[]);
    bad_length[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    let mut server_error = tcp_frame(1, &[]);
    server_error[15] = 0x21;
    let replies = [
        bad_magic,
        bad_length,
        server_error,
        tcp_frame(2, &[0; 8]),
        tcp_frame(1, &[0; 7]),
        tcp_frame(1, &[0, 0, 0, 0, 0, 0, 0, 10]),
        tcp_frame(1, &[0, 0, 1, 0, 0, 0, 0, 10]),
        tcp_frame(1, &[0, 0, 0, 20, 0, 0, 0, 255]),
    ];
    for reply in replies {
        let (port, worker) = tcp_server(move |mut stream| {
            assert_eq!(read_frame(&mut stream).0, 0);
            let _ = stream.write_all(&reply);
        });
        let mut plc = tcp_client(port);
        assert!(!plc.connect().is_success);
        assert!(!plc.is_connected());
        assert!(!plc.read::<u16>("D0", 1).is_success);
        worker.join().unwrap();
    }
    let (port, worker) = tcp_server(|mut stream| {
        handshake(&mut stream, 25, 24);
    });
    let mut plc = tcp_client(port);
    plc.set_route(FinsRoute {
        source_node: 25,
        ..Default::default()
    })
    .to_result()
    .unwrap();
    assert!(!plc.connect().is_success);
    assert!(!plc.is_connected());
    worker.join().unwrap();
}

#[test]
fn tcp_malformed_envelopes_and_wrong_sid_close_the_connection() {
    for kind in 0..6 {
        let (port, worker) = tcp_server(move |mut stream| {
            handshake(&mut stream, 0, 20);
            let (_, request) = read_frame(&mut stream);
            let mut reply = tcp_frame(2, &response(&request, &[0, 1]));
            match kind {
                0 => reply[0] = b'X',
                1 => reply[4..8].copy_from_slice(&u32::MAX.to_be_bytes()),
                2 => reply[11] = 3,
                3 => reply[15] = 0x20,
                4 => reply[25] = 99,
                _ => {
                    reply.pop();
                }
            }
            let _ = stream.write_all(&reply);
        });
        let mut plc = tcp_client(port);
        plc.connect().to_result().unwrap();
        assert!(!plc.read::<u16>("D0", 1).is_success);
        assert!(!plc.is_connected());
        worker.join().unwrap();
    }
}

#[test]
fn tcp_controller_error_does_not_desynchronize_the_next_frame() {
    let (port, worker) = tcp_server(|mut stream| {
        handshake(&mut stream, 0, 20);
        let (_, request) = read_frame(&mut stream);
        let mut reply = response(&request, &[]);
        reply[12..14].copy_from_slice(&[0x11, 3]);
        stream.write_all(&tcp_frame(2, &reply)).unwrap();
        let (_, request) = read_frame(&mut stream);
        stream
            .write_all(&tcp_frame(2, &response(&request, &[0, 42])))
            .unwrap();
    });
    let mut plc = tcp_client(port);
    plc.connect().to_result().unwrap();
    assert!(plc.read::<u16>("D0", 1).msg.contains("0x1103"));
    assert!(plc.is_connected());
    assert_eq!(&*plc.read::<u16>("D0", 1).to_result().unwrap(), &[42]);
    worker.join().unwrap();
}

#[test]
fn tcp_deadline_bounds_a_trickling_response() {
    let (port, worker) = tcp_server(|mut stream| {
        handshake(&mut stream, 0, 20);
        let (_, request) = read_frame(&mut stream);
        let reply = tcp_frame(2, &response(&request, &[0, 1]));
        for byte in reply.iter().take(8) {
            if stream.write_all(&[*byte]).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
    });
    let mut plc = OmronFinsTcp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(1000, 120));
    plc.connect().to_result().unwrap();
    let start = Instant::now();
    assert!(!plc.read::<u16>("D0", 1).is_success);
    assert!(!plc.is_connected());
    assert!(start.elapsed() < Duration::from_millis(700));
    worker.join().unwrap();
}

#[test]
fn writes_are_not_retried_when_tcp_peer_closes_without_acknowledging() {
    let (port, worker) = tcp_server(|mut stream| {
        handshake(&mut stream, 0, 20);
        let (_, request) = read_frame(&mut stream);
        assert_eq!(&request[10..12], &[1, 2]);
    });
    let mut plc = tcp_client(port);
    plc.connect().to_result().unwrap();
    let result = plc.write("D0", 123_u16);
    assert!(!result.is_success);
    assert!(result.msg.contains("not retried"));
    assert!(!plc.is_connected());
    worker.join().unwrap();
}

#[test]
fn udp_ignores_old_sid_and_other_peers() {
    let (port, worker) = udp_server(|socket| {
        let mut buffer = [0; 2048];
        let (count, peer) = socket.recv_from(&mut buffer).unwrap();
        let request = &buffer[..count];
        let valid = response(request, &[0, 42]);
        let other_peer = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        other_peer
            .send_to(&response(request, &[0, 99]), peer)
            .unwrap();
        let mut stale = valid.clone();
        stale[9] = stale[9].wrapping_sub(1);
        socket.send_to(&stale, peer).unwrap();
        socket.send_to(&valid, peer).unwrap();
    });
    let mut plc = udp_client(port);
    plc.connect().to_result().unwrap();
    assert_eq!(&*plc.read::<u16>("D0", 1).to_result().unwrap(), &[42]);
    worker.join().unwrap();
}

#[test]
fn udp_rejects_truncation_oversize_and_wrong_routes() {
    for kind in 0..3 {
        let (port, worker) = udp_server(move |socket| {
            let mut buffer = [0; 2048];
            let (count, peer) = socket.recv_from(&mut buffer).unwrap();
            let mut reply = response(&buffer[..count], &[0, 1]);
            match kind {
                0 => reply.truncate(13),
                1 => reply.resize(2013, 0),
                _ => reply[4] = 99,
            }
            socket.send_to(&reply, peer).unwrap();
        });
        let mut plc = udp_client(port);
        plc.connect().to_result().unwrap();
        assert!(!plc.read::<u16>("D0", 1).is_success);
        assert!(!plc.is_connected());
        worker.join().unwrap();
    }
}

#[test]
fn udp_stale_traffic_cannot_extend_the_transaction_deadline() {
    let (port, worker) = udp_server(|socket| {
        let mut buffer = [0; 2048];
        let (count, peer) = socket.recv_from(&mut buffer).unwrap();
        let mut reply = response(&buffer[..count], &[0, 1]);
        reply[9] = 0;
        for _ in 0..8 {
            let _ = socket.send_to(&reply, peer);
            thread::sleep(Duration::from_millis(30));
        }
    });
    let mut plc = OmronFinsUdp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(1000, 120));
    plc.connect().to_result().unwrap();
    let start = Instant::now();
    assert!(!plc.read::<u16>("D0", 1).is_success);
    assert!(!plc.is_connected());
    assert!(start.elapsed() < Duration::from_millis(700));
    worker.join().unwrap();
}

#[test]
fn invalid_network_settings_fail_before_any_connection() {
    for (address, port, timeout) in [
        (IpAddr::V4(Ipv4Addr::LOCALHOST), 0, Timeout::default()),
        (Ipv4Addr::LOCALHOST.into(), 9600, Timeout::new(0, 1000)),
        (Ipv4Addr::LOCALHOST.into(), 9600, Timeout::new(1000, -1)),
        (Ipv4Addr::BROADCAST.into(), 9600, Timeout::default()),
        (Ipv4Addr::UNSPECIFIED.into(), 9600, Timeout::default()),
        (Ipv4Addr::new(224, 0, 0, 1).into(), 9600, Timeout::default()),
        (Ipv6Addr::LOCALHOST.into(), 9600, Timeout::default()),
    ] {
        let mut tcp = OmronFinsTcp::new(address, port, timeout);
        let mut udp = OmronFinsUdp::new(address, port, timeout);
        assert!(!tcp.connect().is_success);
        assert!(!udp.connect().is_success);
        assert!(!tcp.is_connected());
        assert!(!udp.is_connected());
    }
}

#[test]
fn udp_connect_is_local_only_and_does_not_probe_a_plc() {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut plc = udp_client(socket.local_addr().unwrap().port());
    plc.connect().to_result().unwrap();
    assert!(plc.is_connected());
    let mut buffer = [0; 2048];
    assert!(socket.recv_from(&mut buffer).is_err());
    plc.disconnect().to_result().unwrap();
    assert!(!plc.is_connected());
}
