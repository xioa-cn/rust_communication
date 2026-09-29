# rs_appliaction

基于 Rust 的 PLC 通讯库，提供西门子 S7、三菱 MELSEC、欧姆龙 FINS、Modbus 及汇川内置 Modbus TCP 的连接与数据读写能力。

根库使用 Rust 2024 Edition，当前不依赖第三方 crate；网络通讯基于标准库 TCP / UDP，Modbus 串口通讯通过调用方提供的字节流接入。各协议沿用一致的连接、读取、单值写入、数组写入及错误处理方式，适合在上位机、采集程序和设备调试工具中集成。

需要图形化连接与调试界面，请查看 **[PLC 通讯工作台 demo 文档](demo/README.md)**。该示例使用 Tauri + Vue，直接调用本库；启动、打包、连接配置和界面操作以 demo 文档为准。

## 支持的通讯协议

| 设备 / 协议 | 通讯方式 | 主要客户端 |
| --- | --- | --- |
| 西门子 S7 | S7 over TCP | `S7Net` |
| 三菱 MELSEC MC 3E | TCP / UDP，Binary / ASCII | `MelsecMcNet`、`MelsecMcAsciiNet`、`MelsecMcUdp`、`MelsecMcAsciiUdp` |
| 三菱 MELSEC A1E | TCP，Binary / ASCII | `MelsecA1ENet`、`MelsecA1EAsciiNet` |
| 三菱 MELSEC MC-R | TCP，Binary | `MelsecMcRNet` |
| 欧姆龙 FINS | TCP / UDP | `OmronFinsTcp`、`OmronFinsUdp` |
| Modbus | TCP / UDP / RTU / ASCII | `ModbusTcp`、`ModbusUdp`、`ModbusRtu`、`ModbusAscii` |
| 汇川 | 内置 Modbus TCP，按系列映射软元件地址 | `InovanceModbusTcp` |

支持协议不等于所有 CPU、固件和工程配置均已完成实机验证。连接前应确认设备开放了相应服务，并按实际工程填写端口、路由、站号和可访问地址。

## 接入 Rust 项目

在使用方的 `Cargo.toml` 中添加本地路径依赖，路径指向本仓库根目录：

```toml
[dependencies]
rs_appliaction = { path = "../rs_appliaction" }
```

使用支持 Rust 2024 Edition 的工具链。以下示例中的 IP、端口和地址均需替换为实际配置；示例不会由文档检查自动执行。

### 统一调用方式

| 方法 | 用途 | 成功时的内容 |
| --- | --- | --- |
| `connect()` | 建立连接 / 初始化通讯会话 | `bool` |
| `disconnect()` | 释放连接 | `bool` |
| `read::<T>(address, length)` | 连续读取 | `Box<[T]>` |
| `write::<T>(address, value)` | 写入单个值 | 写入值 |
| `write_all::<T>(address, values)` | 写入数组或切片 | 写入元素个数 `usize` |

调用结果使用 `Operator<T>`，可检查 `is_success`、`msg`、`content`，也可通过 `.to_result()?` 转为标准 `Result`。通用接口位于 `communication::device_base`，包括 `DeviceBase`、`ReadBase<T>`、`WriteBase<T>`；各客户端也提供直接调用的方法。

数值和 `bool` 的读取数量按元素计算，而不是统一按字节计算。`read::<String>(address, byte_length)` 按 UTF-8 字节数读取，结果是只含一个字符串的 `Box<[String]>`；`read_string` 则直接返回字符串。具体类型和地址支持以对应协议为准。

`Timeout::new(connect_ms, receive_ms)` 的单位为毫秒，两个值都必须大于零；`Timeout::default()` 的连接与接收超时均为 5 秒。

### 西门子 S7 读取示例

```rust,no_run
use rs_appliaction::communication::s7::s7_net::S7Net;
use rs_appliaction::communication::s7::s7_type::S7Type;
use rs_appliaction::communication::timeout::Timeout;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut plc = S7Net::new(
        "192.168.0.10".parse()?,
        102,
        S7Type::S1200,
        Timeout::new(3_000, 5_000),
        0,
        0,
    );

    plc.connect().to_result()?;
    let words = plc.read::<u16>("DB1.DBW0", 3).to_result()?;
    let flags = plc.read::<bool>("M100.0", 8).to_result()?;
    println!("words={words:?}, flags={flags:?}");
    plc.disconnect().to_result()?;
    Ok(())
}
```

