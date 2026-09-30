import type { InovanceSeries, PlcProtocol } from './types.ts'
import { isCipProtocol, isInovanceIecSeries, isInovanceProtocol } from './types.ts'

export interface AddressExample { address: string; description: string; bit: boolean; word: boolean; note: string }
const entry = (address: string, description: string, bit: boolean, word: boolean, note: string): AddressExample => ({ address, description, bit, word, note })

export function addressExamples(protocol: PlcProtocol, series: InovanceSeries = 'AM'): AddressExample[] {
  if (isCipProtocol(protocol)) return [
    ...(protocol === 'omron_cip' ? [entry('Text', '[String] Omron STRING 标签', false, true, '选择 String，数量 1；解析 UTF-8 长度头，不是 BYTE 数组。当前仅支持读取。'), entry('Texts[0]', '[String] 字符串数组', false, true, '选择 String，数量为字符串个数；从指定下标逐项读取，保留空字符串和中文。')] : []),
    entry('Flag', '[Bool] 字节型 BOOL 标签', true, false, 'PLC 工程中的符号名；写入 true/false，不支持整数打包的 BOOL 数组。'),
    entry('Values[0]', '[原子类型] 数组', false, true, '长度按元素；类型必须与 PLC 标签定义完全一致。'),
    entry('Bytes[0]', '[Byte] USINT/BYTE 数组', false, true, '批量字节读写仅适用于字节型数组，不是任意标签的内存映射。'),
    entry('Program:Main.Tag', '[原子类型] 程序标签', true, true, '使用目标 PLC 工程实际定义的标签路径。'),
    entry('Matrix[1,256].Value', '[原子类型] 数组与成员', true, true, '最终成员必须是支持的原子类型，不读取整个 UDT。'),
    entry('type=0xD2;Words[0]', '[Word] 显式 WORD 类型', false, true, '使用 u16；默认 u16 写 UINT/C7，WORD/D2 必须显式指定。'),
  ]
  if (isInovanceProtocol(protocol)) {
    if (isInovanceIecSeries(series)) {
      const rows = [
        entry('MW100', '[Word] M 寄存器', false, true, '十进制字偏移；长度按类型元素数，不是字节地址。'),
        entry('MD100', '[DWord] 双字别名', false, true, 'MD100 = MW200；数值类型决定每元素字数。'),
        entry('MB1 / MB100', '[Byte] 字节地址', false, true, '字节/文本低字节在先；支持奇数起点。多字节数值必须从偶数 MB 起点访问。'),
        entry('MX100.0', '[Bool] M 字节位', true, false, '位号 0–7；MX1.0 对应 MW0 的第 8 位，不是 MW1。'),
        entry('MW100.15', '[Bool] 寄存器位', true, false, '位号 0–15；写入先读后改写，不是原子操作。'),
        entry('IX0.0 / I0', '[Bool] 输入', true, false, '功能码 02，只读；物理输入对应关系由 PLC 配置决定。'),
        entry('QX0.0 / Q0', '[Bool] 输出', true, false, '点号前为字节偏移，位号 0–7；无点号时为位偏移。'),
        entry('s=2;MW100 / x=2;MW100', '[Word] 单次指定站号', false, true, '两种前缀都表示站号；不改变连接默认值，不是功能码。'),
      ]
      if (series !== 'EVO') rows.push(
        entry('SM0', '[Bool] 系统位', true, false, 'AM/AC/AP 共用映射；使用 AM600 扩展功能码，CPU 必须支持。'),
        entry('SD0 / SDW0', '[Word] 系统字', false, true, '实际可访问范围、扩展支持和写权限由 CPU/固件决定。'),
      )
      return rows
    }
    const h3u = series === 'H3U'
    const rows = [
      entry('M100', '[Bool] 中间继电器', true, false, h3u ? 'M0–7679、M8000–8511；不允许连续请求跨越中间空段。' : '十进制，M0–7999。'),
      entry('X10 / X1.0', '[Bool] 输入', true, false, '八进制，二者都表示偏移 8；只读。不要套用三菱十六进制 X10。'),
      entry('Y10 / Y1.0', '[Bool] 输出', true, false, h3u ? '八进制，X/Y0–377。' : '八进制，X/Y0–1777。'),
      entry('S0', '[Bool] 状态继电器', true, false, '十进制，S0–4095。'),
      entry('D100 / R100', '[Word] 数据 / 文件寄存器', false, true, h3u ? 'D0–8511，R0–32767。原始字节和文本按字节长度。' : 'D0–7999，R0–32767。原始字节和文本按字节长度。'),
      entry('D100.15', '[Bool] 寄存器位', true, false, '位号 0–15；写入先读后改写，保留邻位但不是原子操作。'),
      entry('s=2;D100 / x=2;D100', '[Word] 单次指定站号', false, true, '站号范围 1–255；不支持广播，不改变连接默认站号。'),
    ]
    if (h3u) rows.push(
      entry('T0 / C0', '[Bool/Word] 定时器 / 计数器', true, true, 'Bool 是触点；T0–511、C0–199 的数值是当前值。'),
      entry('C200', '[32-bit] 双字计数器', false, true, 'C200–255 仅用 UInt/Int/Float；不能用批量 Byte，也不能跨越 C199/C200 边界。'),
      entry('SM0 / SD0', '[Bool/Word] 特殊区', true, true, 'H3U 支持 SM0–1023、SD0–1023；不要与 AM 扩展功能码混用。'),
    )
    else rows.push(entry('B0', '[Bool] 链接继电器', true, false, 'H5U/Easy 支持 B0–32767，不支持 H3U 的 T/C/SM/SD 地址。'))
    return rows
  }
  if (protocol.startsWith('omron_')) return [
    entry('D100 / DM100', '[Word] 数据存储区', false, true, '十进制字地址；UInt/Float 等 32 位类型占 2 字，64 位占 4 字。'),
    entry('D100.0', '[Bool] 数据存储区位', true, false, '位号 0–15；连续位访问可跨字。'),
    entry('CIO100 / C100', '[Word] CIO 区', false, true, 'C 是 CIO 的别名，不是计数器。'),
    entry('CIO100.15 / C100.15', '[Bool] CIO 区位', true, false, '以第 100 字第 15 位为起点；省略位号时 Bool 默认第 0 位。'),
    entry('W100 / W100.0', '[Word/Bool] 工作区', true, true, 'W / WR 为别名；数值不允许使用位后缀。'),
    entry('H100 / H100.0', '[Word/Bool] 保持区', true, true, 'H / HR 为别名；字和位偏移都是十进制。'),
    entry('A100 / A100.0', '[Word/Bool] 辅助区', true, true, 'A / AR 为别名；可写范围由设备和组态决定，写入前核对。'),
    entry('E0.100 / EF.100 / E10.100', '[Word] EM bank', false, true, 'bank 使用十六进制 0–18；字偏移为十进制，实际 bank 取决于型号。'),
    entry('E0.100.15', '[Bool] EM bank 位', true, false, 'E / EM 为别名；最后一段为 0–15 的位号。'),
    entry('D200', '[Byte/String] 连续字节或文本', false, true, '长度按字节，无长度头；奇数字节写入会读改写末字以保留邻接字节。'),
  ]
  if (protocol.startsWith('modbus_')) return [
    entry('100 / C100', '[Bool] 线圈', true, false, 'Bool 读取默认线圈区；FC01 读取，FC05/0F 写入。'),
    entry('DI100', '[Bool] 离散输入', true, false, 'FC02，只读；不要把 x=2 当作功能码。'),
    entry('100 / HR100', '[Word] 保持寄存器', false, true, '数值类型默认 HR；FC03 读取，FC06/10 写入。'),
    entry('IR100', '[Word] 输入寄存器', false, true, 'FC04，只读；支持数值和原始字符串读取。'),
    entry('HR256', '[Word] 十进制偏移', false, true, '十六进制 100H = 256；当前库需填写 HR256，不接受 H 后缀。'),
    entry('x=2;C100', '[Bool] 指定站号线圈', true, false, '仅本次读写使用站号 2，不修改连接默认站号。'),
    entry('x=2;DI100', '[Bool] 指定站号输入', true, false, '站号 2 的离散输入，只读。'),
    entry('x=2;100', '[Bool/Word] 指定站号', true, true, 'Bool 访问线圈；数值或字符串访问 HR。x 是站号，不是功能码。'),
    entry('x=2;HR100', '[Word] 指定站号保持寄存器', false, true, 'TCP/UDP 站号 1–255；RTU/ASCII 站号 1–247。'),
    entry('x=2;IR100', '[Word] 指定站号输入寄存器', false, true, '站号 2 的输入寄存器，只读。'),
    entry('HR100 / IR100', '[String] 原始 UTF-8', false, true, '长度按字节；仅 HR 可写；无长度头，不自动清理尾部。'),
  ]
  if (protocol === 's7') return [
    entry('DB1.100.3 / DB1.DBX100.3', '[Bool] 数据块位', true, false, 'DB1 第 100 字节第 3 位；位号范围 0–7。'),
    entry('DB1.100', '[Bool/数值] 数据块简写', true, true, 'Bool 默认第 0 位；数值以第 100 字节为起点。'),
    entry('DB1.DBB100', '[Byte] 数据块字节', false, true, '字节偏移 100；使用 Byte/SByte。'),
    entry('DB1.DBW100', '[Word] 数据块字', false, true, '字节偏移 100；数值类型决定每元素宽度。'),
    entry('DB1.DBD100', '[DWord] 数据块双字', false, true, '字节偏移 100；常用 Int/UInt/Float。'),
    entry('M100.3 / M100', '[Bool/数值] 标志区', true, true, '带 .3 用 Bool；无位号可读数值或默认第 0 位。'),
    entry('I0.0 / E0.0', '[Bool] 输入区', true, false, 'I/E 为别名；实际可访问范围由 PLC 组态决定。'),
    entry('Q0.0 / A0.0', '[Bool] 输出区', true, false, 'Q/A 为别名；写入前确认 PLC 控制逻辑。'),
    entry('V100 / V100.0', '[Bool/数值] V 区', true, true, '库映射到 DB1；用于具有相应 V 区映射的设备。'),
    entry('DB1.100', '[String / S7 STRING] 文本', false, true, 'String 指向正文并按字节读；S7 STRING 指向长度头，不填长度。'),
  ]
  const entries = [
    entry('M100', '[Bool] 内部继电器', true, false, '十进制；Bool 可从任意位读取。字访问位设备需 16 位对齐。'),
    entry('X10 / Y10', '[Bool] 输入 / 输出', true, false, '十六进制编号，10 表示 16；不要附加 H。'),
    entry('B10', '[Bool] 链接继电器', true, false, '十六进制编号。'),
    entry('D100', '[Word] 数据寄存器', false, true, '十进制编号；数值长度按元素，原始文本长度按字节。'),
    entry('W10', '[Word] 链接寄存器', false, true, '十六进制编号，W10 表示偏移 16；W10H 不支持。'),
    entry('R100', '[Word] 文件寄存器', false, true, '十进制；具体设备范围由型号及协议决定。'),
    entry('TS0 / TC0 / TN0', '[Bool/Word] 定时器', true, true, 'TS 接点、TC 线圈使用 Bool；TN 当前值使用数值。'),
    entry('CS0 / CC0 / CN0', '[Bool/Word] 计数器', true, true, 'CS 接点、CC 线圈使用 Bool；CN 当前值使用数值。'),
  ]
  if (!protocol.startsWith('a1e_')) entries.push(entry('SM0 / SD0', '[Bool/Word] 特殊设备', true, true, 'SM 使用 Bool，SD 使用数值；A1E 批量访问不支持。'), entry('SB10 / SW10', '[Bool/Word] 特殊链接', true, true, '十六进制；SB 为位、SW 为字。'))
  if (protocol === 'mc_r_binary') entries.push(entry('RD100', '[Word] iQ-R 扩展设备', false, true, '仅 MC-R 二进制协议支持；十进制编号。'))
  return entries
}

