import assert from 'node:assert/strict'
import test from 'node:test'
import { createDeviceConnection, filterDeviceGroups, getConnectedDevice } from '../src/deviceCatalog.ts'
import { availableDataTypes, defaultAddress, isModbusProtocol, isOmronProtocol, isSerialProtocol } from '../src/types.ts'
import { addressExamples, addressNotes } from '../src/addressExamples.ts'
import { batchRequest, batchWriteRequest } from '../src/batchBuffer.ts'
import { generateRustExample } from '../src/rustExamples.ts'
import { parseBatch } from '../src/workbench.ts'

test('Omron.cs has independent TCP/UDP leaves with FINS defaults', () => {
  assert.deepEqual(filterDeviceGroups('欧姆龙')[0].devices.map(device => device.id), ['omron-fins-tcp', 'omron-fins-udp'])
  assert.equal(filterDeviceGroups('Omron.cs')[0].id, 'omron')
  assert.equal(filterDeviceGroups('FINS')[0].devices.length, 2)
  const tcp = createDeviceConnection('omron-fins-tcp'), udp = createDeviceConnection('omron-fins-udp')
  assert.equal(tcp.port, 9600)
  assert.equal(udp.port, 9600)
  assert.equal(tcp.omron.byteOrder, 'CDAB')
  assert.equal(tcp.omron.sourceNode, 0)
  assert.equal(tcp.omron.destinationNode, 0)
  assert.equal(tcp.omron.gatewayCount, 2)
  tcp.omron.sourceNode = 20
  assert.equal(udp.omron.sourceNode, 0)
  for (const request of [tcp, udp]) {
    assert.ok(isOmronProtocol(request.protocol))
    assert.ok(!isModbusProtocol(request.protocol) && !isSerialProtocol(request.protocol))
    assert.equal(getConnectedDevice({ connected: true, protocol: request.protocol, cpu: null, pduLength: null })?.id, request === tcp ? 'omron-fins-tcp' : 'omron-fins-udp')
  }
})

test('Omron.cs uses CIO bit addresses and enables bytes and raw text, not S7 headers', () => {
  for (const protocol of ['omron_fins_tcp', 'omron_fins_udp'] as const) {
    assert.equal(defaultAddress(protocol, 'bool'), 'CIO100.0')
    assert.equal(defaultAddress(protocol, 'u32'), 'D100')
    const types = availableDataTypes(protocol).map(type => type.value)
    for (const type of ['bool', 'u8', 'i8', 'u64', 'f64', 'raw_string']) assert.ok(types.includes(type as typeof types[number]))
    assert.ok(!types.includes('s7_string'))
    const rows = addressExamples(protocol)
    assert.ok(rows.some(row => row.address.includes('CIO100.15')))
    assert.ok(rows.some(row => row.address.includes('E10.100')))
    assert.ok(!rows.some(row => row.address === 'M100'))
    assert.ok(addressNotes(protocol).includes('CDAB'))
    assert.ok(addressNotes(protocol).includes('TIM/CNT'))
  }
})

test('Omron.cs batch bytes and point tables retain actual addresses and request shapes', () => {
  const request = batchRequest('omron_fins_tcp', 'E0.100', 3)
  assert.deepEqual(request, { address: 'E0.100', dataType: 'u8', length: 3 })
  const write = batchWriteRequest('omron_fins_tcp', request, 3, '41 42 43')
  assert.equal(write.address, 'E0.100')
  assert.equal(write.dataType, 'u8')
  assert.deepEqual(write.values, ['65', '66', '67'])
  assert.throws(() => batchWriteRequest('omron_fins_tcp', request, 3, '41 42'), /字节数/)
  const points = parseBatch('CIO100.15,bool,2\nD100,f32,1', 'omron_fins_udp')
  assert.equal(points[0].address, 'CIO100.15')
  assert.equal(points[0].dataType, 'bool')
})

test('Omron.cs Rust creation uses the real route and byte order APIs', () => {
  for (const id of ['omron-fins-tcp', 'omron-fins-udp']) {
    const config = createDeviceConnection(id)
    config.host = '192.0.2.10'
    config.port = 9601
    config.omron = { sourceNode: 20, destinationNode: 10, sourceNetwork: 1, destinationNetwork: 2, sourceUnit: 254, destinationUnit: 0, gatewayCount: 5, byteOrder: 'BADC' }
    const code = generateRustExample(config, { kind: 'read', request: { address: 'D120', dataType: 'f32', length: 2 } })
    assert.ok(code.creation.includes(id.endsWith('tcp') ? 'OmronFinsTcp::new' : 'OmronFinsUdp::new'))
    assert.ok(code.creation.includes('address, 9601, timeout'))
    assert.ok(code.creation.includes('source_node: 20, destination_node: 10'))
    assert.ok(code.creation.includes('source_network: 1, destination_network: 2'))
    assert.ok(code.creation.includes('source_unit: 254, destination_unit: 0'))
    assert.ok(code.creation.includes('gateway_count: 5'))
    assert.ok(code.creation.includes('ByteOrder::BADC'))
    assert.ok(!code.creation.includes('Melsec'))
    assert.ok(code.operation.includes('read::<f32>("D120", 2)'))
    const write = generateRustExample(config, { kind: 'write', request: { address: 'D200', dataType: 'u16', values: ['1', '2'], mode: 'array', confirmed: true } })
    assert.ok(write.operation.includes('let confirmed = false'))
    assert.ok(write.operation.includes('write_all::<u16>'))
  }
})