构造函数最后两个参数为 Rack、Slot，应按实际硬件组态填写；需要自定义 TSAP 时使用 `.with_tsap(local, remote)`。

下面的函数演示单值和数组写入，**不会自行调用**。只有在调用方已连接设备，并确认目标地址、权限和写入内容安全后，才应执行：

```rust,no_run
use rs_appliaction::communication::s7::s7_net::S7Net;

fn write_values(plc: &mut S7Net) -> Result<(), Box<dyn std::error::Error>> {
    plc.write("DB1.DBW0", 123_u16).to_result()?;
    plc.write_all("DB1.DBW2", &[10_u16, 20, 30]).to_result()?;
    Ok(())
}
```

### Modbus TCP 读取示例

```rust,no_run
use rs_appliaction::communication::modbus::{ByteOrder, ModbusTcp};
use rs_appliaction::communication::timeout::Timeout;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut plc = ModbusTcp::new("192.168.0.10".parse()?, 502, Timeout::default());
    plc.set_unit_id(1).to_result()?;
    plc.set_byte_order(ByteOrder::ABCD);

    plc.connect().to_result()?;
    let registers = plc.read::<u16>("HR0", 3).to_result()?;
    let coils = plc.read::<bool>("C0", 8).to_result()?;
    let station_values = plc.read::<u16>("x=2;HR100", 2).to_result()?;
    println!("registers={registers:?}, coils={coils:?}, station_2={station_values:?}");
    plc.disconnect().to_result()?;
    Ok(())
}
```

`x=2;HR100` 只为本次操作指定站号 2，不改变客户端默认站号。四种 Modbus 模式均支持该地址前缀。

### 汇川读取示例

```rust,no_run
use rs_appliaction::communication::inovace::{InovanceModbusTcp, InovanceType};
use rs_appliaction::communication::timeout::Timeout;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut plc = InovanceModbusTcp::new(
        "192.168.0.10".parse()?,
        502,
        InovanceType::H5U,
        Timeout::default(),
    );
    plc.set_unit_id(1).to_result()?;

    plc.connect().to_result()?;
    let words = plc.read::<u16>("D100", 3).to_result()?;
    let inputs = plc.read::<bool>("X10", 8).to_result()?;
    println!("words={words:?}, inputs={inputs:?}");
    plc.disconnect().to_result()?;
    Ok(())
}
```

当前模块路径为 `communication::inovace`，导入时请沿用该拼写。汇川客户端默认站号为 1、数值字序为 `CDAB`；系列与字序必须匹配目标 PLC。

## C# / .NET 接入

本库同时提供原生 DLL 的 C ABI 接口，可在 C# 中通过 P/Invoke 使用现有 PLC 通讯能力：

```powershell
cargo build --release --lib
```

Windows 默认生成 `target/release/rs_appliaction.dll`。将 DLL 放入 C# 程序输出目录，并将 `src/hooks/CSharp` 下的 `.cs` 文件加入项目。C# 按品牌封装为独立文件：[Melsec.cs](src/hooks/CSharp/Melsec.cs)、[Omron.cs](src/hooks/CSharp/Omron.cs)、[Siemens.cs](src/hooks/CSharp/Siemens.cs)、[Inovance.cs](src/hooks/CSharp/Inovance.cs)，通用 Modbus 使用 [Modbus.cs](src/hooks/CSharp/Modbus.cs)。

```csharp
using var plc = new RsCommunication.Melsec("192.168.0.10", 6000);
var connect = plc.Connect();
if (!connect.IsSuccess)
{
    Console.WriteLine($"连接失败 [{connect.ErrorCode}]：{connect.Message}");
    return;
}
var read = plc.Read<short>("D100");
Console.WriteLine(read.IsSuccess ? $"D100 = {read.Content}" : $"读取失败：{read.Message}");
```

各品牌统一使用 `Read<T>`、`Write<T>`、`WriteAll<T>` 和字符串方法；连接及读写返回 `PlcResult` / `PlcResult<T>`，包含 `IsSuccess`、`ErrorCode`、`Message`，有数据的结果另带 `Content`，先判断成功再取数据。使用 `using` 自动释放；原生句柄、缓冲区及 P/Invoke 声明封装在内部，不改变现有 Rust API。

配置说明、C# 调用示例、缓冲区契约和测试命令见 **[C# 接入文档](src/hooks/CSharp/README.md)**。原生实现位于 [src/hooks/csharp_dll.rs](src/hooks/csharp_dll.rs)。

