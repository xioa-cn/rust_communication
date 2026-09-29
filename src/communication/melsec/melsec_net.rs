//! 三菱同步通讯客户端；七种协议共用连接管理和泛型读写流程。
//! 设备地址、帧编解码、值转换分别位于独立模块，不自动重试写入。

use super::melsec_address::Address;
use super::melsec_packet::{self as packet, Protocol, Route};
pub use super::melsec_value::{MelsecReadValue, MelsecValue, MelsecWriteInput, MelsecWriteValue};
use crate::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use crate::communication::timeout::Timeout;
use crate::entity::operate::Operator;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

/// 连接保存在实例中；可变借用保证一个实例上仅串行执行请求。
enum Connection {
    Tcp(TcpStream),
    Udp(UdpSocket),
}

/// 七种公开客户端的共用实现；协议种类构造后不变。
struct MelsecClient {
    address: IpAddr,
    port: u16,
    timeout: Timeout,
    protocol: Protocol,
    udp: bool,
    route: Route,
    connection: Option<Connection>,
}

impl MelsecClient {
    /// 只保存配置，不在构造时访问网络。
    fn new(address: IpAddr, port: u16, timeout: Timeout, protocol: Protocol, udp: bool) -> Self {
        Self {
            address,
            port,
            timeout,
            protocol,
            udp,
            route: Route::default(),
            connection: None,
        }
    }

    /// 校验超时并创建 TCP 或已绑定目标的 UDP Socket；失败不保留半连接。
    fn connect(&mut self) -> Result<bool, String> {
        if self.connection.is_some() {
            return Ok(true);
        }
        let connect_timeout = duration(self.timeout.connect_time_out())?;
        let io_timeout = duration(self.timeout.receive_time_out())?;
        if self.port == 0 {
            return Err("MELSEC port must be greater than zero".into());
        }
        if self.udp {
            let local = match self.address {
                IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            };
            let socket = UdpSocket::bind(SocketAddr::new(local, 0)).map_err(io_error)?;
            socket
                .connect(SocketAddr::new(self.address, self.port))
                .map_err(io_error)?;
            socket
                .set_read_timeout(Some(io_timeout))
                .map_err(io_error)?;
            socket
                .set_write_timeout(Some(io_timeout))
                .map_err(io_error)?;
            self.connection = Some(Connection::Udp(socket));
        } else {
            let socket = TcpStream::connect_timeout(
                &SocketAddr::new(self.address, self.port),
                connect_timeout,
            )
            .map_err(io_error)?;
            socket
                .set_read_timeout(Some(io_timeout))
                .map_err(io_error)?;
            socket
                .set_write_timeout(Some(io_timeout))
                .map_err(io_error)?;
            socket.set_nodelay(true).map_err(io_error)?;
            self.connection = Some(Connection::Tcp(socket));
        }
        Ok(true)
    }

    /// 读取定长元素；String 的 length 表示原始 UTF-8 字节数，不含任何长度头。
    fn read<T: MelsecReadValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Result<Box<[T]>, String> {
        let byte_length = if T::IS_STRING {
            length
        } else {
            value_length(T::BYTE_LEN, T::IS_BIT, length)?
        };
        let address = Address::parse(address, self.protocol)?;
        let bytes = self.read_data(address, byte_length, T::IS_BIT)?;
        if T::IS_STRING {
            return Ok(vec![T::from_le_bytes(&bytes)?].into_boxed_slice());
        }
        let mut values = Vec::new();
        values
            .try_reserve_exact(length)
            .map_err(|_| "MELSEC read allocation failed")?;
        for chunk in bytes.chunks(T::BYTE_LEN) {
            values.push(T::from_le_bytes(chunk)?);
        }
        Ok(values.into_boxed_slice())
    }

    /// 单值写入沿用泛型编码；字符串为无头 UTF-8，不自动追加结束符。
    fn write<T: MelsecWriteValue>(&mut self, address: &str, value: &T) -> Result<(), String> {
        let bytes = value.encode();
        if !T::IS_STRING && bytes.len() != value_length(T::BYTE_LEN, T::IS_BIT, 1)? {
            return Err("Invalid MELSEC value encoding length".into());
        }
        self.write_data(address, bytes, T::IS_BIT)
    }

