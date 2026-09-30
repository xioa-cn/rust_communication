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

## 高频读取与缓冲复用

`Read<T>` 仍返回独立的结果数组；数值读取现在直接填充最终数组，单值读取不再创建临时单元素数组。原生层直接编码到调用方缓冲区，C ABI 版本及字序约定不变。包装层需要支持 `Span<T>` / `MemoryMarshal` 的运行时（.NET Standard 2.1 API，例如 .NET 6+）；无需开启 unsafe。

连续轮询相同长度的数据时，可将数组放在轮询循环外，使用 `ReadInto<T>` 复用：

```csharp
short[] values = new short[100];
var read = plc.ReadInto("1", values);
if (!read.IsSuccess)
    throw new InvalidOperationException(read.Message);
Console.WriteLine($"读取元素数：{read.Content}，首个值：{values[0]}");
```

`destination.Length` 就是读取元素数，成功时 `Content` 返回该数量。每次调用都执行真实设备读取，不缓存旧数据。调用方拥有数组，不应由其他线程同时访问；下一次成功读取会覆盖数组，若需要历史快照需自行复制。失败时不要消费本次结果。此 API 消除重复的数据数组分配，但结果对象等仍有少量托管分配，并非完全零分配。

同一客户端只有一个串行请求通道：多个 `Task` 共用该对象不会使单连接并行收发。不要删除安全锁；多连接并发应在设备允许的连接数和负载范围内单独测试。高频采集优先合并连续地址，不要将每个点都作为单独请求。

性能比较应固定有效地址、类型和数量，预热相同批量调用，检查全部成功，并在 Release 下交替测试多轮。不能把越界失败计入成功吞吐，也不能用首次调用推断稳定延迟。仓库中的 `benchmarks/ModbusRead` 提供同进程 HSL 对比和托管互操作回归验证，只访问它自己启动的回环模拟器。

## C# 缓冲区 API 升级（2026-09-30）

以下接口由 `PlcClient` 统一提供，`Modbus`、`Inovance`、`Melsec`、`Omron`、`Siemens` 均可调用，不需要更换品牌类。现有 `Read<T>`、`Write<T>`、数组版 `ReadInto<T>` 和 `WriteAll<T>(params T[])` 保留原来的调用方式与返回类型。

| 接口 | 用途 |
| --- | --- |
| `ReadInto<T>(string address, T[] destination, int offset, int count)` | 直接填充目标数组的指定区间 |
| `ReadInto<T>(string address, Span<T> destination)` | 读取到数组切片或小型栈缓冲区 |
| `WriteAll<T>(string address, T[] values, int offset, int count)` | 只写入源数组的指定区间，不必自行截取数组 |
| `WriteAll<T>(string address, ReadOnlySpan<T> values)` | 写入只读切片或小型栈缓冲区 |

四个新增重载均返回 `PlcResult<int>`；成功时 `Content` 是传输的 **T 类型元素个数，不是字节数**。数组重载中的 `offset/count` 也是元素单位，`count` 必须大于零且区间必须落在数组内。空 Span、空数组、无效区间及不支持的类型返回失败结果，不发送设备请求。

以下 `plc` 是已经连接成功的品牌客户端，`address` 使用调用方已确认可读写的设备地址：

```csharp
short[] buffer = new short[256];
var read = plc.ReadInto(address, buffer, offset: 16, count: 100);
if (!read.IsSuccess)
    throw new InvalidOperationException(read.Message);

var write = plc.WriteAll(address, buffer, offset: 16, count: 100);
if (!write.IsSuccess)
    throw new InvalidOperationException(write.Message);
```

已有 Span 时可直接传递，不必调用 `ToArray()`。显式指定泛型参数可兼容较早的 C# Span 类型推断规则：

```csharp
Span<short> window = buffer.AsSpan(16, 100);
var readWindow = plc.ReadInto<short>(address, window);
if (!readWindow.IsSuccess)
    throw new InvalidOperationException(readWindow.Message);

var writeWindow = plc.WriteAll<short>(address, window);
if (!writeWindow.IsSuccess)
    throw new InvalidOperationException(writeWindow.Message);
```

小窗口也可以使用 `Span<short> window = stackalloc short[8]`。这些都是同步调用，Span 只在调用期间使用；不会保存 Span、原始指针或调用者数组供后台任务使用。不能跨 `await` 保持 Span，也不要在大量循环中反复 `stackalloc` 或为大批量数据使用栈分配。

区间外的目标数据不改动，写入不修改源数据。调用期间不要让其他线程访问同一读缓冲区或修改写缓冲区。底层仍遵守同一套 FIFO、SafeHandle 保活、字序、超时、分包与错误处理规则；不是并行请求或自动合并读写。小端平台写入直接使用字节视图，大端平台保留字序转换副本。

新接口避免临时切片数组，不代表整个调用零分配；当前 .NET 10 本地回归中四个新增重载的调用线程分配均为 40 B/次（结果对象），不包含原生层、服务端或调用方自己的分配，也不是速度承诺。

