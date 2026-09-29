import assert from 'node:assert/strict'
import test from 'node:test'
import { createDeviceConnection, deviceGroups, filterDeviceGroups, getConnectedDevice, getDeviceProfile } from '../src/deviceCatalog.ts'
import { protocolLabels } from '../src/types.ts'

test('device navigation exposes all existing protocols without mixing brands', () => {
  const devices = deviceGroups.flatMap(group => group.devices)
  assert.equal(new Set(devices.map(device => device.id)).size, devices.length)
  assert.deepEqual([...new Set(devices.flatMap(device => device.protocols))].sort(), Object.keys(protocolLabels).sort())
  assert.ok(deviceGroups.every(group => group.devices.length > 0))
  assert.ok(devices.every(device => device.protocols.length > 0))
  assert.deepEqual(getDeviceProfile('melsec-mc').protocols, ['mc_binary', 'mc_ascii', 'mc_udp_binary', 'mc_udp_ascii'])
  assert.deepEqual(getDeviceProfile('melsec-a1e').protocols, ['a1e_binary', 'a1e_ascii'])
})

test('Siemens series resolve to the correct CPU and S7 connection', () => {
  const expected = { 's7-1200': 'S1200', 's7-1500': 'S1500', 's7-300': 'S300', 's7-400': 'S400', 's7-200': 'S200', 's7-200-smart': 'S200Smart' }
  for (const [id, cpu] of Object.entries(expected)) {
    const request = createDeviceConnection(id)
    assert.equal(request.cpu, cpu)
    assert.equal(request.protocol, 's7')
    assert.equal(request.port, 102)
  }
})

test('connection defaults match ethernet and serial categories', () => {
  for (const id of ['melsec-mc', 'melsec-a1e', 'melsec-mc-r']) assert.equal(createDeviceConnection(id).port, 6000)
  for (const id of ['modbus-tcp', 'modbus-udp']) assert.equal(createDeviceConnection(id).port, 502)
  assert.equal(createDeviceConnection('modbus-rtu').serial.dataBits, 8)
  assert.equal(createDeviceConnection('modbus-ascii').serial.dataBits, 7)
  assert.equal(createDeviceConnection('modbus-rtu').receiveTimeoutMs, 1000)
  assert.equal(createDeviceConnection('modbus-ascii').receiveTimeoutMs, 1000)
})

test('each device starts with independent editable parameters', () => {
  const first = createDeviceConnection('s7-1200')
  const second = createDeviceConnection('s7-1500')
  first.host = '192.168.1.10'
  first.serial.path = 'COM9'
  first.modbus.unitId = 42
  first.melsec.stationNumber = 8
  assert.equal(second.host, '127.0.0.1')
  assert.equal(second.serial.path, 'COM1')
  assert.equal(second.modbus.unitId, 1)
  assert.equal(second.melsec.stationNumber, 0)
})

test('navigation search matches brands, series and normalized labels', () => {
  assert.equal(filterDeviceGroups('西门子')[0].devices.length, 6)
  assert.equal(filterDeviceGroups(' Siemens ')[0].id, 'siemens')
  assert.deepEqual(filterDeviceGroups('1200')[0].devices.map(device => device.id), ['s7-1200'])
  assert.deepEqual(filterDeviceGroups('s7 1500')[0].devices.map(device => device.id), ['s7-1500'])
  assert.equal(filterDeviceGroups('三菱')[0].devices.length, 3)
  assert.equal(filterDeviceGroups('Modbus')[0].devices.length, 4)
  assert.equal(filterDeviceGroups('不存在的设备').length, 0)
  assert.deepEqual(filterDeviceGroups('  '), deviceGroups)
  assert.equal(deviceGroups[0].devices.length, 6)
})

test('unknown categories cannot silently select a different PLC', () => {
  assert.throws(() => getDeviceProfile('unknown'), /未知设备分类/)
  assert.throws(() => createDeviceConnection('unknown'), /未知设备分类/)
})

test('an existing desktop session selects its actual series and protocol category', () => {
  assert.equal(getConnectedDevice({ connected: true, protocol: 's7', cpu: 'S1500', pduLength: 480 })?.id, 's7-1500')
  assert.equal(getConnectedDevice({ connected: true, protocol: 'mc_udp_ascii', cpu: null, pduLength: null })?.id, 'melsec-mc')
  assert.equal(getConnectedDevice({ connected: true, protocol: 'modbus_rtu', cpu: null, pduLength: null })?.id, 'modbus-rtu')
  assert.equal(getConnectedDevice({ connected: false, protocol: 's7', cpu: 'S1500', pduLength: null }), undefined)
  assert.equal(getConnectedDevice({ connected: false, protocol: null, cpu: null, pduLength: null }), undefined)
})
