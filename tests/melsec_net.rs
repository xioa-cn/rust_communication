//! 本地回环服务端使用独立固定报文验证协议；不连接或写入真实 PLC。
use rs_appliaction::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use rs_appliaction::communication::melsec::*;
use rs_appliaction::communication::timeout::Timeout;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, UdpSocket};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// 一步表示客户端应发送的完整报文及模拟 PLC 的完整回复。
type Step = (Vec<u8>, Vec<u8>);

#[test]
#[ignore = "local loopback throughput measurement"]
fn loopback_io_benchmark() {
    let steps = standard_steps(false);
    let script = (0..6200)
        .flat_map(|_| [steps[0].clone(), steps[1].clone()])
        .collect();
    let (port, worker) = tcp_script_chunks(script, 65535);
    let mut plc = MelsecMcNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(1000, 1000));
    plc.connect().to_result().unwrap();
    for round in 0..4 {
        let iterations = if round == 0 { 200 } else { 2000 };
        let mut read_time = Duration::ZERO;
        let mut write_time = Duration::ZERO;
        for _ in 0..iterations {
            let started = std::time::Instant::now();
            assert_eq!(
                &*plc.read::<u32>("D100", 2).to_result().unwrap(),
                &[0x12345678, 0x90abcdef]
            );
            read_time += started.elapsed();
            let started = std::time::Instant::now();
            plc.write::<u16>("D100", 0x1234).to_result().unwrap();
            write_time += started.elapsed();
        }
        if round > 0 {
            println!(
                "MC round={round} read_us={:.2} write_us={:.2}",
                read_time.as_secs_f64() * 1e6 / 2000.0,
                write_time.as_secs_f64() * 1e6 / 2000.0
            );
        }
    }
    plc.disconnect().to_result().unwrap();
    worker.join().unwrap();
}

/// 以十六进制文字给出独立报文向量，避免测试依赖被测编码器。
fn binary(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|part| u8::from_str_radix(part, 16).unwrap())
        .collect()
}

/// 创建 TCP 模拟设备；故意分段回复以覆盖流式拆包。
fn tcp_script(steps: Vec<Step>) -> (u16, JoinHandle<()>) {
    tcp_script_chunks(steps, 3)
}

fn tcp_script_chunks(steps: Vec<Step>, chunk_size: usize) -> (u16, JoinHandle<()>) {
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
            for chunk in reply.chunks(chunk_size) {
                stream.write_all(chunk).unwrap();
            }
        }
    });
    (port, worker)
}

/// UDP 每次只接收一个完整数据报，同时检查实际报文长度。
fn udp_script(steps: Vec<Step>) -> (u16, JoinHandle<()>) {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let port = socket.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let mut buffer = vec![0; 65535];
        for (expected, reply) in steps {
            let (count, peer) = socket.recv_from(&mut buffer).unwrap();
            assert_eq!(&buffer[..count], &expected);
            socket.send_to(&reply, peer).unwrap();
        }
    });
    (port, worker)
}

/// 固定响应的路由和长度与手工构造的 3E 请求对应。
fn mc_reply(payload: &[u8]) -> Vec<u8> {
    let mut response = binary("D0 00 00 FF FF 03 00");
    response.extend_from_slice(&((payload.len() + 2) as u16).to_le_bytes());
    response.extend_from_slice(&[0, 0]);
    response.extend_from_slice(payload);
    response
}

