import assert from 'node:assert/strict'
import test from 'node:test'
import { createDeviceConnection, deviceGroups, filterDeviceGroups, getConnectedDevice } from '../src/deviceCatalog.ts'
import { availableDataTypes, defaultAddress, isInovanceProtocol, isModbusProtocol, isReadOnlyAddress } from '../src/types.ts'
import type { InovanceSeries } from '../src/types.ts'
import { addressExamples, addressNotes } from '../src/addressExamples.ts'
import { batchBytes, batchRequest, batchWriteRequest } from '../src/batchBuffer.ts'
import { generateRustExample } from '../src/rustExamples.ts'
import { parseBatch } from '../src/workbench.ts'

const protocol = 'inovance_modbus_tcp'
const models: [InovanceSeries, string][] = [
  ['AM', 'inovance-am'], ['AC', 'inovance-am'], ['AP', 'inovance-am'], ['EVO', 'inovance-evo'],
  ['H3U', 'inovance-h3u'], ['H5U', 'inovance-h5u'], ['Easy', 'inovance-easy'],
]

test('Inovance groups shared AM/AC/AP navigation and restores all seven actual models', () => {
  const group = deviceGroups.find(group => group.id === 'inovance')!
  assert.equal(group.devices.length, 5)
  assert.equal(filterDeviceGroups('汇川')[0].devices.length, 5)
  assert.equal(filterDeviceGroups('AC')[0].devices[0].id, 'inovance-am')
  assert.equal(filterDeviceGroups('EVO')[0].devices[0].id, 'inovance-evo')
  for (const [series, id] of models) {
    assert.equal(getConnectedDevice({ connected: true, protocol, cpu: null, pduLength: null, inovanceSeries: series })?.id, id)
    assert.equal(getConnectedDevice({ connected: false, protocol, cpu: null, pduLength: null, inovanceSeries: series }), undefined)
  }
  assert.equal(getConnectedDevice({ connected: true, protocol, cpu: null, pduLength: null }), undefined)
})

test('Inovance defaults stay independent from generic Modbus and other series', () => {
  const first = createDeviceConnection('inovance-am'), second = createDeviceConnection('inovance-h3u')
  assert.deepEqual(first.inovance, { series: 'AM', unitId: 1, byteOrder: 'CDAB' })
  assert.equal(first.port, 502)
  assert.equal(second.inovance.series, 'H3U')
  first.inovance.unitId = 9; first.inovance.byteOrder = 'BADC'; first.inovance.series = 'AP'
  assert.equal(second.inovance.unitId, 1)
  assert.equal(second.inovance.byteOrder, 'CDAB')
  assert.equal(first.modbus.unitId, 1)
  assert.equal(createDeviceConnection('modbus-tcp').modbus.byteOrder, 'ABCD')
  assert.equal(isModbusProtocol(protocol), false)
  assert.equal(isInovanceProtocol(protocol), true)
})

test('Inovance chooses series-aware addresses and excludes only S7 string headers', () => {
  const types = availableDataTypes(protocol).map(type => type.value)
  for (const type of ['bool', 'u8', 'i8', 'u16', 'f64', 'raw_string']) assert.ok(types.includes(type as typeof types[number]))
  assert.ok(!types.includes('s7_string'))
  for (const [series] of models) {
    const iec = ['AM', 'AC', 'AP', 'EVO'].includes(series)
    assert.equal(defaultAddress(protocol, 'u16', series), iec ? 'MW100' : 'D100')
    assert.equal(defaultAddress(protocol, 'bool', series), iec ? 'MX100.0' : 'M100')
    const rows = addressExamples(protocol, series)
    assert.ok(rows.some(row => row.address.includes(iec ? 'MW100' : 'D100')))
    if (series === 'EVO') assert.ok(!rows.some(row => /^S[MD]/.test(row.address)))
    if (series === 'H3U') assert.ok(rows.find(row => row.address === 'C200')?.note.includes('32') || rows.find(row => row.address === 'C200')?.description.includes('32'))
    if (series === 'H5U' || series === 'Easy') assert.ok(rows.some(row => row.address === 'B0'))
    assert.ok(addressNotes(protocol, series).includes(series))
  }
})

test('Inovance batch bytes, coils and read-only inputs preserve request shapes', () => {
  assert.deepEqual(batchRequest(protocol, 's=2;MW100', 3), { address: 's=2;MW100', dataType: 'u8', length: 3 })
  for (const address of ['M100', 'X1.0', 'MX100.0', 'MW100.15', 's=2;%IX1.7']) {
    assert.equal(batchRequest(protocol, address, 3).dataType, 'bool')
  }
  assert.deepEqual([...batchBytes(batchRequest(protocol, 'MW100', 3), ['65', '66', '67'])], [65, 66, 67])
  const write = batchWriteRequest(protocol, batchRequest(protocol, 'MW100', 3), 3, '41 42 43')
  assert.deepEqual(write.values, ['65', '66', '67'])
  for (const address of ['X10', 's=2;X1.0', 'IX0.0', 'x=7;%I1']) {
    assert.equal(isReadOnlyAddress(protocol, address), true)
    assert.throws(() => batchWriteRequest(protocol, batchRequest(protocol, address, 1), 1, '01'), /只读/)
  }
  assert.equal(isReadOnlyAddress(protocol, 'MX1.0'), false)
  assert.equal(isReadOnlyAddress(protocol, 'MW100'), false)
  assert.deepEqual(parseBatch('MW100,u16,2\nMX100.0,bool,1', protocol), [
    { address: 'MW100', dataType: 'u16', length: 2 }, { address: 'MX100.0', dataType: 'bool', length: 1 },
  ])
})

test('Inovance Rust samples use the real constructor, chosen model and live operation', () => {
  for (const [series, id] of models) {
    const config = createDeviceConnection(id)
    config.host = '192.0.2.10'; config.port = 1502
    config.inovance = { series, unitId: 9, byteOrder: 'BADC' }
    const address = defaultAddress(protocol, 'u16', series)
    const sample = generateRustExample(config, { kind: 'read', request: { address, dataType: 'u64', length: 2 } })
    assert.ok(sample.creation.includes(`InovanceModbusTcp::new(address, 1502, InovanceType::${series}, timeout)`))
    assert.ok(sample.creation.includes('set_unit_id(9)'))
    assert.ok(sample.creation.includes('ByteOrder::BADC'))
    assert.ok(sample.operation.includes(`read::<u64>("${address}", 2)`))
    assert.ok(sample.source.includes('communication::inovace::'))
    assert.ok(!sample.source.includes('Melsec'))
    const write = generateRustExample(config, { kind: 'write', request: { address, dataType: 'u16', values: ['1', '2'], mode: 'array', confirmed: true } })
    assert.ok(write.operation.includes('let confirmed = false'))
    assert.ok(write.operation.includes('write_all::<u16>'))
  }
})
