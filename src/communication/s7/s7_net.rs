//! S7 会话管理与读写流程；协议细节由 address、packet、transport、value 模块承担。

use std::io::BufReader;
use std::net::{IpAddr, Shutdown, SocketAddr, TcpStream};
use std::time::Duration;

use crate::communication::device_base::{DeviceBase, ReadBase, WriteBase};
use crate::communication::timeout::{as_operator, Timeout};
use crate::entity::operate::Operator;

use super::s7_address::S7Address;
use super::s7_packet::{self as packet, READ_OVERHEAD, REQUESTED_PDU, Response, WRITE_OVERHEAD};
use super::s7_transport::{io_error, receive_data, receive_tpkt, send_tpkt};
use super::s7_type::S7Type;
use super::s7_value::{S7ReadValue, S7Value, S7WriteInput, S7WriteValue};

/// 基于标准库 TCP Socket 的同步客户端，同一连接上的请求串行执行。
/// 不自动重试写入，避免超时后重复修改 PLC 数据。
pub struct S7Net {
    address: IpAddr,
    port: u16,
    s7_type: S7Type,
    timeout: Timeout,
    rack: usize,
    slot: usize,
    /// 显式 TSAP 优先于型号默认值，适用于以太网模块和自定义连接组态。
    tsap: Option<(u16, u16)>,
    stream: Option<BufReader<TcpStream>>,
    /// 不包含 TPKT/COTP 头部的协商后 PDU 长度。
    pdu_length: usize,
    /// 请求与响应配对的序列号；由可变借用保证串行递增。
    sequence: u16,
}

impl S7Net {
    /// 创建客户端，不立即连接。超时单位为毫秒，rack 范围 0..7，slot 范围 0..31。
    /// 参数在 connect 时校验，以 Operator 统一返回错误。
    pub fn new(
        address: IpAddr,
        port: u16,
        s7_type: S7Type,
        timeout: Timeout,
        rack: usize,
        slot: usize,
    ) -> Self {
        Self {
            address,
            port,
            s7_type,
            timeout,
            rack,
            slot,
            tsap: None,
            stream: None,
            pdu_length: REQUESTED_PDU,
            sequence: 0,
        }
    }

    /// 设置本地/远端 TSAP；已有连接会关闭，下次 connect 时使用新参数。
    /// 不改变调用方提供的 rack/slot，两个 TSAP 值必须与 PLC 项目组态一致。
    pub fn with_tsap(mut self, local: u16, remote: u16) -> Self {
        self.close_socket();
        self.tsap = Some((local, remote));
        self
    }

    /// 返回配置的 PLC 系列；TSAP 默认值和 STRING 头布局按型号区分。
    pub fn s7_type(&self) -> S7Type {
        self.s7_type
    }

    /// 返回本地连接状态，不主动探测远端；实际断线在下次 I/O 时发现。
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// 连接完成后返回实际协商的 PDU 长度，断开时返回 None。
    pub fn negotiated_pdu_length(&self) -> Option<usize> {
        self.stream.as_ref().map(|_| self.pdu_length)
    }

    /// 依次完成 TCP 连接、COTP 握手及 S7 PDU 协商；已连接时直接成功。
    pub fn connect(&mut self) -> Operator<bool> {
        as_operator(self.connect_inner())
    }

    /// 关闭本地 Socket，可重复调用。实例释放时 Socket 也会自动释放。
    pub fn disconnect(&mut self) -> Operator<bool> {
        self.close_socket();
        Operator::ok(true)
    }

    /// 数值及 bool 连续读取 length 个 T，返回拥有所有权的堆数组，而不是 Vec。
    /// 例如 read::<u16>("DB1.0", 3) 读取 6 字节并返回 3 个 u16。
    /// bool 按连续位读取，可跨字节；多个请求组成的读取不是原子快照。
    /// bool 支持简写：DB1.100、M100 默认从该字节的第 0 位开始。
    /// String 的 length 表示原始字节数，返回只含一个字符串的数组，不解析 S7 长度头。
    pub fn read<T: S7ReadValue>(&mut self, address: &str, length: usize) -> Operator<Box<[T]>> {
        as_operator(self.read_inner(address, length))
    }

    /// 通用字符串读取：从 address 开始读取 byte_length 字节并按 UTF-8 解码。
    /// 不跳过长度头、不截断零字节；对应按指定长度读取原始文本的方式。
    pub fn read_string(&mut self, address: &str, byte_length: usize) -> Operator<String> {
        let result = (|| {
            let address = S7Address::parse(address, false)?;
            let bytes = self.read_bytes(&address, 0, byte_length, false)?;
            <String as S7ReadValue>::from_be_bytes(&bytes)
        })();
        as_operator(result)
    }

