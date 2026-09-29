# C# PLC 客户端：一个品牌一个文件

业务代码直接使用对应品牌类，不需要操作 `PlcNative`、原生句柄或字节数组。连接、断开、读取和写入都返回统一结果，可以直接判断成功或失败并获取错误信息。

| 文件 | 客户端 | 支持范围 |
| --- | --- | --- |
| [Melsec.cs](Melsec.cs) | `Melsec` | 三菱 MC TCP/UDP、Binary/ASCII、A1E、MC-R |
| [Omron.cs](Omron.cs) | `Omron` | 欧姆龙 FINS TCP/UDP |
| [Siemens.cs](Siemens.cs) | `Siemens` | 西门子 S7-1200/1500/300/400/200/200 SMART |
| [Inovance.cs](Inovance.cs) | `Inovance` | 汇川 AM/AC/AP/EVO/H5U/H3U/Easy |
| [Modbus.cs](Modbus.cs) | `Modbus` | 通用 Modbus TCP/UDP/RTU/ASCII |

每个文件包含该品牌的客户端、协议枚举及可选配置。`PlcClient.cs` 复用读写和资源释放逻辑，`PlcResult.cs` 定义统一结果，`PlcNative.cs` 只负责内部 P/Invoke；不需要为每个品牌复制一份底层声明。

## 接入

在仓库根目录构建原生 DLL：

```powershell
cargo build --release --lib
```

将 `target/release/rs_appliaction.dll` 放到 C# 程序输出目录，并将本目录的 `.cs` 文件加入 C# 项目。DLL 与进程架构必须一致。原生 Rust API 不变。

## 三菱：直接读写

```csharp
using RsCommunication;

using var plc = new Melsec("192.168.0.10", 6000);
var connect = plc.Connect();
if (!connect.IsSuccess)
{
    Console.WriteLine($"连接失败 [{connect.ErrorCode}]：{connect.Message}");
    return;
}

var read = plc.Read<short>("D100");
if (read.IsSuccess)
    Console.WriteLine($"D100 = {read.Content}");
else
    Console.WriteLine($"读取失败 [{read.ErrorCode}]：{read.Message}");

var values = plc.Read<short>("D100", 10);
if (values.IsSuccess)
    Console.WriteLine(string.Join(", ", values.Content));
else
    Console.WriteLine($"数组读取失败：{values.Message}");
```

单值和数组写入的使用方式如下。函数不会自行调用，必须先确认地址、权限及写入内容安全：

```csharp
static void WriteConfirmedValues(Melsec plc)
{
    var write = plc.Write("D100", (short)123);
    if (!write.IsSuccess)
    {
        Console.WriteLine($"写入失败 [{write.ErrorCode}]：{write.Message}");
        return;
    }
    Console.WriteLine($"已写入：{write.Content}");

    var batch = plc.WriteAll<short>("D110", 1, 2, 3);
    Console.WriteLine(batch.IsSuccess
        ? $"已写入 {batch.Content} 个元素"
        : $"数组写入失败 [{batch.ErrorCode}]：{batch.Message}");
}
```

## 其他品牌用法相同

```csharp
using var omron = new Omron("192.168.0.20", 9600);
using var siemens = new Siemens("192.168.0.30", cpu: S7Cpu.S1500);
using var inovance = new Inovance("192.168.0.40", InovanceSeries.H5U);
using var modbus = new Modbus("192.168.0.50", 502, unitId: 1);
```

构造不会自动连接；对需要使用的客户端调用 `Connect()` 并确认 `IsSuccess` 后，再执行 `Read<T>` / `Write<T>`。地址语法仍按品牌区分，例如三菱的 `D100`、欧姆龙的 `CIO100.0`、西门子的 `DB1.DBW0`、Modbus 的 `HR0`。

### 需要高级配置时

各品牌的选项也放在对应文件，不必填写其他品牌的参数。例如欧姆龙 FINS UDP：

