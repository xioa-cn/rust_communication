# 原生 DLL 内部接口

本页保留底层 ABI 契约，供内部维护和测试使用。业务代码请使用 [按品牌封装的 C# 客户端](README.md)，无需直接操作句柄或字节数组。`PlcNative`、原生配置结构和回调适配器已设为 `internal`，以下底层示例仅适用于同程序集内部验证。

本库通过稳定的 C ABI 导出 PLC 接口，C# 使用 P/Invoke 调用，不需要在 C# 中重新实现通讯协议。Rust 实现在 [csharp_dll.rs](../csharp_dll.rs)，配套声明和串口流适配器在 [PlcNative.cs](PlcNative.cs)。现有 Rust API 保持可用，根库没有新增第三方依赖。

## 构建和引用

在仓库根目录执行：

```powershell
cargo build --release --lib
```

Windows 默认生成 `target/release/rs_appliaction.dll`。将 DLL 放入 C# 程序输出目录，并将本目录的 C# 文件加入项目。DLL 与进程架构必须匹配，例如 x64 DLL 对应 x64 进程。不要只复制 `.dll.lib` 或 `.pdb`。

根目录保留了原有同名可执行入口；在 Windows 同时构建 bin 和 lib 时，Cargo 会提示 `.pdb` 输出同名。交付 DLL 时使用上面的 `--lib` 命令单独构建，不需要更改 Rust crate 或 DLL 名称。

`PlcNative.cs` 使用 `CallingConvention.Cdecl` 和显式 UTF-8 字符串封送，不需要启用 C# unsafe。原生接口版本由 `PlcNative.AbiVersion()` 返回，目前为 1；`PlcNative.Options(...)` 自动核对版本与配置结构大小。

## 最小读取示例

地址和网络参数需要按实际 PLC 配置修改。创建客户端不会连接；只有显式调用 `Connect` 才开始通讯。

```csharp
using System;
using System.Buffers.Binary;
using RsCommunication;

PlcOptions options = PlcNative.Options(PlcProtocol.ModbusTcp);
options.Port = 502;
options.UnitId = 1;
options.ByteOrder = PlcByteOrder.ABCD;

PlcNative.Check(PlcNative.Create(ref options, "192.168.0.10", out ulong handle));
try
{
    PlcNative.Check(PlcNative.Connect(handle));
    byte[] buffer = new byte[4];
    PlcNative.Check(PlcNative.Read(handle, "HR0", PlcDataType.UInt16, 2, buffer, out uint written));
    ushort first = BinaryPrimitives.ReadUInt16LittleEndian(buffer.AsSpan(0, 2));
    ushort second = BinaryPrimitives.ReadUInt16LittleEndian(buffer.AsSpan(2, 2));
    Console.WriteLine($"{first}, {second}; bytes={written}");
}
finally
{
    PlcNative.Destroy(handle);
}
```

`handle` 是 `ulong` 整数 ID，不是可解引用的内存指针。用 `Destroy` 释放，不能使用 `Marshal.FreeHGlobal`；释放后不得继续使用。`Disconnect` 只断开通讯，不释放句柄。

### 写入单值与数组

`Write` 的 `count` 是元素个数，缓冲区长度必须恰好等于数量乘以类型宽度；数量为 1 时使用根库的单值写入，否则使用数组写入。下面只定义函数，不会自动执行写入：

```csharp
static void WriteConfirmedValues(ulong handle)
{
    byte[] buffer = new byte[4];
    BinaryPrimitives.WriteUInt16LittleEndian(buffer.AsSpan(0, 2), 123);
    BinaryPrimitives.WriteUInt16LittleEndian(buffer.AsSpan(2, 2), 456);
    PlcNative.Check(PlcNative.Write(handle, "HR10", PlcDataType.UInt16, 2, buffer));
}
```

必须先确认设备、地址、权限和写入内容。超时不意味着 PLC 没有执行，数组可能只写入一部分；本接口不自动重试或回滚。

## 协议与配置

通过 `PlcNative.Options(protocol)` 获取协议默认值，不要手工构造全零配置或修改 `StructSize`。`Create` 接受 IP 字面量，不解析主机名。

| 协议枚举 | ABI 编号 | 主要配置 |
| --- | --- | --- |
| `S7` | 1 | `S7Type`、`Rack`、`Slot`、`UseTsap`、`LocalTsap`、`RemoteTsap` |
| `McBinaryTcp` / `McAsciiTcp` | 2 / 3 | `MelsecNetwork`、`MelsecPc`、`MelsecIo`、`MelsecStation`、`MonitoringTimer` |
| `McBinaryUdp` / `McAsciiUdp` | 4 / 5 | 同 MC TCP |
| `A1eBinaryTcp` / `A1eAsciiTcp` | 6 / 7 | A1E 路由仅使用 `MelsecPc` |
| `McRBinaryTcp` | 8 | 同 MC TCP |
| `FinsTcp` / `FinsUdp` | 9 / 10 | `FinsDestination*`、`FinsSource*`、`FinsGatewayCount`、`ByteOrder` |
| `ModbusTcp` / `ModbusUdp` | 11 / 12 | `UnitId`、`ByteOrder` |
| `ModbusRtu` / `ModbusAscii` | 13 / 14 | 使用 `CreateSerial`；`UnitId`、`ByteOrder`，RTU 另需 `BaudRate` |
| `InovanceModbusTcp` | 15 | `InovanceType`、`UnitId`、`ByteOrder` |

网络协议的 `ConnectTimeoutMs` / `ReceiveTimeoutMs` 单位为毫秒，默认均为 5000。默认端口仅为起点：S7 为 102、MELSEC 为 6000、FINS 为 9600、Modbus / 汇川为 502，须按实际工程修改。

S7 型号枚举为 `S1200/S1500/S300/S400/S200/S200Smart`；`UseTsap = 1` 才启用自定义 TSAP。汇川系列为 `AM/AC/AP/EVO/H5U/H3U/Easy`。FINS / 汇川默认字序是 `CDAB`，通用 Modbus 默认 `ABCD`。

地址沿用 [根库说明](../../../README.md)：例如 S7 的 `DB1.DBW0`、三菱的 `D100`、FINS 的 `CIO100.0`、Modbus 的 `HR0`。Modbus 支持 `x=2;HR100` 的单次站号覆盖，汇川支持 `x=2;D100` / `s=2;D100`。不自动转换 `40001` 等手册编号。

## 数据与缓冲区契约

| `PlcDataType` | ABI 编号 | 每个元素的缓冲区字节数 |
| --- | --- | --- |
| `Bool` | 1 | 1，仅允许 0 或 1 |
| `UInt8` / `Int8` | 2 / 3 | 1，通用 Modbus 不支持 |
| `UInt16` / `Int16` | 4 / 5 | 2 |
| `UInt32` / `Int32` | 6 / 7 | 4 |
| `UInt64` / `Int64` | 8 / 9 | 8 |
| `Float32` / `Float64` | 10 / 11 | 4 / 8 |

- C# 与 DLL 之间的数值缓冲区始终使用**小端**。`ByteOrder` 控制 PLC 侧的字节 / 字序，两者不能混淆。
- `bool` 不是 Win32 的 4 字节 BOOL；64 位整数按完整 8 字节传输，不经过浮点转换。
- 单次有效负载上限为 1 MiB；数值 `count` 必须大于零。底层协议仍会按自己的报文上限分包。
- 缓冲区不足返回 `BufferTooSmall`，`written` 返回所需容量，且不会发送 PLC 请求。可使用 `Array.Empty<byte>()` 查询；这不验证设备连通性或完整地址有效性。
- 成功时 `written` 是实际字节数。除容量不足外，读取失败时 `written` 为 0，输出数据不更新，不应继续解析旧内容。
- C# 辅助方法根据数组实际长度传递容量，并拒绝含 NUL 的主机 / 地址。直接调用原生 ABI 的其他语言调用方必须保证所有指针、长度、生命周期有效，输入 / 输出区域不能重叠。

### 字符串

`ReadString` / `WriteString` 与数值接口分离，使用 UTF-8 `byte[]`；返回数据不额外添加 NUL，也不删除文本内的 NUL。

- `PlcStringKind.RawUtf8`：读取时指定字节数，不是字符数；写入非空原始文本。S7 原始文本通过字节数组写入，不添加 S7 长度头。
- `PlcStringKind.S7String`：只支持 S7，地址指向 STRING 头；读取时 `byteLength` 必须为 0，预留至少 254 字节输出缓冲区。DLL 读取长度头后返回实际正文长度，不向 C# 返回头。
- S7 STRING 写入由根库保留 / 校验相应长度头及容量；支持用空文本清空长度。原始字符串写入不接受空数据。
- 字符串长度按 UTF-8 字节数计算，非 UTF-8 内容会报错；读取原始字符串时必须覆盖完整字符。
- 奇数字节的寄存器文本写入可能先读后写末字，保留相邻字节；这不是原子操作。

解码时使用 `Encoding.UTF8.GetString(buffer, 0, checked((int)written))`，编码时使用 `Encoding.UTF8.GetBytes(text)`。

## C# 串口流接入

根库不新增串口依赖。C# 负责打开和关闭串口、设置波特率 / 数据位 / 校验 / 停止位及读写超时；将已经配置好的双向 `Stream`，例如 `SerialPort.BaseStream`，交给 `PlcSerialStream`。单个流只绑定一个 PLC 句柄。

```csharp
static void ReadRtu(System.IO.Stream openedStream)
{
    var adapter = new PlcSerialStream(openedStream);
    PlcSerialCallbacks callbacks = adapter.Callbacks;
    PlcOptions options = PlcNative.Options(PlcProtocol.ModbusRtu);
    options.BaudRate = 9600;
    options.UnitId = 1;

    PlcNative.Check(PlcNative.CreateSerial(ref options, ref callbacks, out ulong handle));
    try
    {
        PlcNative.Check(PlcNative.Connect(handle));
        byte[] buffer = new byte[2];
        PlcNative.Check(PlcNative.Read(handle, "HR0", PlcDataType.UInt16, 1, buffer, out uint written));
        Console.WriteLine(BinaryPrimitives.ReadUInt16LittleEndian(buffer));
    }
    finally
    {
        PlcNative.Destroy(handle);
        GC.KeepAlive(adapter);
    }
}
```

`BaudRate` 必须与实际流一致；原生 `ReceiveTimeoutMs` 不会替调用方设置串口超时。调用方须确保阻塞读取最终会超时返回。

回调委托和上下文必须一直存活到 `Destroy` 返回，不能让 GC 提前回收。示例中的 `GC.KeepAlive` 是必要的生命周期约束；若跨方法保存句柄，应把 adapter 一起保存在实例字段中。`PlcSerialStream` 不拥有或关闭传入的流，调用方在销毁句柄后自行释放流。

自定义回调必须使用 Cdecl，读取 / 写入返回实际字节数，失败返回 -1、超时返回 -2；flush 成功返回 0。不得让托管异常跨过 native 边界，不得在回调内再次调用同一句柄。配套 adapter 已把流异常转为返回码。

RTU / ASCII 断开或传输错误导致底层流失效后，不能对原句柄直接重连；应销毁并重新创建串口句柄。它们不执行自动重试。

## 错误与线程

| 状态 | 数值 | 含义 |
| --- | --- | --- |
| `Ok` | 0 | 成功 |
| `InvalidArgument` | -1 | 空指针、配置尺寸、参数、编码或长度错误 |
| `InvalidHandle` | -2 | 句柄不存在或已释放 |
| `BufferTooSmall` | -3 | 输出缓冲区不足；未发送 PLC 请求 |
| `NotSupported` | -4 | 协议不支持此类型或字符串模式 |
| `OperationFailed` | -5 | 根库参数校验、通讯或 PLC 返回的错误 |
| `InternalError` | -6 | Rust 可展开 panic、会话锁中毒或内部一致性错误 |

调用后立即在**同一线程**使用 `GetLastError()` 获取详细信息；不要先切换线程、`await` 或调用其他成功的操作。`Check(status)` 会读取该信息并抛出 C# 异常。成功的普通操作会清空错误；`AbiVersion` 和错误查询不会清空。

所有通讯是同步阻塞的；GUI / 异步服务应安排在工作线程。每个句柄的完整操作由独立锁串行化，不同句柄可以并行；销毁等待已经执行的操作结束。UDP 的 `IsConnected` 仅表示本地 Socket 就绪，不是远端在线证明。

接口捕获 Rust 可展开 panic，但无法恢复无效指针、越界内存或 `panic=abort` 进程终止。不要在仍有句柄、活动调用或串口回调时卸载 DLL。本接口不替代工业安全联锁或业务权限确认。

## 本机验证

在仓库根目录执行：

```powershell
cargo test --lib hooks::csharp_dll --offline
cargo build --release --lib --offline
dotnet run --project tests/csharp/InteropSmoke.csproj --configuration Release -- target/release/rs_appliaction.dll
```

Rust 测试覆盖句柄、参数、缓冲区、数值精度、线程错误隔离、panic、TCP 读写、站号覆盖以及 RTU / ASCII 回调。C# 冒烟测试加载实际 DLL，核对导出符号和结构布局，执行回环 TCP 与内存串口回调，并检查 GC 后委托存活和异常转换。

测试不连接真实 PLC，也不打开物理串口。C# 测试工程当前使用本机可用的 `net10.0`，这不是原生 DLL 对所有调用方框架版本的限制。实际设备仍需按各协议的工程配置完成联调。