/// 标准 3E 二进制请求；R 请求独立给出扩展地址字段。
fn standard_steps(r_series: bool) -> Vec<Step> {
    let requests = if r_series {
        [
            "50 00 00 FF FF 03 00 0E 00 10 00 01 04 02 00 64 00 00 00 A8 00 04 00",
            "50 00 00 FF FF 03 00 10 00 10 00 01 14 02 00 64 00 00 00 A8 00 01 00 34 12",
            "50 00 00 FF FF 03 00 0E 00 10 00 01 04 03 00 64 00 00 00 90 00 03 00",
            "50 00 00 FF FF 03 00 10 00 10 00 01 14 03 00 64 00 00 00 90 00 03 00 10 10",
        ]
    } else {
        [
            "50 00 00 FF FF 03 00 0C 00 10 00 01 04 00 00 64 00 00 A8 04 00",
            "50 00 00 FF FF 03 00 0E 00 10 00 01 14 00 00 64 00 00 A8 01 00 34 12",
            "50 00 00 FF FF 03 00 0C 00 10 00 01 04 01 00 64 00 00 90 03 00",
            "50 00 00 FF FF 03 00 0E 00 10 00 01 14 01 00 64 00 00 90 03 00 10 10",
        ]
    };
    requests
        .into_iter()
        .zip([
            mc_reply(&binary("78 56 34 12 EF CD AB 90")),
            mc_reply(&[]),
            mc_reply(&[0x10, 0x10]),
            mc_reply(&[]),
        ])
        .map(|(request, reply)| (binary(request), reply))
        .collect()
}

/// ASCII 长度是字符数，十进制 D/M 地址不转换成十六进制文本。
fn ascii_steps() -> Vec<Step> {
    [
        (
            "500000FF03FF000018001004010000D*0001000004",
            "D00000FF03FF000014000056781234CDEF90AB",
        ),
        (
            "500000FF03FF00001C001014010000D*00010000011234",
            "D00000FF03FF0000040000",
        ),
        (
            "500000FF03FF000018001004010001M*0001000003",
            "D00000FF03FF0000070000101",
        ),
        (
            "500000FF03FF00001B001014010001M*0001000003101",
            "D00000FF03FF0000040000",
        ),
    ]
    .into_iter()
    .map(|(request, reply)| (request.as_bytes().to_vec(), reply.as_bytes().to_vec()))
    .collect()
}

/// A1E 二进制的设备代码在线上低字节在前，命令头只有一个字节。
fn a1e_steps() -> Vec<Step> {
    [
        (
            "01 FF 10 00 64 00 00 00 20 44 04 00",
            "81 00 78 56 34 12 EF CD AB 90",
        ),
        ("03 FF 10 00 64 00 00 00 20 44 01 00 34 12", "83 00"),
        ("00 FF 10 00 64 00 00 00 20 4D 03 00", "80 00 10 10"),
        ("02 FF 10 00 64 00 00 00 20 4D 03 00 10 10", "82 00"),
    ]
    .into_iter()
    .map(|(request, reply)| (binary(request), binary(reply)))
    .collect()
}

/// A1E ASCII 的设备代码先于地址；奇数个位补一个字符。
fn a1e_ascii_steps() -> Vec<Step> {
    [
        ("01FF00104420000000640400", "810056781234CDEF90AB"),
        ("03FF001044200000006401001234", "8300"),
        ("00FF00104D20000000640300", "80001010"),
        ("02FF00104D200000006403001010", "8200"),
    ]
    .into_iter()
    .map(|(request, reply)| (request.as_bytes().to_vec(), reply.as_bytes().to_vec()))
    .collect()
}

/// 同一泛型调用流程证明七种客户端兼容公共设备与读写 trait。
fn exercise<Client>(client: &mut Client)
where
    Client: DeviceBase + ReadBase<u32> + ReadBase<bool> + WriteBase<u16> + WriteBase<bool>,
{
    assert!(client.connect().to_result().unwrap());
    assert!(client.connect().to_result().unwrap());
    assert_eq!(
        &*ReadBase::<u32>::read(client, "D100", 2)
            .to_result()
            .unwrap(),
        &[0x12345678, 0x90abcdef]
    );
    assert_eq!(
        WriteBase::<u16>::write(client, "D100", 0x1234)
            .to_result()
            .unwrap(),
        0x1234
    );
    assert_eq!(
        &*ReadBase::<bool>::read(client, "M100", 3)
            .to_result()
            .unwrap(),
        &[true, false, true]
    );
    assert_eq!(
        WriteBase::<bool>::write_all(client, "M100", &[true, false, true])
            .to_result()
            .unwrap(),
        3
    );
    assert!(client.disconnect().to_result().unwrap());
    assert!(client.disconnect().to_result().unwrap());
}

