import { isInovanceProtocol, isModbusProtocol, isReadOnlyAddress } from './types.ts'
import type { PlcProtocol, ReadRequest, WriteRequest } from './types.ts'
import { decodeText, parseHex, reversePairs } from './workbench.ts'
import type { TextEncoding } from './workbench.ts'

export type BatchFormat = 'hex' | 'u16' | 'i16' | 'u32' | 'i32' | 'f32' | 'f64' | TextEncoding

export function batchRequest(protocol: PlcProtocol, address: string, length: number): ReadRequest {
  const target = address.trim()
  if (!target || new TextEncoder().encode(target).length > 128) throw new Error('地址不能为空，且最多 128 字节。')
  const inovanceTarget = target.replace(/^[xs]\s*=\s*\d+\s*;\s*/i, '').replace(/^%/, '')
  const inovanceBit = isInovanceProtocol(protocol) && /^(?:(?:M|SM|S|B|X|Y|I|IX|Q|QX)\d+(?:\.\d+)?|(?:MX|MW|MD|D|R|SD|SDW|T|C)\d+\.\d+)$/i.test(inovanceTarget)
  const dataType = inovanceBit ? 'bool' : !isModbusProtocol(protocol) ? 'u8' : /^(?:x\s*=\s*\d+\s*;\s*)?(?:C|DI)\d+$/i.test(target) ? 'bool' : 'u16'
  const maximum = dataType === 'u16' ? 512 : 1024
  if (!Number.isInteger(length) || length < 1 || length > maximum) throw new Error(`长度必须为 1–${maximum}；寄存器每项 2 字节。`)
  return { address: target, dataType, length }
}

export function batchBytes(request: ReadRequest, values: string[]): Uint8Array {
  if (request.dataType === 'bool') return Uint8Array.from(values, value => value === 'true' ? 1 : 0)
  if (request.dataType === 'u16') return Uint8Array.from(values.flatMap(value => [Number(value) >> 8, Number(value) & 255]))
  return Uint8Array.from(values, Number)
}

export function formatBatch(bytes: Uint8Array, format: BatchFormat, reverse: boolean, perLine: number, littleEndian = false): string {
  if (!Number.isInteger(perLine) || perLine < 1 || perLine > 64) throw new Error('每行数量必须为 1–64。')
  const buffer = reverse ? reversePairs(bytes) : bytes
  if (['ascii', 'utf-8', 'utf-16le', 'utf-16be'].includes(format)) return decodeText(buffer, format as TextEncoding)
  const width = format === 'hex' ? 1 : format === 'f64' ? 8 : ['u16', 'i16'].includes(format) ? 2 : 4
  if (buffer.length % width) throw new Error(`${format} 解析要求字节数为 ${width} 的倍数。`)
  const view = new DataView(buffer.buffer, buffer.byteOffset, buffer.byteLength), values: string[] = []
  for (let offset = 0; offset < buffer.length; offset += width) {
    const value = format === 'hex' ? buffer[offset].toString(16).padStart(2, '0').toUpperCase()
      : format === 'u16' ? view.getUint16(offset, littleEndian) : format === 'i16' ? view.getInt16(offset, littleEndian)
      : format === 'u32' ? view.getUint32(offset, littleEndian) : format === 'i32' ? view.getInt32(offset, littleEndian)
      : format === 'f32' ? view.getFloat32(offset, littleEndian) : view.getFloat64(offset, littleEndian)
    values.push(String(value))
  }
  const lines: string[] = []
  for (let offset = 0; offset < values.length; offset += perLine) lines.push(values.slice(offset, offset + perLine).join(' '))
  return lines.join('\n')
}

export function batchWriteRequest(protocol: PlcProtocol, request: ReadRequest, originalLength: number, text: string): WriteRequest {
  if (isReadOnlyAddress(protocol, request.address)) throw new Error('该输入地址为只读区域，不能回写。')
  const bytes = parseHex(text)
  if (bytes.length !== originalLength) throw new Error('回写字节数必须与本次读取一致，避免覆盖相邻地址。')
  if (request.dataType === 'u16' && bytes.length % 2) throw new Error('寄存器回写需要完整的双字节数据。')
  if (request.dataType === 'bool' && bytes.some(value => value !== 0 && value !== 1)) throw new Error('线圈回写仅支持 00 / 01。')
  const values = request.dataType === 'u16'
    ? Array.from({ length: bytes.length / 2 }, (_, index) => String((bytes[index * 2] << 8) | bytes[index * 2 + 1]))
    : [...bytes].map(value => request.dataType === 'bool' ? String(Boolean(value)) : String(value))
  return { address: request.address, dataType: request.dataType, mode: values.length === 1 ? 'single' : 'array', values, confirmed: true }
}
