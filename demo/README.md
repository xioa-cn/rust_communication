# PLC 通讯工作台（S7 / MELSEC / Omron FINS / Modbus）

Tauri 2 + Vue 3 + TypeScript + Vite + Naive UI 桌面示例。前端不实现协议，通过 Tauri 命令直接调用仓库根目录的 `rs_appliaction` 库。

## 启动

在 Windows 上准备 Node.js 22.12+（或 24 LTS）、Rust MSVC 工具链、Visual Studio C++ Build Tools（桌面 C++ 工作负载 / Windows SDK）及 WebView2 Runtime。

```powershell
cd E:\Project\cargo\rs_appliaction\demo
npm install
npm run tauri:dev
```

`npm run dev` 仅启动浏览器预览，不提供 Rust IPC，所有 PLC 操作会禁用。不要将浏览器预览误认为桌面应用。

`npm run tauri:dev` 启动 Tauri 桌面应用，`npm run tauri:build` 打包桌面应用。两者会通过 Tauri 配置自动调用对应的前端命令；保留 `dev` / `build` 作为纯前端脚本，避免循环调用。

```powershell
# 前端类型检查与生产构建
npm run build
# 生成 Windows 桌面程序与 NSIS 安装包
npm run tauri:build
# 只运行 demo 的模拟/回环测试，不连接实际 PLC
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

安装包输出到 `src-tauri/target/release/bundle/nsis/`。首次 Rust 编译和安装包构建需要下载依赖/打包工具。

## 设备分类导航

左侧按「品牌 → 系列 / 通讯类型」选择设备，不再把所有协议放进同一个下拉框：

- 西门子：S7-1200、S7-1500、S7-300、S7-400、S7-200、S7-200 SMART。
- 三菱：按 MC、A1E、MC-R 协议分类；右侧只显示当前分类支持的 TCP / UDP、Binary / ASCII 通讯方式，不代表新增硬件型号支持。
- 欧姆龙：FINS TCP、FINS UDP，调用根库的 `OmronFinsTcp` / `OmronFinsUdp`；CS/CJ/CP 系列实际能力由设备组态决定。
- 通用 Modbus：TCP、UDP、RTU 串口、ASCII 串口。

品牌可展开 / 收起，也可搜索品牌或系列。切换分类会保留本次打开页面期间各分类的连接参数草稿，并重置读写输入和写入确认；刷新或退出后草稿不保留。连接中或任务执行期间不可切换；先停止任务，再断开当前会话。仍采用单设备会话，不会在启动时自动连接或恢复采集。

分类与默认参数集中定义在 `src/deviceCatalog.ts`。新增入口必须关联已有后端支持的协议。运行 `npm test` 可检查分类覆盖、CPU 映射、搜索和默认参数。

## 如何引用 src 下的项目

`src-tauri/Cargo.toml` 已设置：

```toml
rs_appliaction = { path = "../.." }
```

Cargo 路径指向根 `Cargo.toml` 所在目录，而不是直接指向 `src`。库入口为根目录的 `src/lib.rs`，示例没有复制协议源码或新增 S7 通讯依赖。

## 工程调试工作区

- 桌面单屏工作区：从最小 820×640 窗口起，连接状态、左右读写卡片和工程工具区同时可见，右侧主页面不滚动、不分页。紧凑窗口自动收拢类型按钮为分类内的类型选择，左侧品牌 / 系列导航保持不变；长数据、代码和文本可在各自区域内滚动查看，不显示滚动条。
- 顶部连接配置入口打开独立弹窗：按设备/通讯方式、通讯地址（或串口设置）、协议参数、高级设置分区排列，数值控件填满对应网格；底部左侧显示本地会话状态，右侧仅显示连接或断开操作。已连接时锁定参数，断开后可修改；保留 IP、端口、串口、超时、站号、机架/槽位、TSAP 和字节序设置，成功连接后自动关闭，重新打开仍保留草稿。高级设置展开时也适配 820×640；运行任务时原位显示停止按钮，不挤压读写区域。
- 左右读写卡片：Bool、Byte/SByte、Short/UShort、Int/UInt、Long/ULong、Float/Double、String、S7 STRING；类型按协议能力禁用。支持 Dec/Hex/Bit、结果搜索、重复值抑制、最近 120 个数值样本首元素曲线，以及最近 500 次操作的耗时统计。
- 数值输入支持单值、数组 `[1,2,3]`、连续 `[1:100]`、重复 `[1*100]`，保留 64 位整数精度并检查范围；HEX 写入适用于 S7/MELSEC/FINS。文本支持 UTF-8/ASCII；S7/MELSEC/FINS 原始字符串额外支持 UTF-16 LE/BE 和字节对交换。
- 定时读取和点表监视必须手动启动；请求串行等待，不堆积定时器。定时写入需要显式确认且限定次数，支持逐次自增。错误即停，不自动重试；停止后等待已发出的请求结束，不能撤销已发送的写入。
- 工程工具箱仅保留五个页面：批量读取、报文读取、点位变量、地址示例、代码示例。移除远程调试、线程测试、数据模拟、数据导出和特殊功能；保留点表自身的导入、导出和监视。
- 批量读取采用“地址 + 长度 → 结果 → Rust 代码 → 解析/查找”的布局：S7/MELSEC/FINS 长度按字节，Modbus HR/IR 按寄存器、C/DI 按线圈；最多 1024 字节或 512 寄存器。支持 HEX、数值、文本解码、字节对交换、每行数量、选中位置及带超时的正则查找。Modbus 寄存器结果按高字节在前展开，不是原始网络线帧。
- 关闭批量解析与字节交换后可编辑 HEX；回写必须显式确认，目标绑定最近成功读取的地址且字节数不得改变。修改地址/长度/结果/显示设置、断开或切换设备会撤销确认；DI/IR 禁止回写。多地址采集使用点位变量工具，不再把地址清单塞进连续批量读取页。
- 地址示例按当前协议展示“地址类型 / 描述信息 / 位 / 字 / 备注”。列出真实支持的语法与只读限制，不直接照搬其他库的 `s=`、`format=` 等地址扩展；本项目的 Modbus `x=2;` 表示站号 2，不是功能码 2。
- Rust 示例左侧创建 PLC，右侧显示当前连接/读取/写入调用。点击连接或主读写按钮自动显示对应示例，批量操作更新示例但保留当前工具页；定时任务不会反复抢走工具页。代码使用本库真实 API、操作地址/类型/数量及连接参数，支持下载完整 `.rs`；写入示例始终保留 `confirmed = false`，不会因为界面确认而自动放行示例写入。

能力边界：报文工具读取的是数据区字节，不是线帧抓包；Modbus 将解码后的寄存器值重新编码为高字节在前。工作台仍是单设备会话，不提供虚拟 PLC 服务器、任意线帧透传或远程 CPU 启停。

新增组件 `DataConsole.vue`、`AdvancedTools.vue`、`BatchReadPanel.vue`；公共校验与任务控制在 `src/workbench.ts`，批量缓冲区转换在 `src/batchBuffer.ts`，Rust 代码生成在 `src/rustExamples.ts`，地址规则在 `src/addressExamples.ts`，搜索隔离在 `src/workers/search.ts`。单屏布局在 `src/single-screen.css`；浏览器宽度小于 761px 时保留移动端的纵向响应式布局。

验证：`npm test` 运行不连接 PLC 的单元测试；`npm run build` 检查类型与生产打包。`node tests/workbench.browser.mjs` 可对专用本地浏览器运行交互测试，默认预览端口 1462、浏览器 CDP 端口 9342，可通过 `PLC_TEST_APP_URL` / `PLC_TEST_DEBUG_URL` 指定。只对专用测试浏览器运行；测试注入模拟 IPC，不访问真实 PLC，并输出临时目录截图。

`node tests/rustExamples.compile.mjs` 在临时目录生成所有协议和可用读写类型的 Rust 示例并运行 `cargo check --offline`；需要已缓存 Rust 与 serialport 依赖。只检查编译，不执行示例、不打开串口、不连接 PLC。

## 目录分工

```text
demo/
  src/
    App.vue             # 页面布局和 Naive UI 全局 Provider
    theme.ts            # Apple 风格主题：颜色、圆角、字体与组件配置
    style.css           # 页面布局、响应式和减少动态效果适配
    components/         # Naive UI 连接、读写、结果与日志面板
    composables/useS7.ts # 操作状态与错误日志
    services/s7.ts      # Tauri invoke 封装
    types.ts            # 前后端请求/响应约定
  src-tauri/src/
    main.rs             # 可执行程序入口
    lib.rs              # 应用启动、状态管理与 IPC 注册
    commands/
      mod.rs            # 命令模块入口
      connection.rs     # connect / disconnect / status
      data.rs           # read / write
    models/
      mod.rs            # DTO 统一导出
      connection.rs     # 型号、连接请求与连接状态
      data.rs           # 数据类型、读写请求与响应
    plc/
      mod.rs            # 通讯服务统一导出
      connection.rs     # 连接参数、CPU 映射与 TSAP 校验
      read.rs           # 泛型读取与 STRING 分发
      write.rs          # 值解析、写入确认与泛型写入
      validation.rs     # 地址、数量与公共限制
      response.rs       # 库结果转换与已连接检查
      tests.rs          # 无网络的参数与安全约束测试
    state/
      mod.rs            # 状态模块入口
      session.rs        # 共享会话、状态快照与阻塞线程调度