macro_rules! protocol_test {
    ($test:ident, $client:ident, $server:ident, $steps:expr) => {
        #[test]
        fn $test() {
            let (port, worker) = $server($steps);
            let mut client =
                $client::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port, Timeout::default());
            exercise(&mut client);
            assert!(!client.is_connected());
            worker.join().unwrap();
        }
    };
}
protocol_test!(
    mc_binary_tcp,
    MelsecMcNet,
    tcp_script,
    standard_steps(false)
);
protocol_test!(mc_ascii_tcp, MelsecMcAsciiNet, tcp_script, ascii_steps());
protocol_test!(
    mc_binary_udp,
    MelsecMcUdp,
    udp_script,
    standard_steps(false)
);
protocol_test!(mc_ascii_udp, MelsecMcAsciiUdp, udp_script, ascii_steps());
protocol_test!(
    mc_r_binary_tcp,
    MelsecMcRNet,
    tcp_script,
    standard_steps(true)
);
protocol_test!(a1e_binary_tcp, MelsecA1ENet, tcp_script, a1e_steps());
protocol_test!(
    a1e_ascii_tcp,
    MelsecA1EAsciiNet,
    tcp_script,
    a1e_ascii_steps()
);

/// 数组超过单帧上限时按设备字地址前移，浮点/整数元素仍是连续数据。
#[test]
fn chunks_words_and_reports_partial_write_without_retry() {
    let mut first_read = binary("50 00 00 FF FF 03 00 0C 00 10 00 01 04 00 00 64 00 00 A8 C0 03");
    let second_read = binary("50 00 00 FF FF 03 00 0C 00 10 00 01 04 00 00 24 04 00 A8 02 00");
    let values: Vec<u32> = (0..481).map(|index| 0x12340000 + index).collect();
    let bytes: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    let mut first_write = first_read.clone();
    first_write[7..9].copy_from_slice(&1932_u16.to_le_bytes());
    first_write[12] = 0x14;
    first_write.extend_from_slice(&bytes[..1920]);
    let mut second_write = second_read.clone();
    second_write[7] = 16;
    second_write[12] = 0x14;
    second_write.extend_from_slice(&bytes[1920..]);
    let (port, worker) = tcp_script(vec![
        (std::mem::take(&mut first_read), mc_reply(&bytes[..1920])),
        (second_read, mc_reply(&bytes[1920..])),
        (first_write, mc_reply(&[])),
        (second_write, binary("D0 00 00 FF FF 03 00 02 00 51 C0")),
    ]);
    let mut client = MelsecMcNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    assert_eq!(
        &*client.read::<u32>("D100", 481).to_result().unwrap(),
        values.as_slice()
    );
    let result = client.write_all("D100", &values);
    assert!(!result.is_success);
    assert!(
        result.msg.contains("1920 acknowledged bytes"),
        "{}",
        result.msg
    );
    assert!(result.msg.contains("C051"));
    assert!(result.msg.contains("no automatic retry"));
    assert!(!client.is_connected());
    worker.join().unwrap();
}