C ABI 仍为 1，继续使用现有 `plc_read` / `plc_write` 导出。升级时同步本目录 C# 文件并重新编译应用；只替换原生 DLL 不会增加这些 C# 重载。共享 `PlcClient.cs` 和新增接口已同步到 Demo，用户的 `Program.cs` 不由本次升级修改。

仓库原有的 Modbus 默认字序不同：本目录 `ModbusOptions` 为 `ABCD`，Demo 为 `CDAB`；本次升级保留该配置差异，不擅自改变已有数据解释。需要一致行为时，应显式设置 `ModbusOptions.ByteOrder` 并将 options 传给构造函数，而不是只创建一个未使用的 options 对象。Demo 的 `PlcNative.cs` 另有排版差异，接口功能通过直接编译 Demo 封装运行回归来验证，不要求整个目录逐文件哈希相同。

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

同一客户端的操作和释放会串行执行，不同客户端可以并行。竞争同一客户端时，已经进入等待队列的请求按 FIFO 顺序交接；刚完成一次调用的线程不能越过已排队的调用再次抢占。无竞争时直接进入，不分配等待节点。排队线程中断时会移除其等待节点，交接期间也不会丢失执行权；这不是取消已经发送的设备命令。

成功完成等待的节点在线程内复用，避免高频争用时每次都新建等待对象；一个线程最多缓存一个节点，也可以用于该线程随后访问的另一个客户端。被中断的等待节点不回收，防止延迟到达的唤醒信号误用到下一次请求。FIFO、重入、释放和异常处理规则不变。

公平交接仍有线程唤醒及排队开销，并不保证硬实时延迟或每个任务完全同时结束。调用是同步阻塞的，GUI 应放到工作线程执行。不要在串口流的读写回调中反过来调用同一 PLC 客户端。

写入不自动重试，超时不代表 PLC 没有执行；分包写入可能只完成部分操作。请先在测试 PLC 或模拟器上验证，不能代替工业安全联锁。

## 数值写入热路径

数值/布尔 `Read`、`ReadInto`、`Write`、`WriteAll` 使用静态委托并显式传递参数，不再每次分配捕获地址、数量或数值的闭包。这是所有品牌共用的 C# 封装优化，API 和原生 ABI 不变；需要重新编译 C# 程序，只替换原生 DLL 不会生效。Demo 风格的 `Task.Run` 测试及尚未消除的延迟尖峰见 `benchmarks/ModbusRead/CSHARP_HOTPATH_RESULTS.md`。

Windows 上的 Modbus TCP（含汇川封装）另外缓存已成功设置的 Socket 读/写超时选项，跳过实际毫秒值相同的重复设置；每次收发仍检查剩余事务期限。该缓存不保存 PLC 值，连接重建时重新初始化。30,000 次长循环及仍存在的最大延迟限制见 `benchmarks/ModbusRead/SOCKET_TIMEOUT_RESULTS.md`，不能承诺每次调用都比 HSL 快。

`Write<T>` 直接传递局部标量的字节视图，不再构造单元素数组、临时 `byte[]` 和中间 `PlcResult<int>`。小端平台上的 `WriteAll<T>` 直接传递调用者数组的只读字节视图，移除 `Buffer.BlockCopy`；大端平台仍先转换副本，不会原地反转调用者数组。调用完成前不要从其他线程修改输入数组。字符串写入不在本次优化范围内。

原生 ABI 的标量解码使用栈上值，仍通过 `from_le_bytes` 处理非对齐数据和布尔校验，不强制转换不可信指针。Modbus（包含汇川封装）、三菱、欧姆龙和 S7 的内置数值/布尔批量编码直接追加到预留缓冲，避免每个元素各分配一个临时 `Vec`；自定义数值类型保留原有编码方法作为默认回退。

MC/FINS/S7 TCP 在连接内复用接收缓冲，写入确认也走同一路径；S7 直接重组 COTP 分段到最终响应，保留粘包中的剩余帧。FINS 保留整次事务期限，即使数据已缓冲也不能绕过超时。MC UDP 的 65,535 字节接收区改为每连接复用。这些都是报文缓冲，不缓存 PLC 数据，不跳过确认、不自动重试，也不移除公平锁。

修改托管包装层后必须重新编译 C# 工程，不能只替换 DLL。性能对比、适用范围及尾延迟限制见仓库 `benchmarks/ModbusRead/WRITE_RESULTS.md`；不要把降低分配量的百分比直接当成通讯速度提升比例。

## 验证

```powershell
cargo build --release --lib --offline
dotnet run --project tests/csharp/InteropSmoke.csproj --configuration Release -- target/release/rs_appliaction.dll
```

测试只使用本机回环服务和内存串口流，覆盖品牌选择、三菱 / 欧姆龙实际报文、泛型数值与数组、UTF-8 文本、自动释放及串口回调生命周期，以及连接拒绝、未连接读写、PLC 错误响应、参数无效、释放后调用、错误信息保留和成功读取 0 / false，不连接真实 PLC。

底层维护人员可参阅 [原生 ABI 内部说明](NativeAbi.md)，业务代码无需使用这些内部接口。
