import type { ConnectRequest, ConnectionStatus, CpuModel, InovanceSeries, PlcProtocol } from './types'
import { isInovanceProtocol, isModbusProtocol, isOmronProtocol, isSerialProtocol } from './types.ts'

export interface DeviceProfile {
  id: string
  label: string
  cpu?: CpuModel
  inovanceSeries?: InovanceSeries[]
  protocols: PlcProtocol[]
}

export interface DeviceGroup {
  id: string
  label: string
  description: string
  devices: DeviceProfile[]
}

export const deviceGroups: DeviceGroup[] = [
  {
    id: 'siemens', label: '西门子', description: 'Siemens SIMATIC',
    devices: [
      { id: 's7-1200', label: 'S7-1200', cpu: 'S1200', protocols: ['s7'] },
      { id: 's7-1500', label: 'S7-1500', cpu: 'S1500', protocols: ['s7'] },
      { id: 's7-300', label: 'S7-300', cpu: 'S300', protocols: ['s7'] },
      { id: 's7-400', label: 'S7-400', cpu: 'S400', protocols: ['s7'] },
      { id: 's7-200', label: 'S7-200', cpu: 'S200', protocols: ['s7'] },
      { id: 's7-200-smart', label: 'S7-200 SMART', cpu: 'S200Smart', protocols: ['s7'] },
    ],
  },
  {
    id: 'mitsubishi', label: '三菱', description: 'MELSEC · 按协议分类',
    devices: [
      { id: 'melsec-mc', label: 'MC', protocols: ['mc_binary', 'mc_ascii', 'mc_udp_binary', 'mc_udp_ascii'] },
      { id: 'melsec-a1e', label: 'A1E', protocols: ['a1e_binary', 'a1e_ascii'] },
      { id: 'melsec-mc-r', label: 'MC-R', protocols: ['mc_r_binary'] },
    ],
  },
  {
    id: 'omron', label: '欧姆龙', description: 'Omron · FINS · CS / CJ / CP',
    devices: [
      { id: 'omron-fins-tcp', label: 'FINS · TCP', protocols: ['omron_fins_tcp'] },
      { id: 'omron-fins-udp', label: 'FINS · UDP', protocols: ['omron_fins_udp'] },
    ],
  },
  {
    id: 'modbus', label: '通用 Modbus', description: '以太网 / 串口设备',
    devices: [
      { id: 'modbus-tcp', label: 'TCP', protocols: ['modbus_tcp'] },
      { id: 'modbus-udp', label: 'UDP', protocols: ['modbus_udp'] },
      { id: 'modbus-rtu', label: 'RTU · 串口', protocols: ['modbus_rtu'] },
      { id: 'modbus-ascii', label: 'ASCII · 串口', protocols: ['modbus_ascii'] },
    ],
  },
  {
    id: 'inovance', label: '汇川', description: 'Inovance · 内置 Modbus TCP',
    devices: [
      { id: 'inovance-am', label: 'AM / AC / AP', protocols: ['inovance_modbus_tcp'], inovanceSeries: ['AM', 'AC', 'AP'] },
      { id: 'inovance-evo', label: 'EVO', protocols: ['inovance_modbus_tcp'], inovanceSeries: ['EVO'] },
      { id: 'inovance-h3u', label: 'H3U', protocols: ['inovance_modbus_tcp'], inovanceSeries: ['H3U'] },
      { id: 'inovance-h5u', label: 'H5U', protocols: ['inovance_modbus_tcp'], inovanceSeries: ['H5U'] },
      { id: 'inovance-easy', label: 'Easy', protocols: ['inovance_modbus_tcp'], inovanceSeries: ['Easy'] },
    ],
  },
]

export function getDeviceProfile(id: string): DeviceProfile {
  const profile = deviceGroups.flatMap(group => group.devices).find(device => device.id === id)
  if (!profile) throw new Error(`未知设备分类：${id}`)
  return profile
}

export function getConnectedDevice(status: ConnectionStatus): DeviceProfile | undefined {
  if (!status.connected || !status.protocol) return undefined
  const protocol = status.protocol
  return deviceGroups.flatMap(group => group.devices).find(device =>
    device.protocols.includes(protocol) && (!device.cpu || device.cpu === status.cpu)
      && (!device.inovanceSeries || Boolean(status.inovanceSeries && device.inovanceSeries.includes(status.inovanceSeries))),
  )
}

export function filterDeviceGroups(query: string): DeviceGroup[] {
  const keyword = query.trim().toLocaleLowerCase().replace(/[\s-]/g, '')
  const matches = (value: string) => value.toLocaleLowerCase().replace(/[\s-]/g, '').includes(keyword)
  return deviceGroups.map(group => ({
    ...group,
    devices: matches(`${group.label} ${group.description}`)
      ? group.devices
      : group.devices.filter(device => matches(device.label)),
  })).filter(group => group.devices.length > 0)
}

export function createDeviceConnection(id: string): ConnectRequest {
  const profile = getDeviceProfile(id)
  const protocol = profile.protocols[0]
  return {
    protocol, host: '127.0.0.1', port: protocol === 's7' ? 102 : isModbusProtocol(protocol) || isInovanceProtocol(protocol) ? 502 : isOmronProtocol(protocol) ? 9600 : 6000,
    cpu: profile.cpu ?? 'S1200', rack: 0, slot: protocol === 's7' ? 1 : 0,
    connectTimeoutMs: 5000, receiveTimeoutMs: isSerialProtocol(protocol) ? 1000 : 5000,
    localTsap: '', remoteTsap: '',
    melsec: { networkNumber: 0, pcNumber: 255, ioNumber: 1023, stationNumber: 0, monitoringTimer: 16 },
    modbus: { unitId: 1, byteOrder: 'ABCD' },
    inovance: { series: profile.inovanceSeries?.[0] ?? 'AM', unitId: 1, byteOrder: 'CDAB' },
    omron: { sourceNode: 0, destinationNode: 0, sourceNetwork: 0, destinationNetwork: 0, sourceUnit: 0, destinationUnit: 0, gatewayCount: 2, byteOrder: 'CDAB' },
    serial: { path: 'COM1', baudRate: 9600, dataBits: protocol === 'modbus_ascii' ? 7 : 8, parity: 'even', stopBits: 1 },
  }
}