/// 原始字节读取不跳过高字节；奇数长度写入保留原有末字高字节。
#[test]
fn packed_bytes_and_utf8_keep_neighboring_byte() {
    let (port, worker) = tcp_script(vec![
        (
            binary("50 00 00 FF FF 03 00 0C 00 10 00 01 04 00 00 64 00 00 A8 02 00"),
            mc_reply(&[1, 2, 3, 4]),
        ),
        (
            binary("50 00 00 FF FF 03 00 0C 00 10 00 01 04 00 00 65 00 00 A8 01 00"),
            mc_reply(&[3, 0xab]),
        ),
        (
            binary("50 00 00 FF FF 03 00 10 00 10 00 01 14 00 00 64 00 00 A8 02 00 05 06 07 AB"),
            mc_reply(&[]),
        ),
        (
            binary("50 00 00 FF FF 03 00 0C 00 10 00 01 04 00 00 C8 00 00 A8 01 00"),
            mc_reply(&[0, 0xee]),
        ),
        (
            binary("50 00 00 FF FF 03 00 0E 00 10 00 01 14 00 00 C8 00 00 A8 01 00 80 EE"),
            mc_reply(&[]),
        ),
        (
            binary(
                "50 00 00 FF FF 03 00 12 00 10 00 01 14 00 00 2C 01 00 A8 03 00 E4 BD A0 E5 A5 BD",
            ),
            mc_reply(&[]),
        ),
        (
            binary("50 00 00 FF FF 03 00 0C 00 10 00 01 04 00 00 2C 01 00 A8 03 00"),
            mc_reply("你好".as_bytes()),
        ),
        (
            binary("50 00 00 FF FF 03 00 0C 00 10 00 01 04 00 00 2C 01 00 A8 03 00"),
            mc_reply("你好".as_bytes()),
        ),
    ]);
    let mut client = MelsecNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    assert_eq!(
        &*client.read::<u8>("D100", 3).to_result().unwrap(),
        &[1, 2, 3]
    );
    assert_eq!(
        client.write_all("D100", &[5_u8, 6, 7]).to_result().unwrap(),
        3
    );
    assert_eq!(client.write("D200", -128_i8).to_result().unwrap(), -128);
    assert_eq!(
        client.write::<String>("D300", "你好").to_result().unwrap(),
        "你好"
    );
    assert_eq!(client.read_string("D300", 6).to_result().unwrap(), "你好");
    assert_eq!(
        &*client.read::<String>("D300", 6).to_result().unwrap(),
        &["你好"]
    );
    worker.join().unwrap();
}

/// A1E 先判断错误短帧，不能等待并不存在的成功数据区。
#[test]
fn a1e_short_errors_and_extended_errors_close_connection() {
    for reply in [binary("81 50"), binary("81 5B 12")] {
        let (port, worker) =
            tcp_script(vec![(binary("01 FF 10 00 64 00 00 00 20 44 01 00"), reply)]);
        let mut client = MelsecA1ENet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
        client.connect().to_result().unwrap();
        let result = client.read::<u16>("D100", 1);
        assert!(!result.is_success);
        assert!(result.msg.contains("end code"), "{}", result.msg);
        assert!(!client.is_connected());
        worker.join().unwrap();
    }
    for reply in [b"8150".to_vec(), b"815B12".to_vec()] {
        let (port, worker) = tcp_script(vec![(b"01FF00104420000000640100".to_vec(), reply)]);
        let mut client =
            MelsecA1EAsciiNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
        client.connect().to_result().unwrap();
        let result = client.read::<u16>("D100", 1);
        assert!(!result.is_success);
        assert!(result.msg.contains("end code"), "{}", result.msg);
        assert!(!client.is_connected());
        worker.join().unwrap();
    }
}

/// UDP 的短帧、错误长度、路由和非 0/1 位值均不能触发越界或被静默接受。
#[test]
fn malformed_udp_frames_close_connection() {
    let request = binary("50 00 00 FF FF 03 00 0C 00 10 00 01 04 01 00 64 00 00 90 03 00");
    let mut wrong_route = mc_reply(&[0x10, 0x10]);
    wrong_route[2] = 1;
    for reply in [
        vec![],
        vec![0xd0],
        binary("D0 00 00 FF FF 03 00 00 00"),
        mc_reply(&[0x20, 0x10]),
        mc_reply(&[0x10]),
        wrong_route,
    ] {
        let (port, worker) = udp_script(vec![(request.clone(), reply)]);
        let mut client = MelsecMcUdp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
        client.connect().to_result().unwrap();
        assert!(!client.read::<bool>("M100", 3).is_success);
        assert!(!client.is_connected());
        worker.join().unwrap();
    }
    for reply in [
        b"D00000FF03FF000000".to_vec(),
        b"D00000FF03FF000004ZZZZ".to_vec(),
        b"D00000FF03FF0000070000102".to_vec(),
    ] {
        let (port, worker) = udp_script(vec![(
            b"500000FF03FF000018001004010001M*0001000003".to_vec(),
            reply,
        )]);
        let mut client =
            MelsecMcAsciiUdp::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
        client.connect().to_result().unwrap();
        assert!(!client.read::<bool>("M100", 3).is_success);
        assert!(!client.is_connected());
        worker.join().unwrap();
    }
}

