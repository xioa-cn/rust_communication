use rs_appliaction::communication::inovace::{InovanceModbusTcp, InovanceType};
use rs_appliaction::communication::timeout::Timeout;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn binary(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (u16, JoinHandle<()>) {
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
        handler(stream);
    });
    (port, worker)
}

fn request(stream: &mut TcpStream, expected_unit: u8, expected_pdu: &str) -> u16 {
    let mut header = [0; 7];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(&header[2..4], &[0, 0]);
    assert_eq!(header[6], expected_unit);
    let mut pdu = vec![0; usize::from(u16::from_be_bytes([header[4], header[5]])) - 1];
    stream.read_exact(&mut pdu).unwrap();
    assert_eq!(pdu, binary(expected_pdu));
    u16::from_be_bytes([header[0], header[1]])
}

fn reply(stream: &mut TcpStream, transaction: u16, unit: u8, pdu: &str) {
    let pdu = binary(pdu);
    let mut frame = transaction.to_be_bytes().to_vec();
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&((pdu.len() + 1) as u16).to_be_bytes());
    frame.push(unit);
    frame.extend(pdu);
    // 故意拆成小块，确认客户端使用 read_exact 语义，而不依赖 TCP 包边界。
    for chunk in frame.chunks(2) {
        stream.write_all(chunk).unwrap();
    }
}

fn client(series: InovanceType, port: u16) -> InovanceModbusTcp {
    InovanceModbusTcp::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        port,
        series,
        Timeout::new(1000, 1000),
    )
}

#[test]
fn all_series_construct_without_connecting_and_validate_network_settings() {
    for series in [
        InovanceType::AM,
        InovanceType::AC,
        InovanceType::AP,
        InovanceType::EVO,
        InovanceType::H3U,
        InovanceType::H5U,
        InovanceType::Easy,
    ] {
        let mut plc = client(series, 0);
        assert_eq!(plc.series(), series);
        assert!(!plc.is_connected());
        assert!(!plc.connect().is_success);
        assert!(!plc.is_connected());
        assert!(plc.disconnect().is_success);
    }
    let mut plc = InovanceModbusTcp::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        1,
        InovanceType::H5U,
        Timeout::new(0, 1000),
    );
    assert!(!plc.connect().is_success);
}

#[test]
fn tcp_reads_and_writes_every_series_through_mbap() {
    for (series, address, read_pdu, reply_pdu, write_pdu, write_reply) in [
        (
            InovanceType::H3U,
            "T10",
            "03 F0 0A 00 01",
            "03 02 12 34",
            "06 F0 0A 56 78",
            "06 F0 0A 56 78",
        ),
        (
            InovanceType::H5U,
            "D100",
            "03 00 64 00 01",
            "03 02 12 34",
            "06 00 64 56 78",
            "06 00 64 56 78",
        ),
        (
            InovanceType::Easy,
            "R100",
            "03 30 64 00 01",
            "03 02 12 34",
            "06 30 64 56 78",
            "06 30 64 56 78",
        ),
        (
            InovanceType::AM,
            "SD100",
            "33 00 64 00 01",
            "33 02 12 34",
            "36 00 64 56 78",
            "36 00 64 56 78",
        ),
        (
            InovanceType::AC,
            "MW100",
            "03 00 64 00 01",
            "03 02 12 34",
            "06 00 64 56 78",
            "06 00 64 56 78",
        ),
        (
            InovanceType::AP,
            "MW100",
            "03 00 64 00 01",
            "03 02 12 34",
            "06 00 64 56 78",
            "06 00 64 56 78",
        ),
        (
            InovanceType::EVO,
            "MW100",
            "03 00 64 00 01",
            "03 02 12 34",
            "06 00 64 56 78",
            "06 00 64 56 78",
        ),
    ] {
        let (port, worker) = server(move |mut stream| {
            let first = request(&mut stream, 7, read_pdu);
            reply(&mut stream, first, 7, reply_pdu);
            let second = request(&mut stream, 10, write_pdu);
            assert_ne!(first, second);
            reply(&mut stream, second, 10, write_reply);
            let mut extra = [0; 1];
            assert_eq!(stream.read(&mut extra).unwrap(), 0);
        });
        let mut plc = client(series, port);
        assert!(!plc.is_connected());
        assert!(plc.set_unit_id(7).is_success);
        plc.connect().to_result().unwrap();
        plc.connect().to_result().unwrap();
        assert_eq!(
            &*plc.read::<u16>(address, 1).to_result().unwrap(),
            &[0x1234]
        );
        assert_eq!(
            plc.write::<u16>(&format!("x=10;{address}"), 0x5678)
                .to_result()
                .unwrap(),
            0x5678
        );
        assert_eq!(plc.unit_id(), 7);
        plc.disconnect().to_result().unwrap();
        plc.disconnect().to_result().unwrap();
        worker.join().unwrap();
    }
}

#[test]
fn mismatched_transaction_disconnects_and_never_retries() {
    let (port, worker) = server(|mut stream| {
        let transaction = request(&mut stream, 1, "06 00 00 00 01");
        let mut header = transaction.wrapping_add(1).to_be_bytes().to_vec();
        header.extend_from_slice(&[0, 0, 0, 6, 1]);
        stream.write_all(&header).unwrap();
        let mut extra = [0; 1];
        match stream.read(&mut extra) {
            Ok(count) => assert_eq!(count, 0),
            Err(error) => assert!(matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            )),
        }
    });
    let mut plc = client(InovanceType::H5U, port);
    plc.connect().to_result().unwrap();
    let result = plc.write::<u16>("D0", 1);
    assert!(!result.is_success);
    assert!(result.msg.contains("unconfirmed"));
    assert!(!plc.is_connected());
    assert!(!plc.read::<u16>("D0", 1).is_success);
    worker.join().unwrap();
}

#[test]
fn am_extended_exception_is_not_mistaken_for_invalid_mbap() {
    let (port, worker) = server(|mut stream| {
        let transaction = request(&mut stream, 1, "33 00 00 00 01");
        reply(&mut stream, transaction, 1, "B3 02");
        let transaction = request(&mut stream, 1, "03 00 00 00 01");
        reply(&mut stream, transaction, 1, "03 02 00 01");
    });
    let mut plc = client(InovanceType::AM, port);
    plc.connect().to_result().unwrap();
    assert!(!plc.read::<u16>("SD0", 1).is_success);
    assert!(plc.is_connected());
    assert_eq!(&*plc.read::<u16>("MW0", 1).to_result().unwrap(), &[1]);
    worker.join().unwrap();
}

#[test]
fn partial_tcp_response_invalidates_the_session() {
    let (port, worker) = server(|mut stream| {
        request(&mut stream, 1, "03 00 00 00 01");
        stream.write_all(&[0, 1, 0]).unwrap();
    });
    let mut plc = client(InovanceType::H3U, port);
    plc.connect().to_result().unwrap();
    assert!(!plc.read::<u16>("D0", 1).is_success);
    assert!(!plc.is_connected());
    worker.join().unwrap();
}