    /// 一次性验证并编码整个数组，避免后续元素编码异常造成部分写入。
    fn write_all<T: MelsecValue>(&mut self, address: &str, values: &[T]) -> Result<usize, String> {
        let byte_length = value_length(T::BYTE_LEN, T::IS_BIT, values.len())?;
        let parsed = Address::parse(address, self.protocol)?;
        self.validate(parsed, byte_length, T::IS_BIT)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(byte_length)
            .map_err(|_| "MELSEC write allocation failed")?;
        for value in values {
            let encoded = value.to_le_bytes();
            if encoded.len() != T::BYTE_LEN {
                return Err("Invalid MELSEC array element encoding length".into());
            }
            bytes.extend_from_slice(&encoded);
        }
        self.write_data(address, bytes, T::IS_BIT)?;
        Ok(values.len())
    }

    /// 在第一次 I/O 前验证完整范围；数值按字访问，bool 只能访问位设备。
    fn validate(&self, address: Address, byte_length: usize, bit: bool) -> Result<usize, String> {
        if bit && !address.bit_device {
            return Err("Use a bit device for MELSEC bool access".into());
        }
        let points = if bit {
            byte_length
        } else {
            byte_length.div_ceil(2)
        };
        address.validate(points, bit, self.protocol)?;
        if self.connection.is_none() {
            return Err("MELSEC is not connected; call connect() first".into());
        }
        Ok(points)
    }