export function addressNotes(protocol: PlcProtocol, series: InovanceSeries = 'AM'): string {
  if (protocol === 'omron_cip') return 'EtherNetIP 使用符号标签，固定小端。支持原子类型/数组及 Omron STRING（0x00D0）读取；字符串数组逐元素读取，并非原子快照。不支持 STRING 写入、UDT、打包 BOOL 数组、隐式 UDP I/O。路由在连接参数中设置；未做实机兼容性认证。'
  if (isCipProtocol(protocol)) return 'EtherNetIP 使用符号标签，固定小端。支持原子类型/数组；不支持 STRING/UDT、打包 BOOL 数组、隐式 UDP I/O。路由在连接参数中设置。写入逐段确认，但不提供原子事务或自动重试；未做实机兼容性认证。'
  if (isInovanceProtocol(protocol)) return `${series} · 内置 Modbus TCP，默认数值字序 CDAB；AM/AC/AP 共用映射，EVO 不套用 SM/SD 扩展。字节解析不等于数值字序，MB 低字节在先。无 S7 STRING、EasyNet 或 EtherNet/IP；写前核对地址及型号。`
  if (protocol.startsWith('omron_')) return 'FINS 默认 CDAB，可在连接配置中修改。支持 D/CIO/W/H/A/EM；不支持三菱 M 地址、TIM/CNT、标签或 S7 STRING。批量字节解析按大端显示，32/64 位数值字序请与主读取核对；不是网络线帧。'
  if (protocol.startsWith('modbus_')) return '当前库与截图语法不同：x= 表示站号；不支持 s=、format=、w=、file=、100.1、H 后缀或自定义功能码。字节序在连接配置中设置；地址从 0 开始，40001 不会自动转成 HR0。'
  if (protocol === 's7') return '偏移按字节，不按寄存器；长度按元素（String 按字节）。位/字仅表示访问形式，不代表所有型号均可访问。'
  return '不支持 D100.1 位后缀。十六进制设备编号直接填写，不加 H。地址示例不保证设备存在，写入前核对型号、组态和容量。'
}
