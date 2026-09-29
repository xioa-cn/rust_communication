import type {ConnectRequest, ReadRequest, WriteRequest} from './types'
import type {TextEncoding} from './workbench'

export type RustOperation =
    | { kind: 'connect' }
    | { kind: 'read'; request: ReadRequest; text?: { encoding: TextEncoding; reverse: boolean } }
    | { kind: 'write'; request: WriteRequest }

const melsecClients = {
    mc_binary: 'MelsecMcNet',
    mc_ascii: 'MelsecMcAsciiNet',
    mc_udp_binary: 'MelsecMcUdp',
    mc_udp_ascii: 'MelsecMcAsciiUdp',
    a1e_binary: 'MelsecA1ENet',
    a1e_ascii: 'MelsecA1EAsciiNet',
    mc_r_binary: 'MelsecMcRNet',
} as const

export function rustString(value: string): string {
    return '"' + [...value].map(character => {
        if (character === '"') return '\\"'
        if (character === '\\') return '\\\\'
        const point = character.codePointAt(0)!
        return point < 32 || point === 127 || (point >= 0xd800 && point <= 0xdfff)
            ? `\\u{${point >= 0xd800 && point <= 0xdfff ? 'fffd' : point.toString(16)}}` : character
    }).join('') + '"'
}

function createPlc(config: ConnectRequest) {
    const imports: string[] = []
    const statements: string[] = []
    const serial = config.protocol === 'modbus_rtu' || config.protocol === 'modbus_ascii'
    if (serial) {
        const client = config.protocol === 'modbus_rtu' ? 'ModbusRtu' : 'ModbusAscii'
        imports.push(`use rs_appliaction::communication::modbus::{${client}, ByteOrder};`, 'use std::time::Duration;')
        statements.push(`let port = serialport::new(${rustString(config.serial.path.trim())}, ${config.serial.baudRate})`,
            `    .data_bits(serialport::DataBits::${config.serial.dataBits === 7 ? 'Seven' : 'Eight'})`,
            `    .parity(serialport::Parity::${{none: 'None', odd: 'Odd', even: 'Even'}[config.serial.parity]})`,
            `    .stop_bits(serialport::StopBits::${config.serial.stopBits === 2 ? 'Two' : 'One'})`,
            '    .flow_control(serialport::FlowControl::None)',
            `    .timeout(Duration::from_millis(${config.receiveTimeoutMs}))`,
            '    .dtr_on_open(false)', '    .open()?;',
            `let mut plc = ${client}::new(port, ${config.modbus.unitId}${config.protocol === 'modbus_rtu' ? ', ' + config.serial.baudRate : ''});`)
    } else {
        imports.push('use rs_appliaction::communication::timeout::Timeout;', 'use std::net::IpAddr;')
        statements.push(`let address: IpAddr = ${rustString(config.host.trim())}.parse()?;`,
            `let timeout = Timeout::new(${config.connectTimeoutMs}, ${config.receiveTimeoutMs});`)
        if (config.protocol === 's7') {
            imports.unshift('use rs_appliaction::communication::s7::{s7_net::S7Net, s7_type::S7Type};')
            statements.push(`let mut plc = S7Net::new(address, ${config.port}, S7Type::${config.cpu}, timeout, ${config.rack}, ${config.slot});`)
            const local = config.localTsap?.trim(), remote = config.remoteTsap?.trim()
            if (local && remote) statements.push(`plc = plc.with_tsap(u16::from_str_radix(${rustString(local.replace(/^0x/i, ''))}, 16)?, u16::from_str_radix(${rustString(remote.replace(/^0x/i, ''))}, 16)?);`)
        } else if (config.protocol === 'inovance_modbus_tcp') {
            imports.unshift('use rs_appliaction::communication::inovace::{InovanceModbusTcp, InovanceType, ByteOrder};')
            statements.push(`let mut plc = InovanceModbusTcp::new(address, ${config.port}, InovanceType::${config.inovance.series}, timeout);`,
                `plc.set_unit_id(${config.inovance.unitId}).to_result()?;`,
                `plc.set_byte_order(ByteOrder::${config.inovance.byteOrder});`)
        } else if (config.protocol === 'omron_fins_tcp' || config.protocol === 'omron_fins_udp') {
            const client = config.protocol === 'omron_fins_tcp' ? 'OmronFinsTcp' : 'OmronFinsUdp'
            imports.unshift(`use rs_appliaction::communication::omron::{${client}, FinsRoute, ByteOrder};`)
            statements.push(`let mut plc = ${client}::new(address, ${config.port}, timeout);`,
                'plc.set_route(FinsRoute {',
                `    source_node: ${config.omron.sourceNode}, destination_node: ${config.omron.destinationNode},`,
                `    source_network: ${config.omron.sourceNetwork}, destination_network: ${config.omron.destinationNetwork},`,
                `    source_unit: ${config.omron.sourceUnit}, destination_unit: ${config.omron.destinationUnit},`,
                `    gateway_count: ${config.omron.gatewayCount},`,
                '}).to_result()?;', `plc.set_byte_order(ByteOrder::${config.omron.byteOrder});`)
        } else if (config.protocol === 'modbus_tcp' || config.protocol === 'modbus_udp') {
            const client = config.protocol === 'modbus_tcp' ? 'ModbusTcp' : 'ModbusUdp'
            imports.unshift(`use rs_appliaction::communication::modbus::{${client}, ByteOrder};`)
            statements.push(`let mut plc = ${client}::new(address, ${config.port}, timeout);`)
        } else {
            const client = melsecClients[config.protocol as keyof typeof melsecClients]
            imports.unshift(`use rs_appliaction::communication::melsec::${client};`)
            statements.push(`let mut plc = ${client}::new(address, ${config.port}, timeout)`,
                `    .with_route(${config.melsec.networkNumber}, ${config.melsec.pcNumber}, ${config.melsec.ioNumber}, ${config.melsec.stationNumber})`,
                `    .with_monitoring_timer(${config.melsec.monitoringTimer});`)
        }
    }
    if (config.protocol.startsWith('modbus_')) statements.push(`plc.set_unit_id(${config.modbus.unitId}).to_result()?;`, `plc.set_byte_order(ByteOrder::${config.modbus.byteOrder});`)
    return {imports, statements, serial}
}

