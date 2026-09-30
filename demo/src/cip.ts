import type { CipOptions } from './types.ts'

export function parseCipRoute(text: string): number[] {
  if (!text.trim()) return []
  const parts = text.trim().split(/[,\s]+/)
  if (parts.length > 500 || parts.length % 2 !== 0) throw new Error('CIP 路由必须是偶数字节，最多 500 字节；直连时留空。')
  return parts.map(part => {
    if (!/^(?:0x[\da-f]{1,2}|\d{1,3})$/i.test(part)) throw new Error('CIP 路由请输入十进制或 0x 十六进制字节，以逗号或空格分隔。')
    const value = Number(part)
    if (value > 255) throw new Error('CIP 路由字节范围为 0–255。')
    return value
  })
}

export function validateCipOptions(options: CipOptions): void {
  if (typeof options.connected !== 'boolean') throw new Error('请选择 CIP 连接模式。')
  const maximum = options.connected ? 4000 : 504
  if (!Number.isInteger(options.connectionSize) || options.connectionSize < 128 || options.connectionSize > maximum)
    throw new Error(`CIP 报文大小必须为 128–${maximum} 字节。`)
  if (!Number.isInteger(options.packetIntervalUs) || options.packetIntervalUs < 1 || options.packetIntervalUs > 0xffffffff)
    throw new Error('CIP RPI 必须为 1–4294967295 微秒。')
  if (!Number.isInteger(options.timeoutMultiplier) || options.timeoutMultiplier < 0 || options.timeoutMultiplier > 7)
    throw new Error('CIP 超时倍率范围为 0–7。')
  if (!Array.isArray(options.route) || options.route.length > 500 || options.route.length % 2 !== 0
    || options.route.some(value => !Number.isInteger(value) || value < 0 || value > 255))
    throw new Error('CIP 路由必须是偶数字节，单字节范围为 0–255。')
}