/// 验证整个范围后才发包，非法第二块地址不能导致第一次写入已执行。
#[test]
fn rejects_invalid_inputs_before_network_io() {
    let (port, worker) = tcp_script(vec![]);
    let mut client = MelsecMcNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    assert!(!client.read::<u16>("D0", 1).is_success);
    client.connect().to_result().unwrap();
    for text in [
        "",
        "D",
        "D-1",
        "D+1",
        "D1.0",
        "D16777216",
        "XX1",
        "XGG",
        "D0X10",
    ] {
        assert!(!client.read::<u16>(text, 1).is_success, "{text}");
    }
    assert!(!client.read::<u16>("D0", 0).is_success);
    assert!(!client.read::<u64>("D0", usize::MAX).is_success);
    assert!(!client.read::<bool>("D100", 1).is_success);
    assert!(!client.write_all("D16777215", &[1_u16, 2]).is_success);
    assert!(!client.write_all::<u16>("D0", &[]).is_success);
    assert!(!client.write("M16777215", 1_u16).is_success);
    assert!(client.is_connected());
    worker.join().unwrap();
    for timeout in [Timeout::new(0, 100), Timeout::new(100, -1)] {
        let mut client = MelsecNet::new(Ipv4Addr::LOCALHOST.into(), port, timeout);
        assert!(!client.connect().is_success);
        assert!(!client.is_connected());
    }
}

/// 超时后不保留可能混入迟到应答的旧连接。
#[test]
fn tcp_and_udp_timeout_invalidate_connection() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 21];
        stream.read_exact(&mut request).unwrap();
        thread::sleep(Duration::from_millis(350));
    });
    let mut client = MelsecNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::new(2000, 50));
    client.connect().to_result().unwrap();
    assert!(!client.read::<u16>("D0", 1).is_success);
    assert!(!client.is_connected());
    worker.join().unwrap();
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let mut client = MelsecMcUdp::new(
        Ipv4Addr::LOCALHOST.into(),
        socket.local_addr().unwrap().port(),
        Timeout::new(2000, 50),
    );
    client.connect().to_result().unwrap();
    assert!(!client.read::<u16>("D0", 1).is_success);
    assert!(!client.is_connected());
}

/// A1E 的 256 点编码为零，第二块地址按实际字数推进。
#[test]
fn a1e_word_limit_uses_zero_count_and_advances_address() {
    let bytes: Vec<u8> = (0..257_u16).flat_map(u16::to_le_bytes).collect();
    let first = binary("01 FF 10 00 00 00 00 00 20 44 00 00");
    let second = binary("01 FF 10 00 00 01 00 00 20 44 01 00");
    let mut reply_first = vec![0x81, 0];
    reply_first.extend_from_slice(&bytes[..512]);
    let mut reply_second = vec![0x81, 0];
    reply_second.extend_from_slice(&bytes[512..]);
    let mut write_first = first.clone();
    write_first[0] = 3;
    write_first.extend_from_slice(&bytes[..512]);
    let mut write_second = second.clone();
    write_second[0] = 3;
    write_second.extend_from_slice(&bytes[512..]);
    let (port, worker) = tcp_script(vec![
        (first, reply_first),
        (second, reply_second),
        (write_first, vec![0x83, 0]),
        (write_second, vec![0x83, 0]),
    ]);
    let mut client = MelsecA1ENet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    let values = client.read::<u16>("D0", 257).to_result().unwrap();
    assert_eq!(values.as_ref(), (0..257).collect::<Vec<u16>>());
    assert_eq!(client.write_all("D0", &values).to_result().unwrap(), 257);
    worker.join().unwrap();
}

