import type {DataType, PlcProtocol, ReadRequest} from './types'
import {availableDataTypes, defaultAddress} from './types.ts'

export type NumberDisplay = 'dec' | 'hex' | 'bin'
export type TextEncoding = 'utf-8' | 'ascii' | 'utf-16le' | 'utf-16be'

export interface Sample {
    id: number;
    time: string;
    protocol: PlcProtocol | null;
    device: string;
    endpoint: string;
    address: string;
    dataType: DataType;
    values: string[];
    elapsedMs: number
}

export interface Point {
    id: number;
    name: string;
    address: string;
    dataType: DataType;
    length: number;
    value: string;
    elapsedMs: number | null;
    error: string
}

export const quickTypes: { label: string; value: DataType }[] = [
    {label: 'Bool', value: 'bool'}, {label: 'Byte', value: 'u8'},
    {label: 'SByte', value: 'i8'},
    {label: 'Short', value: 'i16'}, {label: 'UShort', value: 'u16'},
    {label: 'Int', value: 'i32'}, {label: 'UInt', value: 'u32'},
    {label: 'Long', value: 'i64'}, {label: 'ULong', value: 'u64'},
    {label: 'Float', value: 'f32'}, {label: 'Double', value: 'f64'},
    {label: 'String', value: 'raw_string'}, {label: 'S7 STRING', value: 's7_string'},
]

export function parseValues(input: string, type: DataType): string[] {
    if (type === 'raw_string' || type === 's7_string') return [input]
    const text = input.trim()
    const sequence = /^\[(-?\d+):(-?\d+)\]$/.exec(text)
    const repeated = /^\[([^\[\]*]+)\*(\d+)\]$/.exec(text)
    let values: string[]
    if (sequence) {
        const start = BigInt(sequence[1]), end = BigInt(sequence[2])
        const length = Number((end >= start ? end - start : start - end) + 1n)
        if (length > 1024) throw new Error('连续数组最多 1024 项。')
        values = Array.from({length}, (_, index) => String(start + BigInt(index) * (end >= start ? 1n : -1n)))
    } else if (repeated) {
        const length = Number(repeated[2])
        if (!Number.isInteger(length) || length < 1 || length > 1024) throw new Error('重复次数必须为 1–1024。')
        values = Array.from({length}, () => repeated[1].trim())
    } else values = text.replace(/^\[/, '').replace(/\]$/, '').split(/[,;\s]+/).filter(Boolean)
    if (!values.length || values.length > 1024) throw new Error('请输入 1–1024 个值。')
    if (type === 'bool') return values.map(value => {
        if (/^(true|1)$/i.test(value)) return 'true'
        if (/^(false|0)$/i.test(value)) return 'false'
        throw new Error('Bool 仅支持 true / false / 1 / 0。')
    })
    const integer = /^([ui])(8|16|32|64)$/.exec(type)
    return values.map(value => {
        if (integer) {
            if (!/^[+-]?\d+$/.test(value)) throw new Error(`${type} 需要十进制整数。`)
            const bits = BigInt(integer[2]), number = BigInt(value), signed = integer[1] === 'i'
            const minimum = signed ? -(1n << (bits - 1n)) : 0n
            const maximum = signed ? (1n << (bits - 1n)) - 1n : (1n << bits) - 1n
            if (number < minimum || number > maximum) throw new Error(`${value} 超出 ${type} 的范围。`)
            return String(number)
        }
        if (!/^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i.test(value) || !Number.isFinite(Number(value)) || (type === 'f32' && !Number.isFinite(Math.fround(Number(value))))) throw new Error('浮点数必须为有限十进制数值。')
        return value
    })
}

export function displayValue(value: string, mode: NumberDisplay): string {
    if (mode === 'dec' || !/^-?\d+$/.test(value)) return value
    const number = BigInt(value), sign = number < 0n ? '-' : '', magnitude = number < 0n ? -number : number
    return sign + (mode === 'hex' ? '0x' : '0b') + magnitude.toString(mode === 'hex' ? 16 : 2).toUpperCase()
}

export function parseHex(text: string): Uint8Array {
    const value = text.trim().replace(/0x/gi, '').replace(/[\s,;:-]/g, '')
    if (!value || value.length % 2 || !/^[\da-f]+$/i.test(value)) throw new Error('请输入完整的 HEX 字节，例如 01 02 0A FF。')
    if (value.length > 2048) throw new Error('最多处理 1024 字节。')
    return Uint8Array.from(value.match(/../g)!, byte => Number.parseInt(byte, 16))
}

