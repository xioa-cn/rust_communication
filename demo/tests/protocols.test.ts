import assert from 'node:assert/strict'
import test from 'node:test'
import { availableDataTypes, defaultAddress, isModbusProtocol, isSerialProtocol, isReadOnlyModbusAddress, protocolLabels } from '../src/types.ts'
import type { PlcProtocol } from '../src/types.ts'

const modbusProtocols: PlcProtocol[] = ['modbus_tcp', 'modbus_udp', 'modbus_rtu', 'modbus_ascii']

test('four Modbus modes have distinct protocol labels', () => {
  for (const protocol of modbusProtocols) {
    assert.equal(isModbusProtocol(protocol), true)
    assert.ok(protocolLabels[protocol].includes('Modbus'))
  }
  assert.equal(new Set(modbusProtocols.map(protocol => protocolLabels[protocol])).size, 4)
})

test('only RTU and ASCII select serial connection fields', () => {
  assert.equal(isSerialProtocol('modbus_rtu'), true)
  assert.equal(isSerialProtocol('modbus_ascii'), true)
  assert.equal(isSerialProtocol('modbus_tcp'), false)
  assert.equal(isSerialProtocol('modbus_udp'), false)
  assert.equal(isSerialProtocol('s7'), false)
  assert.equal(isModbusProtocol('mc_udp_ascii'), false)
})

test('Modbus offers numeric values and raw UTF-8 strings', () => {
  for (const protocol of modbusProtocols) {
    assert.deepEqual(availableDataTypes(protocol).map(option => option.value), ['bool', 'u16', 'i16', 'u32', 'i32', 'u64', 'i64', 'f32', 'f64', 'raw_string'])
  }
})

test('S7 keeps byte and string data types', () => {
  const types = availableDataTypes('s7').map(option => option.value)
  for (const type of ['u8', 'i8', 'raw_string', 's7_string']) assert.ok(types.includes(type as typeof types[number]))
})

test('MELSEC keeps raw strings but excludes S7 length headers', () => {
  const protocols: PlcProtocol[] = ['mc_binary', 'mc_ascii', 'mc_udp_binary', 'mc_udp_ascii', 'a1e_binary', 'a1e_ascii', 'mc_r_binary']
  for (const protocol of protocols) {
    const types = availableDataTypes(protocol).map(option => option.value)
    assert.ok(types.includes('raw_string'))
    assert.ok(types.includes('u8'))
    assert.equal(types.includes('s7_string'), false)
  }
})

test('Modbus default addresses are zero-based and type-aware', () => {
  for (const protocol of modbusProtocols) {
    assert.equal(defaultAddress(protocol, 'bool'), 'C0')
    assert.equal(defaultAddress(protocol, 'u16'), 'HR0')
    assert.equal(defaultAddress(protocol, 'f64'), 'HR0')
  }
})

test('legacy protocols retain their own address formats', () => {
  assert.equal(defaultAddress('s7', 'u16'), 'DB1.0')
  assert.equal(defaultAddress('s7', 'bool'), 'DB1.0.0')
  assert.equal(defaultAddress('mc_binary', 'u32'), 'D100')
  assert.equal(defaultAddress('mc_binary', 'bool'), 'M100')
})

test('station-prefixed input areas remain read-only', () => {
  for (const address of ['IR100', 'DI0', 'x=2;IR100', 'x=2;DI0', ' X = 2 ; ir100 ']) assert.equal(isReadOnlyModbusAddress(address), true)
  for (const address of ['HR100', 'C0', 'x=2;100', 'x=2;HR100', 'x=2;C0']) assert.equal(isReadOnlyModbusAddress(address), false)
})