```

## 界面与维护约定

- 采用 Apple 官网式白灰底色、留白、磨砂导航、圆角卡片和蓝色主按钮；保留 Rust 品牌，不使用 Apple 标识。
- 表单、选择器、数字输入、分段标签、按钮、提示、结果表格和日志确认均使用 Naive UI；图标来自 Ionicons。
- 全局组件样式集中在 `src/theme.ts`；布局在 `src/style.css`。主题不影响 IPC 数据结构。
- 浏览器预览允许编辑参数，但连接、读取和写入保持禁用；不注入模拟结果。
- 连接弹窗内的高级设置折叠收纳超时、TSAP 与 FINS 路由。日志支持关键词和状态筛选，清空需要确认；工作台读取结果在固定区域连续展示，不分页。
- Rust 依赖方向：commands → models / plc / state；plc 继续调用根目录现有协议库。不要在 IPC 层重复实现 S7 协议。
- 前端仍调用同名 connect、disconnect、status、read、write 命令；目录调整不改变 IPC 名称和参数。

## 三菱协议支持

连接面板现在支持以下三菱协议，并通过根库的统一读写 API 执行：

- MC 3E Binary TCP / ASCII TCP
- MC 3E Binary UDP / ASCII UDP
- A1E Binary TCP / ASCII TCP
- MC-R Binary TCP

选择三菱协议后填写网络号、PC 号、目标 I/O、站号和监视定时器；S7 的 Rack、Slot、TSAP 仅在 S7 协议下使用。MC 默认端口通常为 6000，实际端口以 PLC 工程配置为准。地址示例为 D100、M100、X10、W20；X/Y/B/W 等设备按协议使用十六进制编号。

三菱协议的 STRING 选项仅表示原始 UTF-8 文本，不写入 S7 长度头；写入仍需要确认地址、数据类型和 PLC 预留容量。UDP 连接状态只代表本地 Socket 已就绪，不代表远端 PLC 在线。

## 欧姆龙 FINS 支持

- 左侧「欧姆龙 → FINS · TCP / FINS · UDP」分别保留独立草稿，不混入三菱或 Modbus 分类。
- 预填端口 9600、字节序 CDAB；仅支持单播 IPv4。基本参数为源节点 SA1、目标节点 DA1 与字节序；高级设置提供网络号 SNA/DNA、单元号 SA2/DA2、网关跳数 GCT 和超时。
- 节点 0 为自动：TCP 由 FINS 握手协商，UDP 使用本地/目标 IPv4 末段。跨网络时目标节点必须显式填写 1–254；UDP 本地就绪不代表 PLC 实时在线。
- 默认位地址 `CIO100.0`，数值/字节地址 `D100`。支持 D、CIO/C、W、H、A、EM bank；位号为 0–15，EM bank 为十六进制，字偏移为十进制。C 是 CIO 别名，不是计数器。
- 普通读写、数组、HEX、字符串、连续批量读取、报文区字节、点表和 Rust 示例均已接入。S7 STRING 不可用；不提供 TIM/CNT、标签、CPU 启停或广播。
- 字符串/HEX/批量读取按字节访问；奇数字节写入由根库读改写末字以保留邻接字节，非原子操作。字内交换受连接字节序影响；批量解析按大端显示，不自动转换多字数值的字序。
- 点击连接/主读写会显示对应 Rust 示例，携带本次完整 FINS 路由和字节序；批量操作更新示例但不抢走工具页。写入示例仍默认 `confirmed = false`。
- 已连接时参数锁定；820×640 下展开全部路由设置仍保持单屏，无右侧主滚动条或分页。

后端通过现有 connect/read/write IPC 调用根库，不复制 FINS 报文实现；`omron` 连接选项缺省时沿用根库默认节点/字节序。
新增验证在 `tests/omron.test.ts`、`src-tauri/src/plc/omron_tests.rs` 和浏览器回归脚本中；仅使用模拟数据或本机随机端口服务，未进行实际 PLC 联调。

## 读写说明

- 型号与当前 Rust 库一致：S1200、S1500、S300、S400、S200、S200Smart。机架/槽位按实际组态填写，切换型号不会自动覆盖。
- 默认示例地址 `127.0.0.1:102` 只是可编辑输入，启动不会连接。自定义 TSAP 用十六进制，同时留空则沿用库默认值。
- bool 支持 `DB1.0` / `DB1.0.1` / `M0.1` 等库已有简写；读数量是位数，写入值为 `true` / `false`。
- 数值类型支持 i8/u8/i16/u16/i32/u32/i64/u64/f32/f64，读取数量按元素计，读取结果来自库的 `Box<[T]>`。
- 单值写入调用 `write::<T>`；数组调用 `write_all::<T>`。数组以逗号、空格或换行分隔，支持可选外层 `[]`。整数使用十进制。
- **原始 String**：调用 `read::<String>(address, byte_length)`，地址指向正文，数量是 UTF-8 字节数。S7/MELSEC/FINS 写入调用 `write_all::<u8>`；Modbus 写入调用 `write::<String>`，奇数字节通过末字读改写保留相邻字节。不带长度头、不自动清理原有尾部。
- **西门子 STRING**：调用 `read_s7_strings(address)`，不传长度，地址指向长度头；写入调用 `write::<String>`。S200/SMART 与其他型号的头布局由原库处理，不支持 WSTRING。
- 字符串均使用 UTF-8；非 UTF-8 数据可用 S7/MELSEC/FINS 的 u8 或 Modbus 的 u16 读取原始数据后自行解码。STRING 写入前确认 PLC 中的容量/预留内存正确。
- i64/u64 经 IPC 以文本传递和显示，避免 JavaScript 的整数精度损失；不接受 NaN/Infinity 写入。
- 示例将数值/位单次数量限制为 1..1024，原始文本读取为 1..1024 字节；超时范围为 1..60000 ms，限制仅在 demo，不改原库。

## 线程和安全约定

`Arc<Mutex<Option<PlcClient>>>` 持有共享会话，锁覆盖一次完整操作。Socket/串口 I/O 和锁等待在 `spawn_blocking` 线程池执行，不阻塞界面。状态标签是本地连接状态，不是远端心跳。

## Modbus 支持

连接面板提供 `Modbus TCP`、`Modbus UDP`、`Modbus RTU · 串口`、`Modbus ASCII · 串口` 四种选项。

- TCP/UDP 填写设备 IP、实际端口、站号和字节序。预填端口 502，站号 1，字节序 ABCD；请按设备手册调整。
- UDP 使用 MBAP + PDU，不是 RTU-over-UDP；本地 Socket 就绪不代表设备在线。
- RTU/ASCII 填写串口名（例如 COM3 或 /dev/ttyUSB0）、波特率、数据位、校验位、停止位、站号和收发超时。
- RTU 默认 9600/8E1，ASCII 默认 9600/7E1；选择无校验时默认改为两个停止位，仍可按设备设置调整。
- 站号不支持 0 广播；TCP/UDP 接受 1–255，RTU/ASCII 接受 1–247。
- 串口仅在点击“建立连接”后打开，不在启动或切换协议时探测端口。串口打开由操作系统处理，不使用网络连接超时字段。
- `serialport` 依赖只加入 demo；根协议库仍无第三方依赖。串口流会传给根库的 RTU/ASCII 客户端，不在 demo 重写协议报文。
- 不使用软/硬件流控，打开时不主动拉高 DTR。RS-485 方向切换和严格实时字符间隔仍依赖串口驱动及适配器；推荐使用自动收发方向的适配器。

### 地址与数据类型

地址从零开始：`C0` 为线圈，`DI0` 为只读离散输入，`HR0` 为保持寄存器，`IR0` 为只读输入寄存器。
不自动把 `40001` 转为 `HR0`。读写数量为元素数，多字数值按照实际寄存器宽度连续访问。

Modbus 提供 `bool/u16/i16/u32/i32/u64/i64/f32/f64` 和“String · 原始 UTF-8”。`u8/i8` 与 S7 STRING 不支持，前后端均校验。
字符串读取长度为字节数，返回一个字符串，不解析长度头、不删除零字节；长度必须覆盖完整的 UTF-8 字符。HR/IR 均可读取字符串，仅 HR 可写。
字符串按连续寄存器存放：ABCD/CDAB 保持字内字节顺序，BADC/DCBA 交换每个寄存器的两个字节，不颠倒整段文本。
奇数字节字符串写入先读末寄存器以保留相邻字节，需要读取权限；该读改写不是原子操作。空字符串和字符串数组写入不支持，不会隐式清空设备内存。

### 单次指定站号

四种 Modbus 模式均支持 `x=2;100`：本次读写使用站号 2，从协议偏移 100 开始。也可使用 `x=2;HR100`、`x=2;IR100`、`x=2;C0`、`x=2;DI0`。
不带区域前缀时，bool 按线圈解释，数值和字符串按保持寄存器解释；`x` 只表示站号，不表示功能码。
此覆盖作用于本次所有分包和字符串末字读改写，不会更改连接表单中的默认站号，出错后也不会残留。
站号仍遵守 TCP/UDP 的 1–255、RTU/ASCII 的 1–247 范围，不支持广播。多个站号前缀或无效格式会在发送前报错。

```rust
let words = device.read::<u16>("x=2;100", 3).to_result()?;
let texts = device.read::<String>("x=2;100", 10).to_result()?;
let text = device.read_string("x=2;HR100", 10).to_result()?;
```
DI/IR 在界面禁止写入，后端由根库再次校验。切换协议会更新地址提示、清空待写内容并重置确认，防止沿用另一协议的旧写入内容。

ABCD/BADC/CDAB/DCBA 按每个数值转换；64 位 CDAB 反转全部四个寄存器的顺序。
64 位整数仍通过 IPC 文本传递，不经过 JavaScript Number 转换。
写入必须明确确认，不自动重试；超时可能已经执行，分包写入也可能只完成一部分。

### 验证命令

在 demo 目录执行：

```text
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib --offline
```

新增测试通过本地回环 TCP/UDP 验证 demo 完整的创建、连接、读写、站号、字节序、只读限制、精度和异常状态。
串口配置测试不会打开真实设备；RTU/ASCII 报文另由根库的内存流测试覆盖，实际串口电气层仍需设备联调。

不会在页面启动时自动连接、轮询、写入或重试。手动写入每次重新确认；定时写入需确认整个有界任务，任务结束后撤销确认。修改地址/类型/值/任务参数会撤销确认。定时任务运行中禁止修改连接和切换设备；先停止任务，等待当前请求完成后再断开。写入超时不代表 PLC 没执行，多包数组操作可能只完成一部分；请核对设备后再决定后续操作。

本示例不能替代工业安全联锁或权限管理。请先在测试 PLC/模拟器上确认组态和地址，勿直接在生产设备上试写。