## 各协议的地址与使用约定

### 西门子 S7

- 支持 `S1200`、`S1500`、`S300`、`S400`、`S200`、`S200Smart`；通过 `S7Type` 选择。
- 字节 / 数值地址示例：`DB1.0`、`DB1.DBW0`；位地址示例：`DB1.100.3`、`M100.3`。读取 `bool` 时省略位号默认从第 0 位开始。
- S200 / S200 SMART 支持 `V`、`VB`、`VW`、`VD` 地址并映射到 DB1；S200 需要支持 S7 over TCP 的以太网模块或网关。
- `read_string` 读取无头 UTF-8 文本；`read_s7_string` / `read_s7_strings` 自动读取 S7 STRING 的长度头和内容。
- **`write::<String>` 写入的是带头 S7 STRING**，与原始文本读取不是同一种内存格式。写原始文本应使用 `write_all(address, text.as_bytes())`。
- S200 / SMART 使用一字节长度头，其他支持的系列使用两字节头。写入前须确认 PLC 已预留足够容量。
- 不包含串口 PPI、S7 Plus、符号寻址或 WSTRING。

更多接口与示例见 [S7 模块](src/communication/s7/mod.rs)。

### 三菱 MELSEC

- 从 `communication::melsec` 导入所需客户端，使用 `new(ip, port, timeout)` 创建；`MelsecMcNet` 是 `MelsecNet` 的别名。
- 使用 `with_route(...)` 配置网络号、PC 号、目标 I/O 和站号，使用 `with_monitoring_timer(...)` 配置监视定时器。端口以 PLC 工程配置为准。
- `D100` 为字地址，连续读取 `u32` 时依次占用 `D100/D101`、`D102/D103`；`M100` 为位地址。
- `X/Y/B/W/SB/SW/DX/DY/ZR` 区域使用十六进制编号，其他支持的区域使用十进制。
- 字符串为无头 UTF-8，不使用 S7 STRING 长度头。奇数字节写入会读改写末字，以保留邻接字节。
- 不支持标签、扩展模块缓冲区、寄存器点位后缀或串口协议；MC-R 的特殊设备结构支持也有限制。

七种客户端的使用示例和范围说明见 [MELSEC 模块](src/communication/melsec/mod.rs)。

### 欧姆龙 FINS

- 从 `communication::omron` 导入 `OmronFinsTcp` 或 `OmronFinsUdp`，使用 `new(ip, port, timeout)` 创建；端口按设备设置填写。
- 通过 `FinsRoute` 和 `set_route(...)` 配置节点、网络、单元及网关跳数；通过 `set_byte_order(...)` 配置多字数据转换。
- 支持 `D`、`CIO` / `C`、`W`、`H`、`A`、`EM bank`。例如 `D100` 为字地址，`CIO100.0` 为位地址；位号为 0–15。
- `C` 是 CIO 的别名，不是计数器；EM bank 使用十六进制，字偏移使用十进制。
- 支持单播 IPv4；不提供 TIM/CNT、标签、CPU 启停或广播。原始字符串不带 S7 STRING 长度头。

客户端定义见 [Omron 模块](src/communication/omron/mod.rs)，节点配置与调试说明见 [demo 文档](demo/README.md)。

### Modbus

| 地址 | 区域 | 读写权限 |
| --- | --- | --- |
| `C0` | 线圈 | 可读写 |
| `DI0` | 离散输入 | 只读 |
| `HR0` | 保持寄存器 | 可读写 |
| `IR0` | 输入寄存器 | 只读 |

- 地址从零开始，不自动把手册中的 `40001` 转换成 `HR0`。使用前先确认设备手册的编号规则。
- 支持 `bool/u16/i16/u32/i32/u64/i64/f32/f64/String`，不提供 `u8/i8` 直接读写。
- 支持 `ABCD/BADC/CDAB/DCBA` 字节 / 字序配置，按每个数值转换，不整体颠倒字符串。
- TCP / UDP 站号范围为 1–255，RTU / ASCII 为 1–247；不支持站号 0 广播。
- UDP 使用 MBAP + PDU，不是 RTU-over-UDP。
- RTU / ASCII 接收实现 `Read + Write` 的流，由调用方打开串口并配置波特率、校验、停止位和超时；根库不负责枚举或打开串口。
- `ModbusRtu::new(stream, unit_id, baud_rate)` 的波特率应与实际串口一致；`ModbusAscii::new(stream, unit_id)` 使用已配置好的流。`demo` 中提供了基于 `serialport` 的接入示例。
- 字符串采用原始 UTF-8，不裁剪零字节；奇数字节写入会先读末寄存器以保留相邻字节。空字符串及字符串数组写入不支持。

