export type PlcProtocol =
    's7'
    | 'mc_binary'
    | 'mc_ascii'
    | 'mc_udp_binary'
    | 'mc_udp_ascii'
    | 'a1e_binary'
    | 'a1e_ascii'
    | 'mc_r_binary'
    | 'modbus_tcp'
    | 'modbus_udp'
    | 'modbus_rtu'
    | 'modbus_ascii'
    | 'omron_fins_tcp'
    | 'omron_fins_udp'
    | 'inovance_modbus_tcp'
export type InovanceSeries = 'AM' | 'AC' | 'AP' | 'EVO' | 'H3U' | 'H5U' | 'Easy'
export interface InovanceOptions {
    series: InovanceSeries;
    unitId: number;
    byteOrder: ModbusByteOrder
}
export type CpuModel = 'S1200' | 'S1500' | 'S300' | 'S400' | 'S200' | 'S200Smart'
export type DataType =
    'bool'
    | 'u8'
    | 'i8'
    | 'u16'
    | 'i16'
    | 'u32'
    | 'i32'
    | 'u64'
    | 'i64'
    | 'f32'
    | 'f64'
    | 'raw_string'
    | 's7_string'

export interface MelsecOptions {
    networkNumber: number;
    pcNumber: number;
    ioNumber: number;
    stationNumber: number;
    monitoringTimer: number
}

export type ModbusByteOrder = 'ABCD' | 'BADC' | 'CDAB' | 'DCBA'

export interface ModbusOptions {
    unitId: number;
    byteOrder: ModbusByteOrder
}

export interface OmronOptions {
    sourceNode: number;
    destinationNode: number;
    sourceNetwork: number;
    destinationNetwork: number;
    sourceUnit: number;
    destinationUnit: number;
    gatewayCount: number;
    byteOrder: ModbusByteOrder
}

export interface SerialOptions {
    path: string;
    baudRate: number;
    dataBits: 7 | 8;
    parity: 'none' | 'odd' | 'even';
    stopBits: 1 | 2
}

export interface ConnectRequest {
    protocol: PlcProtocol
    host: string
    port: number
    cpu: CpuModel
    rack: number
    slot: number
    connectTimeoutMs: number
    receiveTimeoutMs: number
    localTsap: string | null
    remoteTsap: string | null
    melsec: MelsecOptions
    modbus: ModbusOptions
    omron: OmronOptions
    inovance: InovanceOptions
    serial: SerialOptions
}

export interface ConnectionStatus {
    connected: boolean;
    protocol: PlcProtocol | null;
    cpu: string | null;
    pduLength: number | null
    inovanceSeries?: InovanceSeries | null
}

export interface ReadRequest {
    address: string;
    dataType: DataType;
    length?: number
}

export interface WriteRequest {
    address: string;
    dataType: DataType;
    mode: 'single' | 'array';
    values: string[];
    confirmed: boolean
}

export interface ReadResponse {
    values: string[];
    elapsedMs: number
}

export interface WriteResponse {
    count: number;
    unit: string;
    elapsedMs: number
}

export interface ReadResult extends ReadResponse {
    address: string;
    dataType: DataType;
    time: string
}

export interface LogEntry {
    id: number;
    time: string;
    level: 'ok' | 'error' | 'info';
    message: string
}

export const dataTypes: { value: DataType; label: string }[] = [
    {value: 'bool', label: 'bool · 位'},
    ...['u8', 'i8', 'u16', 'i16', 'u32', 'i32', 'u64', 'i64', 'f32', 'f64'].map(value => ({
        value: value as DataType,
        label: value
    })),
    {value: 'raw_string', label: 'String · 原始 UTF-8'},
    {value: 's7_string', label: 'STRING · S7 长度头'},
]

export const protocolLabels: Record<PlcProtocol, string> = {
    s7: 'S7 / TCP',
    mc_binary: 'MC / Binary TCP',
    mc_ascii: 'MC / ASCII TCP',
    mc_udp_binary: 'MC / Binary UDP',
    mc_udp_ascii: 'MC / ASCII UDP',
    a1e_binary: 'A1E / Binary TCP',
    a1e_ascii: 'A1E / ASCII TCP',
    mc_r_binary: 'MC-R / Binary TCP',
    modbus_tcp: 'Modbus / TCP',
    modbus_udp: 'Modbus / UDP',
    modbus_rtu: 'Modbus RTU / 串口',
    modbus_ascii: 'Modbus ASCII / 串口',
    omron_fins_tcp: 'FINS / TCP',
    omron_fins_udp: 'FINS / UDP',
    inovance_modbus_tcp: '汇川 Modbus / TCP',
}

export function isModbusProtocol(protocol: PlcProtocol): boolean {
    return protocol.startsWith('modbus_')
}

export function isSerialProtocol(protocol: PlcProtocol): boolean {
    return protocol === 'modbus_rtu' || protocol === 'modbus_ascii'
}

export function isOmronProtocol(protocol: PlcProtocol): boolean {
    return protocol === 'omron_fins_tcp' || protocol === 'omron_fins_udp'
}

export function availableDataTypes(protocol: PlcProtocol) {
    return dataTypes.filter(option => {
        if (isModbusProtocol(protocol)) return !['u8', 'i8', 's7_string'].includes(option.value)
        return protocol === 's7' || option.value !== 's7_string'
    })
}

export function isInovanceProtocol(protocol: PlcProtocol): boolean {
    return protocol === 'inovance_modbus_tcp'
}

export function isInovanceIecSeries(series: InovanceSeries): boolean {
    return ['AM', 'AC', 'AP', 'EVO'].includes(series)
}

export function defaultAddress(protocol: PlcProtocol, type: DataType, series: InovanceSeries = 'AM'): string {
    if (isInovanceProtocol(protocol)) return isInovanceIecSeries(series)
        ? type === 'bool' ? 'MX100.0' : 'MW100'
        : type === 'bool' ? 'M100' : 'D100'
    if (isModbusProtocol(protocol)) return type === 'bool' ? 'C0' : 'HR0'
    if (protocol === 's7') return type === 'bool' ? 'DB1.0.0' : 'DB1.0'
    if (isOmronProtocol(protocol)) return type === 'bool' ? 'CIO100.0' : 'D100'
    return type === 'bool' ? 'M100' : 'D100'
}

export function isReadOnlyModbusAddress(address: string): boolean {
    return /^(?:x\s*=\s*\d+\s*;\s*)?(DI|IR)\d+$/i.test(address.trim())
}

export function isReadOnlyAddress(protocol: PlcProtocol, address: string): boolean {
    if (isModbusProtocol(protocol)) return isReadOnlyModbusAddress(address)
    return isInovanceProtocol(protocol) && /^(?:[xs]\s*=\s*\d+\s*;\s*)?%?(?:IX|I|X)\d+(?:\.\d+)?$/i.test(address.trim())
}