export function reversePairs(bytes: Uint8Array): Uint8Array {
    const result = bytes.slice()
    for (let index = 0; index + 1 < result.length; index += 2) [result[index], result[index + 1]] = [result[index + 1], result[index]]
    return result
}

export function encodeText(text: string, encoding: TextEncoding): Uint8Array {
    if (encoding === 'ascii') {
        if ([...text].some(char => char.codePointAt(0)! > 127)) throw new Error('ASCII 只能写入 0–127 范围内的字符。')
        return Uint8Array.from([...text], char => char.charCodeAt(0))
    }
    if (encoding === 'utf-8') return new TextEncoder().encode(text)
    const bytes = new Uint8Array(text.length * 2), view = new DataView(bytes.buffer)
    for (let index = 0; index < text.length; index++) view.setUint16(index * 2, text.charCodeAt(index), encoding === 'utf-16le')
    return bytes
}

export function decodeText(bytes: Uint8Array, encoding: TextEncoding): string {
    if (encoding === 'ascii' && bytes.some(byte => byte > 127)) throw new Error('返回值包含非 ASCII 字节。')
    if (encoding.startsWith('utf-16') && bytes.length % 2) throw new Error('UTF-16 字节数必须为偶数。')
    return new TextDecoder(encoding === 'ascii' ? 'utf-8' : encoding, {fatal: true}).decode(bytes)
}

export function parseBatch(text: string, protocol: PlcProtocol): ReadRequest[] {
    const rows = text.split(/\r?\n/).map(row => row.trim()).filter(Boolean)
    if (!rows.length || rows.length > 100) throw new Error('请输入 1–100 行地址。每行：地址,类型,数量。')
    return rows.map((row, index) => {
        const columns = row.split(',').map(value => value.trim())
        const [address, type = 'u16', count = '1'] = columns
        const dataType = type as DataType, length = Number(count)
        if (columns.length > 3 || !address || new TextEncoder().encode(address).length > 128 || !availableDataTypes(protocol).some(option => option.value === dataType)) throw new Error(`第 ${index + 1} 行的地址或类型无效。`)
        if (!Number.isInteger(length) || length < 1 || length > 1024) throw new Error(`第 ${index + 1} 行数量必须为 1–1024。`)
        return {address, dataType, ...(dataType === 's7_string' ? {} : {length})}
    })
}

export function csvCell(value: unknown): string {
    let text = String(value ?? '')
    if (/^[\s]*[=+\-@\t\r]/.test(text)) text = "'" + text
    return '"' + text.replace(/"/g, '""') + '"'
}

export function samplesToCsv(samples: Sample[]): string {
    return '\uFEFF' + [['时间', '协议', '设备', '连接端点', '地址', '类型', '值', '耗时(ms)'], ...samples.map(sample => [sample.time, sample.protocol, sample.device, sample.endpoint, sample.address, sample.dataType, sample.values.join('; '), sample.elapsedMs])].map(row => row.map(csvCell).join(',')).join('\r\n')
}

export function downloadText(filename: string, content: string, type = 'text/plain;charset=utf-8') {
    const url = URL.createObjectURL(new Blob([content], {type})), anchor = document.createElement('a')
    anchor.href = url
    anchor.download = filename
    anchor.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export function exampleRequests(protocol: PlcProtocol): ReadRequest[] {
    return [{
        address: defaultAddress(protocol, 'u16'),
        dataType: 'u16',
        length: 10
    }, {address: defaultAddress(protocol, 'bool'), dataType: 'bool', length: 1}]
}

export function createSerialRunner(onState: (running: boolean) => void) {
    let running = false, stopped = false
    let wake: (() => void) | undefined
    return {
        get running() {
            return running
        },
        get stopping() {
            return stopped
        },
        stop() {
            stopped = true;
            wake?.()
        },
        async start(job: (index: number) => Promise<boolean | void>, interval: number, count = 0) {
            if (running) throw new Error('已有任务正在运行。')
            if (!Number.isFinite(interval) || interval < 0 || interval > 60000 || !Number.isInteger(count) || count < 0 || count > 10000) throw new Error('任务参数无效。')
            running = true
            stopped = false
            onState(true)
            try {
                for (let index = 0; !stopped && (!count || index < count); index++) {
                    if (await job(index) === false) break
                    if (stopped || (count && index + 1 >= count)) break
                    await new Promise<void>(resolve => {
                        const timer = setTimeout(() => {
                            wake = undefined;
                            resolve()
                        }, interval)
                        wake = () => {
                            clearTimeout(timer);
                            wake = undefined;
                            resolve()
                        }
                    })
                }
            } finally {
                running = false;
                stopped = true;
                onState(false)
            }
        },
    }
}