更多说明见 [Modbus 模块](src/communication/modbus/mod.rs)。

### 汇川

- 使用 `InovanceType` 显式选择 `H3U`、`H5U`、`Easy`、`AM`、`AC`、`AP` 或 `EVO`，不同系列的软元件地址不能混用。
- H3U / H5U / Easy 使用 `D100`、`M100`、`X10` 等地址，`X/Y` 使用八进制；Easy 沿用 H5U 内置 Modbus 映射，并不是 EasyNet 协议。
- AM / AC / AP 以及 EVO 支持 `I/IX`、`Q/QX`、`MW`、`MD`、`MB`、`MX` 等 IEC 风格地址；例如 `MD100` 对应 `MW200`。输入区为只读。
- AM / AC / AP 的 `SM`、`SD/SDW` 使用 AM600 扩展功能码；EVO 不套用这些扩展。实际支持情况以 CPU 和固件配置为准。
- 支持 `D100.3`、`MW100.15` 等寄存器位访问，以及 `x=2;D100` / `s=2;D100` 的单次站号覆盖。
- 寄存器位写入和不满字的字节写入属于读改写，不能作为原子操作使用。
- 仅实现内置 Modbus TCP，不包含 EtherNet/IP、EasyNet 或 PLC 启停控制。

各系列地址边界和数据映射见 [汇川模块](src/communication/inovace/mod.rs)。

## 通讯与写入注意事项

- 本库为同步阻塞 I/O。接入 GUI 或异步服务时，应在工作线程或阻塞任务中执行；共享同一客户端时需串行化完整的请求与响应过程。
- UDP 的本地 Socket 就绪不代表远端 PLC 在线；连接状态不能代替实际读写结果或业务心跳。
- UTF-8 字节长度不等于字符数；字符串读取必须覆盖完整字符，不自动识别其他编码，也不应假定写入会清理原有尾部数据。
- 分包读写不是 PLC 快照或事务；多包写入可能只完成一部分，读改写也可能与 PLC 程序或其他客户端发生竞争。
- **写入超时不代表设备没有执行。** 不应无条件重试，应先核对设备状态，再决定如何恢复。
- 先在模拟器或测试 PLC 上确认通讯组态、地址、类型、字序和权限，再用于实际设备。本库与 demo 不替代工业安全联锁和权限管理。

## 项目结构

```text
rs_appliaction/
├── src/
│   ├── communication/
│   │   ├── device_base.rs  通用连接与读写 trait
│   │   ├── timeout.rs      超时配置
│   │   ├── s7/             西门子 S7
│   │   ├── melsec/         三菱 MELSEC
│   │   ├── omron/          欧姆龙 FINS
│   │   ├── modbus/         Modbus TCP / UDP / RTU / ASCII
│   │   └── inovace/        汇川内置 Modbus TCP
│   ├── hooks/csharp_dll.rs C ABI / C# DLL 接口
│   ├── hooks/CSharp/       C# P/Invoke 声明与接入文档
│   └── entity/operate.rs   统一操作结果 Operator<T>
├── tests/                  协议、数据转换与通讯测试
└── demo/                   Tauri + Vue PLC 通讯工作台
    └── README.md           桌面示例的启动与使用说明
```

## 构建与验证

在仓库根目录执行：

```powershell
cargo check
cargo test --lib
cargo test --doc
cargo test --test s7_net --test s7_cpu_types
cargo test --test melsec_net
cargo test --test omron_net --test omron_values
cargo test --test modbus_net --test modbus_values --test modbus_strings --test modbus_serial
cargo test --test inovance_net --test inovance_values
```

以上选定的协议测试使用本机回环服务或内存流，不要求连接真实 PLC。仓库中的 `tests/s7_local_test.rs` 和 `tests/melsec_local_test.rs` 另有面向本机指定端口的联调示例；直接执行不带筛选的 `cargo test` 会包含这些测试，请先检查目标和环境，不要把它们当作无外部设备依赖的测试。

桌面示例的开发、构建及验证命令请参阅 **[demo/README.md](demo/README.md)**。

## 许可证

本项目使用 [Apache License 2.0](LICENSE)。