/// MC ASCII 位命令单块上限是 3584，第二块必须按位数而不是字节数推进。
#[test]
fn ascii_bits_split_at_3584_and_preserve_odd_tail() {
    let first_read = b"500000FF03FF000018001004010001M*0000000E00".to_vec();
    let second_read = b"500000FF03FF000018001004010001M*0035840001".to_vec();
    let mut reply_first = b"D00000FF03FF000E040000".to_vec();
    reply_first.extend(std::iter::repeat_n(b'1', 3584));
    let mut first_write = b"500000FF03FF000E18001014010001M*0000000E00".to_vec();
    first_write.extend(std::iter::repeat_n(b'1', 3584));
    let second_write = b"500000FF03FF000019001014010001M*00358400011".to_vec();
    let (port, worker) = tcp_script(vec![
        (first_read, reply_first),
        (second_read, b"D00000FF03FF00000500001".to_vec()),
        (first_write, b"D00000FF03FF0000040000".to_vec()),
        (second_write, b"D00000FF03FF0000040000".to_vec()),
    ]);
    let mut client = MelsecMcAsciiNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    assert!(
        client
            .read::<bool>("M0", 3585)
            .to_result()
            .unwrap()
            .iter()
            .all(|value| *value)
    );
    assert_eq!(
        client
            .write_all("M0", &vec![true; 3585])
            .to_result()
            .unwrap(),
        3585
    );
    worker.join().unwrap();
}

/// 二进制位命令单块 7168 点、每字节两点，不误用每字节八点的位图。
#[test]
fn binary_bits_split_at_7168() {
    let mut first_write = binary("50 00 00 FF FF 03 00 0C 0E 10 00 01 14 01 00 00 00 00 90 00 1C");
    first_write.extend(std::iter::repeat_n(0x11, 3584));
    let (port, worker) = tcp_script(vec![
        (first_write, mc_reply(&[])),
        (
            binary("50 00 00 FF FF 03 00 0D 00 10 00 01 14 01 00 00 1C 00 90 01 00 10"),
            mc_reply(&[]),
        ),
    ]);
    let mut client = MelsecMcNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    assert_eq!(
        client
            .write_all("M0", &vec![true; 7169])
            .to_result()
            .unwrap(),
        7169
    );
    worker.join().unwrap();
}

/// iQ-R 的第 4 个地址字节不能截断；普通 3E 对相同地址必须拒绝。
#[test]
fn r_series_supports_32_bit_device_addresses_and_prevalidates_end() {
    let (port, worker) = tcp_script(vec![
        (
            binary("50 00 00 FF FF 03 00 0E 00 10 00 01 04 02 00 00 00 00 01 A8 00 01 00"),
            mc_reply(&[0x34, 0x12]),
        ),
        (
            binary("50 00 00 FF FF 03 00 10 00 10 00 01 14 02 00 00 00 00 01 A8 00 01 00 78 56"),
            mc_reply(&[]),
        ),
        (
            binary("50 00 00 FF FF 03 00 0E 00 10 00 01 04 02 00 FF FF FF FF A8 00 01 00"),
            mc_reply(&[0x34, 0x12]),
        ),
    ]);
    let mut client = MelsecMcRNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    assert!(!client.read::<u16>("D4294967295", 2).is_success);
    assert!(!client.write_all("D4294967295", &[1_u16, 2]).is_success);
    assert_eq!(
        &*client.read::<u16>("D16777216", 1).to_result().unwrap(),
        &[0x1234]
    );
    assert_eq!(
        client.write("D16777216", 0x5678_u16).to_result().unwrap(),
        0x5678
    );
    assert_eq!(
        &*client.read::<u16>("D4294967295", 1).to_result().unwrap(),
        &[0x1234]
    );
    worker.join().unwrap();
}