    /// 西门子 STRING 读取：address 指向长度头，内容长度由 PLC 自动给出。
    /// S200/SMART 使用一字节长度头，S300/S400/S1200/S1500 使用两字节头。
    /// 与 read_string 不同，不需要传入字节数；不用于 WSTRING。
    pub fn read_s7_string(&mut self, address: &str) -> Operator<String> {
        as_operator(self.read_s7_string_inner(address))
    }

    /// 自动读取一个带长度头的 STRING，不需要传长度，返回只含一个字符串的数组。
    /// 保留数组返回形式，调用方可通过 content[0] 访问结果。
    pub fn read_s7_strings(&mut self, address: &str) -> Operator<Box<[String]>> {
        as_operator(
            self.read_s7_string_inner(address)
                .map(|value| Box::<[String]>::from([value])),
        )
    }

    /// 写入一个值，成功后返回原值，无需传入长度。
    /// 位写入使用 S7 位传输，不先读取整字节，因此不会覆盖相邻位。
    /// write::<String>(address, "text") 按 S7 STRING 写入，address 必须指向长度头。
    /// 字符串采用 UTF-8；两字节头先读取容量，超过容量时返回错误。
    /// S200/SMART 无容量字段，只限制协议字节长度；调用方需确保 V 区已预留足够空间。
    pub fn write<T: S7WriteValue>(
        &mut self,
        address: &str,
        value: impl S7WriteInput<T>,
    ) -> Operator<T> {
        let value = value.into_value();
        as_operator(self.write_inner(address, &value).map(|_| value))
    }

    /// 连续写入数组或切片，返回成功写入的元素数；空数组返回错误。
    /// 例如 write_all("DB1.0", &[1_u16, 2, 3]) 写入 6 字节并返回 3。
    /// 分块写入不是原子操作；失败时已确认成功的部分不会自动回滚。
    pub fn write_all<T: S7Value>(&mut self, address: &str, array: &[T]) -> Operator<usize> {
        as_operator(self.write_all_inner(address, array))
    }

    /// 校验配置后建立会话，任一握手步骤失败都不保留半初始化连接。
    fn connect_inner(&mut self) -> Result<bool, String> {
        if self.is_connected() {
            return Ok(true);
        }
        if self.timeout.connect_time_out() <= 0 || self.timeout.receive_time_out() <= 0 {
            return Err("Timeout values must be positive milliseconds".into());
        }
        let default_tsap = self.s7_type.default_tsap(self.rack, self.slot)?;
        let (local_tsap, remote_tsap) = self.tsap.unwrap_or(default_tsap);
        let connect_timeout = Duration::from_millis(self.timeout.connect_time_out() as u64);
        let receive_timeout = Duration::from_millis(self.timeout.receive_time_out() as u64);
        let mut stream =
            TcpStream::connect_timeout(&SocketAddr::new(self.address, self.port), connect_timeout)
                .map_err(|error| format!("TCP connection failed: {error}"))?;
        // receive_time_out 同时限制单次 Socket 收发等待，不是整次分块操作的总期限。
        stream
            .set_read_timeout(Some(receive_timeout))
            .map_err(io_error)?;
        stream
            .set_write_timeout(Some(receive_timeout))
            .map_err(io_error)?;
        stream.set_nodelay(true).map_err(io_error)?;
        send_tpkt(
            &mut stream,
            &packet::connection_request(local_tsap, remote_tsap),
        )?;
        let mut stream = BufReader::with_capacity(REQUESTED_PDU + 7, stream);
        packet::validate_connection_confirm(&receive_tpkt(&mut stream, 260)?)?;

        self.pdu_length = REQUESTED_PDU;
        self.sequence = 0;
        self.stream = Some(stream);
        let result = (|| {
            let response = self.exchange(&packet::setup_parameters(), &[])?;
            self.pdu_length = packet::negotiated_pdu(&response)?;
            Ok(true)
        })();
        if result.is_err() {
            self.close_socket();
        }
        result
    }

    /// 将元素数换算为字节数，分块读取后再按元素宽度解码。
    /// 分块边界不必与元素边界对齐，完整接收后才进行类型转换。
    pub fn read_inner<T: S7ReadValue>(
        &mut self,
        address: &str,
        length: usize,
    ) -> Result<Box<[T]>, String> {
        let address = S7Address::parse(address, T::IS_BIT)?;
        if T::IS_STRING {
            let bytes = self.read_bytes(&address, 0, length, false)?;
            return Ok(Box::new([T::from_be_bytes(&bytes)?]));
        }
        if T::BYTE_LEN == 0 || (T::IS_BIT && T::BYTE_LEN != 1) {
            return Err("Invalid S7 element byte width".into());
        }
        let byte_length = length
            .checked_mul(T::BYTE_LEN)
            .ok_or("Element count exceeds the S7 address range")?;
        let bytes = self.read_bytes(&address, 0, byte_length, T::IS_BIT)?;
        bytes
            .chunks_exact(T::BYTE_LEN)
            .map(T::from_be_bytes)
            .collect()
    }