```csharp
using var plc = new Omron("192.168.0.20", new OmronOptions
{
    Protocol = OmronProtocol.FinsUdp,
    Port = 9600,
    SourceNode = 20,
    DestinationNode = 10,
    ByteOrder = PlcByteOrder.CDAB,
    ConnectTimeoutMs = 3000,
    ReceiveTimeoutMs = 5000
});
var connect = plc.Connect();
if (!connect.IsSuccess)
{
    Console.WriteLine(connect.Message);
    return;
}
var read = plc.Read<ushort>("D100");
Console.WriteLine(read.IsSuccess ? $"D100 = {read.Content}" : read.Message);
```

- 三菱：`MelsecOptions` 配置 MC/A1E/MC-R、路由和监视定时器。
- 欧姆龙：`OmronOptions` 配置 FINS 节点、网络、单元和字序。
- 西门子：`SiemensOptions` 配置 CPU、Rack/Slot 和 TSAP；本地与远端 TSAP 必须同时指定。
- 汇川：`InovanceOptions` 配置系列、站号和字序；不同系列地址不能混用。
- Modbus：`ModbusOptions` 配置 TCP/UDP、站号和字序。

选项在构造时复制；之后修改选项对象不会改变已创建的客户端。端口必须匹配设备实际组态，超时单位为毫秒。

## 通用方法

`PlcResult` 提供 `IsSuccess`、`ErrorCode`、`Message`；`PlcResult<T>` 继承它并增加 `Content`。属性均只读，结果不会被后续调用覆盖。

**必须先判断 `IsSuccess` 再使用 `Content`。** 失败时 `Content` 为 `default(T)`：数值为 0、布尔为 false、数组和字符串为 null，这些都不是有效读取结果。成功读取的 0 / false 仍然是成功，不应根据内容判断操作状态。

| 调用 | 返回 / 用途 |
| --- | --- |
| `Connect()` / `Disconnect()` | `PlcResult`，建立 / 断开连接的操作结果 |
| `GetConnectionState()` | `PlcResult<bool>`，`Content` 为本地连接状态 |
| `IsConnected` | 便捷 bool 属性；失败时仍抛异常，需错误详情时使用 `GetConnectionState()` |
| `Read<T>(address)` | `PlcResult<T>`，成功时 `Content` 为单值 |
| `Read<T>(address, count)` | `PlcResult<T[]>`，成功时 `Content` 为数组，数量按元素计算 |
| `Write<T>(address, value)` | `PlcResult<T>`，成功时 `Content` 为本次写入值 |
| `WriteAll<T>(address, values)` | `PlcResult<int>`，成功时 `Content` 为写入元素数 |
| `ReadString(address, byteLength)` | `PlcResult<string>`，读取原始 UTF-8 文本 |
| `WriteString(address, value)` | `PlcResult<string>`，成功时 `Content` 为本次写入文本 |
| `Siemens.ReadS7String(address)` / `WriteS7String(address, value)` | `PlcResult<string>`，西门子带长度头 STRING 的读取 / 写入结果 |
| `Dispose()` | 释放原生连接；推荐使用 `using` 自动执行 |

`GetConnectionState().IsSuccess` 表示状态查询成功，是否已连接要看 `Content`。状态仅反映本地会话，UDP 的连接成功或本地已连接不代表远端 PLC 在线。写入成功表示底层调用成功，不会额外回读验证 PLC 实际值。

`T` 支持 `bool/byte/sbyte/ushort/short/uint/int/ulong/long/float/double`；通用 Modbus 不支持 `byte/sbyte`。`decimal`、`DateTime`、自定义结构等类型会被拒绝，字符串使用专用方法。

数值编码、字节序转换和缓冲区容量由客户端处理。单次有效负载上限为 1 MiB，底层协议仍可能分包。`ulong/long` 按完整 64 位传递。