/// 路由与监视定时器进入请求；十六进制设备数制和十进制 D 地址严格区分。
#[test]
fn ascii_route_timer_and_hex_address_are_encoded_correctly() {
    let (port, worker) = tcp_script(vec![
        (
            b"500002030456070018002004010000SW00001A0001".to_vec(),
            b"D0000203045607000800001234".to_vec(),
        ),
        (
            b"500002030456070018002004010000D*0000260001".to_vec(),
            b"D0000203045607000800005678".to_vec(),
        ),
    ]);
    let mut client = MelsecMcAsciiNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default())
        .with_route(2, 3, 0x456, 7)
        .with_monitoring_timer(0x20);
    client.connect().to_result().unwrap();
    assert!(!client.read::<u16>("D999999", 2).is_success);
    assert_eq!(
        &*client.read::<u16>("sw1a", 1).to_result().unwrap(),
        &[0x1234]
    );
    assert_eq!(
        &*client.read::<u16>("D26", 1).to_result().unwrap(),
        &[0x5678]
    );
    worker.join().unwrap();
}

/// MC Q/L 位设备按字读取可以不对齐，A1E 则必须事先校验 16 点边界。
#[test]
fn mc_word_access_to_unaligned_bit_device_is_supported() {
    let (port, worker) = tcp_script(vec![(
        binary("50 00 00 FF FF 03 00 0C 00 10 00 01 04 00 00 64 00 00 90 01 00"),
        mc_reply(&[0x34, 0x12]),
    )]);
    let mut client = MelsecMcNet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    client.connect().to_result().unwrap();
    assert_eq!(
        &*client.read::<u16>("M100", 1).to_result().unwrap(),
        &[0x1234]
    );
    worker.join().unwrap();
    let mut a1e = MelsecA1ENet::new(Ipv4Addr::LOCALHOST.into(), port, Timeout::default());
    let result = a1e.read::<u16>("M100", 1);
    assert!(!result.is_success);
    assert!(result.msg.contains("aligned"));
}

/// 所有公开数值类型使用标准小端序，浮点数保持 IEEE 位模式。
#[test]
fn scalar_conversions_and_invalid_custom_types() {
    fn round_trip<T: MelsecValue + PartialEq + std::fmt::Debug>(value: T) {
        let bytes = value.to_le_bytes();
        assert_eq!(bytes.len(), T::BYTE_LEN);
        assert_eq!(T::from_le_bytes(&bytes).unwrap(), value);
        assert!(T::from_le_bytes(&[]).is_err());
    }
    round_trip(0xab_u8);
    round_trip(-12_i8);
    round_trip(0xcdef_u16);
    round_trip(-123_i16);
    round_trip(0x89abcdef_u32);
    round_trip(-123456_i32);
    round_trip(u64::MAX);
    round_trip(i64::MIN);
    round_trip(1.25_f32);
    round_trip(-1.25_f64);
    round_trip(true);
    round_trip(false);
    assert_eq!(<f32 as MelsecValue>::to_le_bytes(&1.0), [0, 0, 0x80, 0x3f]);
    assert!(<bool as MelsecValue>::from_le_bytes(&[2]).is_err());
    assert!(<String as MelsecReadValue>::from_le_bytes(&[0xff]).is_err());
    struct Zero;
    impl MelsecValue for Zero {
        const BYTE_LEN: usize = 0;
        fn from_le_bytes(_: &[u8]) -> Result<Self, String> {
            Ok(Self)
        }
        fn to_le_bytes(&self) -> Vec<u8> {
            Vec::new()
        }
    }
    let mut client = MelsecNet::new(Ipv4Addr::LOCALHOST.into(), 6000, Timeout::default());
    assert!(!client.read::<Zero>("D0", 1).is_success);
    assert!(!client.write("D0", Zero).is_success);
    assert!(!client.write_all("D0", &[Zero]).is_success);
}