    /// 分块读取后拼成连续字节流，因此跨块的 u32/u64/浮点元素仍正确解码。
    fn read_data(
        &mut self,
        address: Address,
        byte_length: usize,
        bit: bool,
    ) -> Result<Vec<u8>, String> {
        let points = self.validate(address, byte_length, bit)?;
        let total_bytes = points
            .checked_mul(if bit { 1 } else { 2 })
            .ok_or("MELSEC read length overflow")?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total_bytes)
            .map_err(|_| "MELSEC read allocation failed")?;
        let maximum = self.protocol.max_points(address, bit, false);
        let mut done = 0;
        while done < points {
            let count = (points - done).min(maximum);
            bytes.extend_from_slice(&self.exchange(
                address.advanced(done, bit),
                bit,
                count,
                None,
            )?);
            done += count;
        }
        bytes.truncate(byte_length);
        Ok(bytes)
    }

    /// 分块写入；奇数字节通过读改写保留最后一个字的高字节，不用零覆盖邻接数据。
    /// 读改写及多个批次均不保证原子性，PLC 同时修改同一字时由调用方同步。
    fn write_data(&mut self, text: &str, mut bytes: Vec<u8>, bit: bool) -> Result<(), String> {
        let address = Address::parse(text, self.protocol)?;
        let points = self.validate(address, bytes.len(), bit)?;
        if bit && bytes.iter().any(|value| *value > 1) {
            return Err("Invalid MELSEC boolean encoding".into());
        }
        if !bit && !bytes.len().is_multiple_of(2) {
            // 提前读取末尾字，再执行任何写入；失败不会留下前面已写、尾部未读的状态。
            let tail = self.exchange(address.advanced(points - 1, false), false, 1, None)?;
            bytes.push(tail[1]);
        }
        let maximum = self.protocol.max_points(address, bit, true);
        let width = if bit { 1 } else { 2 };
        let mut done = 0;
        while done < points {
            let count = (points - done).min(maximum);
            let payload = &bytes[done * width..(done + count) * width];
            if let Err(error) =
                self.exchange(address.advanced(done, bit), bit, count, Some(payload))
            {
                // 响应丢失无法证明 PLC 未执行；显式报告已确认部分，绝不自动重发。
                let unit = if bit { "bits" } else { "bytes" };
                return Err(format!(
                    "Write failed after {} acknowledged {unit}: {error}; the current chunk may have been written; no automatic retry",
                    done * width
                ));
            }
            done += count;
        }
        Ok(())
    }

    /// 单次请求与响应交换；网络、结构或 PLC 错误后关闭连接，避免复用错位数据。
    fn exchange(
        &mut self,
        address: Address,
        bit: bool,
        points: usize,
        data: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        let request = packet::request(self.protocol, self.route, address, bit, points, data)?;
        let result = (|| {
            let connection = self.connection.as_mut().ok_or("MELSEC is not connected")?;
            let response = receive(
                connection,
                self.protocol,
                self.route,
                &request,
                bit,
                points,
                data.is_some(),
            )?;
            packet::response(
                self.protocol,
                self.route,
                &response,
                bit,
                points,
                data.is_some(),
            )
        })();
        if result.is_err() {
            self.close();
        }
        result
    }

    /// 先取走连接再关闭，保证关闭失败也不会继续复用旧 Socket。
    fn close(&mut self) {
        if let Some(Connection::Tcp(stream)) = self.connection.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

/// TCP 先读固定头再读精确长度，支持拆包；UDP 一次接收完整数据报。
fn receive(
    connection: &mut Connection,
    protocol: Protocol,
    route: Route,
    request: &[u8],
    bit: bool,
    points: usize,
    write: bool,
) -> Result<Vec<u8>, String> {
    match connection {
        Connection::Udp(socket) => {
            if socket.send(request).map_err(io_error)? != request.len() {
                return Err("Incomplete MELSEC UDP send".into());
            }
            let mut response = vec![0; 65_535];
            let received = socket.recv(&mut response).map_err(io_error)?;
            response.truncate(received);
            Ok(response)
        }
        Connection::Tcp(stream) => {
            stream.write_all(request).map_err(io_error)?;
            let header_len = protocol.header_len();
            let mut response = vec![0; header_len];
            stream.read_exact(&mut response).map_err(io_error)?;
            let body_len = if protocol.is_a1e() {
                match packet::a1e_status(protocol, &response, bit, write)? {
                    0 => {
                        if write {
                            0
                        } else {
                            protocol.payload_len(bit, points)
                        }
                    }
                    // 5B 后跟一个异常代码字节（ASCII 为两个字符），没有成功数据区。
                    0x5b => {
                        if protocol.is_ascii() {
                            2
                        } else {
                            1
                        }
                    }
                    _ => 0,
                }
            } else {
                packet::mc_body_len(protocol, route, &response)?
            };
            response.resize(header_len + body_len, 0);
            stream
                .read_exact(&mut response[header_len..])
                .map_err(io_error)?;
            Ok(response)
        }
    }
}

/// 校验元素宽度和乘法溢出；禁止零宽度的自定义定长类型造成除零或空块循环。
fn value_length(width: usize, bit: bool, count: usize) -> Result<usize, String> {
    if width == 0 || count == 0 || (bit && width != 1) {
        return Err("Invalid MELSEC element width or zero length".into());
    }
    width
        .checked_mul(count)
        .ok_or_else(|| "MELSEC value length overflow".into())
}

/// 统一使用毫秒配置，拒绝系统 Socket 不接受的零超时。
fn duration(milliseconds: i32) -> Result<Duration, String> {
    if milliseconds <= 0 {
        return Err("MELSEC timeout must be greater than zero".into());
    }
    Ok(Duration::from_millis(milliseconds as u64))
}

/// 给底层 I/O 错误加上协议上下文，保留系统错误内容。
fn io_error(error: std::io::Error) -> String {
    format!("MELSEC socket error: {error}")
}

/// 内部使用 Result，外部统一采用项目既有的 Operator 返回类型。
fn as_operator<T>(result: Result<T, String>) -> Operator<T> {
    match result {
        Ok(value) => Operator::ok(value),
        Err(error) => Operator::err(&error),
    }
}

/// 为不同帧格式生成相同公共 API，避免协议间方法签名或返回类型漂移。
macro_rules! define_client {
    ($name:ident, $protocol:expr, $udp:expr, $description:literal) => {
        #[doc = $description]
        pub struct $name {
            client: MelsecClient,
        }
        impl $name {
            /// 创建客户端，不立即连接；端口必须匹配 PLC 配置，超时单位为毫秒。
            pub fn new(address: IpAddr, port: u16, timeout: Timeout) -> Self {
                Self {
                    client: MelsecClient::new(address, port, timeout, $protocol, $udp),
                }
            }
            /// 设置网络号、PC 号、目标 I/O 号和站号；已有连接会关闭。
            /// A1E 不支持跨网路由，只使用 PC 号，其他三个字段忽略。
            pub fn with_route(
                mut self,
                network_number: u8,
                pc_number: u8,
                io_number: u16,
                station_number: u8,
            ) -> Self {
                self.client.close();
                self.client.route.network = network_number;
                self.client.route.pc = pc_number;
                self.client.route.io = io_number;
                self.client.route.station = station_number;
                self
            }
            /// 设置 PLC 监视定时器，每单位 250ms；零表示 PLC 无限等待，Socket 超时仍生效。
            pub fn with_monitoring_timer(mut self, monitoring_timer: u16) -> Self {
                self.client.route.timer = monitoring_timer;
                self
            }
            /// 返回本地 Socket 状态，不主动探测 PLC；UDP 的连接成功不表示远端在线。
            pub fn is_connected(&self) -> bool {
                self.client.connection.is_some()
            }
            /// 显式建立连接；已连接时直接成功，不自动执行 PLC 握手或读写。
            pub fn connect(&mut self) -> Operator<bool> {
                as_operator(self.client.connect())
            }
            /// 关闭连接，可重复调用；实例释放时也会释放 Socket。
            pub fn disconnect(&mut self) -> Operator<bool> {
                self.client.close();
                Operator::ok(true)
            }
            /// 读取 length 个数值/bool，返回 Box<[T]>；u8/i8 连续打包，不按一个字一个字节读取。
            /// String 的 length 为原始字节数，返回仅含一个 UTF-8 字符串的数组。
            pub fn read<T: MelsecReadValue>(
                &mut self,
                address: &str,
                length: usize,
            ) -> Operator<Box<[T]>> {
                as_operator(self.client.read(address, length))
            }
            /// 按原始字节长度读取 UTF-8 文本，不裁剪零字节、不解析或跳过长度头。
            pub fn read_string(&mut self, address: &str, byte_length: usize) -> Operator<String> {
                as_operator(
                    self.client
                        .read::<String>(address, byte_length)
                        .map(|values| values.into_vec().remove(0)),
                )
            }
            /// 写入单值并返回原值；支持 write::<String>(address, &str)。
            /// 奇数字节数先读取末字高字节并保留；字符串不含长度头和结束符，调用方负责容量。
            pub fn write<T: MelsecWriteValue>(
                &mut self,
                address: &str,
                value: impl MelsecWriteInput<T>,
            ) -> Operator<T> {
                let value = value.into_value();
                as_operator(self.client.write(address, &value).map(|_| value))
            }
            /// 借用数组或切片写入，成功返回元素个数；不自动重试，失败不回滚已确认的块。
            /// 不支持字符串数组；多字符串需调用方明确每段地址和存储容量。
            pub fn write_all<T: MelsecValue>(
                &mut self,
                address: &str,
                values: &[T],
            ) -> Operator<usize> {
                as_operator(self.client.write_all(address, values))
            }
        }
        impl DeviceBase for $name {
            /// 通用设备连接入口。
            fn connect(&mut self) -> Operator<bool> {
                Self::connect(self)
            }
            /// 通用设备断开入口。
            fn disconnect(&mut self) -> Operator<bool> {
                Self::disconnect(self)
            }
        }
        impl<T: MelsecReadValue> ReadBase<T> for $name {
            /// 通用泛型读取入口，行为与固有方法一致。
            fn read(&mut self, address: &str, length: usize) -> Operator<Box<[T]>> {
                Self::read(self, address, length)
            }
        }
        impl<T: MelsecValue> WriteBase<T> for $name {
            /// 通用单值写入入口。
            fn write(&mut self, address: &str, value: T) -> Operator<T> {
                Self::write(self, address, value)
            }
            /// 通用数组写入入口。
            fn write_all(&mut self, address: &str, values: &[T]) -> Operator<usize> {
                Self::write_all(self, address, values)
            }
        }
    };
}

define_client!(
    MelsecNet,
    Protocol::McBinary,
    false,
    "MC 3E 二进制 TCP 客户端。"
);
define_client!(
    MelsecMcAsciiNet,
    Protocol::McAscii,
    false,
    "MC 3E ASCII TCP 客户端。"
);
define_client!(
    MelsecMcUdp,
    Protocol::McBinary,
    true,
    "MC 3E 二进制 UDP 客户端。"
);
define_client!(
    MelsecMcAsciiUdp,
    Protocol::McAscii,
    true,
    "MC 3E ASCII UDP 客户端。"
);
define_client!(
    MelsecA1ENet,
    Protocol::A1EBinary,
    false,
    "A1E 二进制 TCP 客户端。"
);
define_client!(
    MelsecA1EAsciiNet,
    Protocol::A1EAscii,
    false,
    "A1E ASCII TCP 客户端。"
);
define_client!(
    MelsecMcRNet,
    Protocol::McRBinary,
    false,
    "MC R 二进制 TCP 客户端，使用 iQ-R 扩展设备指令。"
);

/// MC 二进制 TCP 的标准命名，与 MelsecNet 是同一类型。
pub type MelsecMcNet = MelsecNet;