原始文本长度是 UTF-8 字节数，保留文本中的零字节，不自动清除旧尾部。西门子带长度头的 STRING 另外使用 `Siemens.ReadS7String(address)` / `WriteS7String(address, value)`，不要和原始文本混用。

## Modbus 串口

调用方打开串口并设置波特率、校验、停止位和读写超时，将已打开的双向流传入：

```csharp
static void ReadRtu(System.IO.Stream openedStream)
{
    using var plc = Modbus.Rtu(openedStream, unitId: 1, baudRate: 9600);
    var connect = plc.Connect();
    if (!connect.IsSuccess)
    {
        Console.WriteLine($"串口会话连接失败：{connect.Message}");
        return;
    }
    var read = plc.Read<ushort>("HR0");
    Console.WriteLine(read.IsSuccess ? $"HR0 = {read.Content}" : $"读取失败：{read.Message}");
}
```

ASCII 使用 `Modbus.Ascii(openedStream, unitId: 1)`。客户端自动保留回调委托和处理原生句柄，不需要手动 `GC.KeepAlive` 或 `Destroy`。

流仍由调用方拥有，客户端释放不会关闭传入的流。一个流只交给一个客户端；先释放客户端再关闭流。RTU 波特率必须与实际串口一致；串口超时由调用方设置。RTU/ASCII 断开或传输错误后，应重新创建客户端，不要直接对失效会话重连。

## 错误与线程

操作方法中的参数校验、不支持的泛型类型、已释放对象及原生通讯失败返回 `IsSuccess == false`，不再要求业务代码捕获这些操作异常。原生状态码和原始错误信息会在当前线程立即复制到结果中，之后再调用其他方法也不会丢失。

| ErrorCode | 含义 |
| --- | --- |
| `0` | 成功，`Message` 为 `Success` |
| `-1` | 参数无效，例如空地址、数量越界或无效 UTF-8 输入 |
| `-2` | 无效句柄或客户端已经释放 |
| `-3` | 原生缓冲区不足 |
| `-4` | 不支持的类型或操作 |
| `-5` | 通讯 / 协议操作失败，包括连接失败、PLC 拒绝或返回数据异常 |
| `-6` | 原生内部错误 |

以上是本库的状态码，不是 PLC 厂商的协议错误码；底层提供的设备错误细节保留在 `Message`。`new Melsec(...)` 等构造阶段配置错误、DLL 缺失 / 架构不匹配 / ABI 不兼容，以及内存不足等非预期运行时问题仍会抛异常，不伪装成普通通讯失败。`Dispose()` 遵循 `IDisposable` 保持 void；需要检查断开结果时先调用 `Disconnect()`。

这是 C# 返回类型的变更：原来的 `short value = plc.Read<short>(...)` 需改为检查结果再取 `Content`；原来仅调用 `Write(...)` 而不检查返回值的代码仍可编译，但会忽略失败，必须同步改造。Rust API 和原生 C ABI 不变。

同一客户端的操作和释放会串行执行，不同客户端可以并行。调用是同步阻塞的，GUI 应放到工作线程执行。不要在串口流的读写回调中反过来调用同一 PLC 客户端。

写入不自动重试，超时不代表 PLC 没有执行；分包写入可能只完成部分操作。请先在测试 PLC 或模拟器上验证，不能代替工业安全联锁。

## 验证

```powershell
cargo build --release --lib --offline
dotnet run --project tests/csharp/InteropSmoke.csproj --configuration Release -- target/release/rs_appliaction.dll
```

测试只使用本机回环服务和内存串口流，覆盖品牌选择、三菱 / 欧姆龙实际报文、泛型数值与数组、UTF-8 文本、自动释放及串口回调生命周期，以及连接拒绝、未连接读写、PLC 错误响应、参数无效、释放后调用、错误信息保留和成功读取 0 / false，不连接真实 PLC。

底层维护人员可参阅 [原生 ABI 内部说明](NativeAbi.md)，业务代码无需使用这些内部接口。