function decodeBytes(text: { encoding: TextEncoding; reverse: boolean }): string[] {
    const lines = ['let mut bytes = values.into_vec();']
    if (text.reverse) lines.push('for pair in bytes.chunks_exact_mut(2) { pair.swap(0, 1); }')
    if (text.encoding === 'ascii') lines.push('if !bytes.is_ascii() { return Err("数据包含非 ASCII 字节".into()); }')
    if (text.encoding === 'utf-16le' || text.encoding === 'utf-16be') {
        lines.push('if bytes.len() % 2 != 0 { return Err("UTF-16 字节数必须为偶数".into()); }',
            `let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| u16::from_${text.encoding === 'utf-16le' ? 'le' : 'be'}_bytes([pair[0], pair[1]])).collect();`,
            'let text = String::from_utf16(&units)?;')
    } else lines.push('let text = String::from_utf8(bytes)?;')
    lines.push('println!("读取文本: {}", text);')
    return lines
}

function operationCode(operation: RustOperation): string[] {
    if (operation.kind === 'connect') return ['plc.connect().to_result()?;', 'println!("PLC 连接成功（UDP 仅表示本地通道已就绪）");']
    const request = operation.request, address = rustString(request.address.trim())
    const type = ['raw_string', 's7_string'].includes(request.dataType) ? 'String' : request.dataType
    if (operation.kind === 'read') {
        const call = request.dataType === 's7_string' ? `plc.read_s7_strings(${address})` : `plc.read::<${type}>(${address}, ${operation.request.length ?? 1})`
        if (operation.text && request.dataType === 'u8') return [`let values = ${call}.to_result()?;`, ...decodeBytes(operation.text)]
        return [`match ${call}.to_result() {`, `    Ok(values) => println!("读取 {} 成功: {:?}", ${address}, values),`, `    Err(error) => eprintln!("读取 {} 失败: {}", ${address}, error),`, '}']
    }
    const values = operation.request.values.map(value => type === 'String' ? `${rustString(value)}.to_string()` : `${rustString(value)}.parse::<${type}>()?`)
    const lines = ['let confirmed = false;', 'if !confirmed { return Err("请核对地址和值后，手动确认写入".into()); }']
    if (operation.request.mode === 'array') lines.push(`let values: Vec<${type}> = vec![${values.join(', ')}];`, `let result = plc.write_all::<${type}>(${address}, &values).to_result()?;`)
    else lines.push(`let value: ${type} = ${values[0]};`, `let result = plc.write::<${type}>(${address}, value).to_result()?;`)
    lines.push('println!("写入成功: {:?}", result);')
    return lines
}

export function generateRustExample(config: ConnectRequest, operation: RustOperation = {kind: 'connect'}) {
    const creation = createPlc(config), operationLines = operationCode(operation)
    const body = [...creation.statements, '', ...(operation.kind === 'connect' ? [] : ['plc.connect().to_result()?;', '']), ...operationLines, '', 'plc.disconnect().to_result()?;', 'Ok(())']
    return {
        creation: [...creation.imports, '', ...creation.statements].join('\n'),
        operation: operationLines.join('\n'),
        source: [...creation.imports, '', 'fn main() -> Result<(), Box<dyn std::error::Error>> {', ...body.map(line => line ? '    ' + line : ''), '}', ''].join('\n'),
        title: operation.kind === 'connect' ? '连接 PLC' : `${operation.kind === 'read' ? '读取' : '写入'} ${operation.request.address} · ${operation.request.dataType}`,
        dependency: creation.serial ? 'rs_appliaction（本地路径依赖）+ serialport 4' : 'rs_appliaction（本地路径依赖）',
    }
}