    /// 单独分派带头字符串读取，避免通用 String 读取误把正文当作长度头。
    pub fn read_s7_string_inner(&mut self, address: &str) -> Result<String, String> {
        let address = S7Address::parse(address, false)?;
        super::s7_string::read_s7_value(&address, self.s7_type, |offset, length| {
            self.read_bytes(&address, offset, length, false)
        })
    }

    /// 按偏移读取有效载荷，供定长类型及字符串头/内容共用分包和响应校验。
    /// offset 和 byte_length 对位操作按位计数，其余操作按字节计数。
    fn read_bytes(
        &mut self,
        address: &S7Address,
        offset: usize,
        byte_length: usize,
        is_bit: bool,
    ) -> Result<Vec<u8>, String> {
        if byte_length == 0 {
            return Err("Read length must be greater than zero".into());
        }
        let end = offset
            .checked_add(byte_length)
            .ok_or("Read exceeds the S7 address range")?;
        address.validate(end, is_bit)?;
        self.require_connection()?;
        let mut bytes = Vec::with_capacity(byte_length);
        while bytes.len() < byte_length {
            // 位数组逐位请求，兼容只支持单个位传输的 PLC；偏移量按位递增。
            let maximum = if is_bit {
                1
            } else {
                self.pdu_length - READ_OVERHEAD
            };
            let count = (byte_length - bytes.len()).min(maximum);
            let parameters =
                packet::variable_parameters(address, 0x04, offset + bytes.len(), count);
            let response = self.exchange(&parameters, &[])?;
            response.check_error()?;
            if response.parameters != [0x04, 1] || response.data.len() < 4 {
                return self.invalid_response("Invalid S7 Read Var response");
            }
            if response.data[0] != 0xff {
                return Err(packet::item_error(response.data[0]));
            }
            let transport = if is_bit { 0x03 } else { 0x04 };
            let bit_length = if is_bit { 1 } else { count * 8 };
            if response.data[1] != transport
                || usize::from(packet::read_u16(&response.data[2..4])) != bit_length
                || response.data.len() != count + 4
            {
                return self.invalid_response("S7 read payload type or length mismatch");
            }
            bytes.extend_from_slice(&response.data[4..]);
        }
        Ok(bytes)
    }

    /// 按协商 PDU 大小分块写入，只累计 PLC 已确认成功的字节数。
    pub fn write_inner<T: S7WriteValue>(&mut self, address: &str, value: &T) -> Result<(), String> {
        let address = S7Address::parse(address, T::IS_BIT)?;
        let bytes = value.encode();
        if T::IS_STRING {
            if bytes.len() > 254 {
                return Err("S7 STRING content exceeds 254 bytes; nothing was written".into());
            }
            let header_length = self.s7_type.string_header_length();
            let header = self.read_bytes(&address, 0, header_length, false)?;
            let data = super::s7_string::prepare_write(&address, self.s7_type, &header, &bytes)?;
            // 两字节头跳过最大容量；单字节头从实际长度开始写，不能错位一个字节。
            return self.write_bytes(&address, header_length - 1, &data, false);
        }
        if T::BYTE_LEN == 0 || (T::IS_BIT && T::BYTE_LEN != 1) {
            return Err("Invalid S7 element byte width".into());
        }
        if bytes.len() != T::BYTE_LEN {
            return Err("Encoded value byte length does not match element width".into());
        }
        self.write_bytes(&address, 0, &bytes, T::IS_BIT)
    }

    /// 先编码所有定长元素，再交给共用的写入流程，避免编码失败造成部分写入。
    pub fn write_all_inner<T: S7Value>(
        &mut self,
        address: &str,
        array: &[T],
    ) -> Result<usize, String> {
        if T::BYTE_LEN == 0 || (T::IS_BIT && T::BYTE_LEN != 1) {
            return Err("Invalid S7 element byte width".into());
        }
        let byte_length = array
            .len()
            .checked_mul(T::BYTE_LEN)
            .ok_or("Element count exceeds the S7 address range")?;
        let address = S7Address::parse(address, T::IS_BIT)?;
        address.validate(byte_length, T::IS_BIT)?;
        self.require_connection()?;
        // 先编码并检查所有元素，再开始发送，避免编码失败造成不必要的部分写入。
        let mut bytes = Vec::with_capacity(byte_length);
        for value in array {
            let offset = bytes.len();
            value.append_be_bytes(&mut bytes);
            if bytes.len().checked_sub(offset) != Some(T::BYTE_LEN) {
                return Err("Encoded value byte length does not match element width".into());
            }
        }
        self.write_bytes(&address, 0, &bytes, T::IS_BIT)?;
        Ok(array.len())
    }

    /// 共用的字节/位分块写入；offset 对普通数据按字节计数，对 bool 按位计数。
    /// 字符串长度头和正文分块写入不保证原子性，失败后不能自动重试。
    fn write_bytes(
        &mut self,
        address: &S7Address,
        offset: usize,
        bytes: &[u8],
        is_bit: bool,
    ) -> Result<(), String> {
        if bytes.is_empty() {
            return Err("Write length must be greater than zero".into());
        }
        let end = offset
            .checked_add(bytes.len())
            .ok_or("Write exceeds the S7 address range")?;
        address.validate(end, is_bit)?;
        self.require_connection()?;
        let byte_length = bytes.len();
        let mut written = 0;
        while written < byte_length {
            let maximum = if is_bit {
                1
            } else {
                self.pdu_length - WRITE_OVERHEAD
            };
            let count = (byte_length - written).min(maximum);
            let parameters = packet::variable_parameters(address, 0x05, offset + written, count);
            let data = packet::write_data(&bytes[written..written + count], is_bit);
            let result = (|| {
                let response = self.exchange(&parameters, &data)?;
                response.check_error()?;
                if response.parameters != [0x05, 1] || response.data.len() != 1 {
                    return self.invalid_response("Invalid S7 Write Var response");
                }
                if response.data[0] != 0xff {
                    return Err(packet::item_error(response.data[0]));
                }
                Ok(())
            })();
            if let Err(error) = result {
                // 响应丢失不能证明写入未执行，因此禁止透明重试当前块。
                let unit = if is_bit { "bits" } else { "bytes" };
                return Err(format!(
                    "Write failed after {written} acknowledged {unit}: {error}; \
                     the current chunk may have been written; no automatic retry"
                ));
            }
            written += count;
        }
        Ok(())
    }

    /// 发送一个 Job 并接收对应响应；网络或结构错误后关闭连接，防止错包串入下次请求。
    fn exchange(&mut self, parameters: &[u8], data: &[u8]) -> Result<Response, String> {
        self.require_connection()?;
        if 10 + parameters.len() + data.len() > self.pdu_length {
            return Err("Request exceeds negotiated S7 PDU length".into());
        }
        self.sequence = self.sequence.wrapping_add(1).max(1);
        let request = packet::job(self.sequence, parameters, data);
        let result = (|| {
            let stream = self.stream.as_mut().ok_or("S7 is not connected")?;
            send_tpkt(stream.get_mut(), &request)?;
            let response = receive_data(stream, self.pdu_length)?;
            Response::parse(&response, self.sequence)
        })();
        if result.is_err() {
            self.close_socket();
        }
        result
    }

    /// 不隐式连接，避免读写调用意外阻塞在重新建立连接上。
    fn require_connection(&self) -> Result<(), String> {
        if self.is_connected() {
            Ok(())
        } else {
            Err("S7 is not connected; call connect() first".into())
        }
    }

    /// 业务响应结构异常时主动使会话失效。
    fn invalid_response<T>(&mut self, message: &str) -> Result<T, String> {
        self.close_socket();
        Err(message.into())
    }

    /// 先移除本地连接，再尝试关闭双向传输；关闭失败也不复用旧 Socket。
    fn close_socket(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.get_ref().shutdown(Shutdown::Both);
        }
    }
}

impl DeviceBase for S7Net {
    fn connect(&mut self) -> Operator<bool> {
        S7Net::connect(self)
    }

    fn disconnect(&mut self) -> Operator<bool> {
        S7Net::disconnect(self)
    }
}

impl<T: S7ReadValue> ReadBase<T> for S7Net {
    fn read(&mut self, address: &str, length: usize) -> Operator<Box<[T]>> {
        S7Net::read(self, address, length)
    }
}

impl<T: S7Value> WriteBase<T> for S7Net {
    fn write(&mut self, address: &str, value: T) -> Operator<T> {
        S7Net::write(self, address, value)
    }

    fn write_all(&mut self, address: &str, array: &[T]) -> Operator<usize> {
        S7Net::write_all(self, address, array)
    }
}
