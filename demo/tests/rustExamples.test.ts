import assert from 'node:assert/strict'
import test from 'node:test'
import { createDeviceConnection, deviceGroups } from '../src/deviceCatalog.ts'
import { generateRustExample, rustString } from '../src/rustExamples.ts'
import { addressExamples, addressNotes } from '../src/addressExamples.ts'

test('Rust examples preserve connection parameters for every protocol', () => {
  for (const device of deviceGroups.flatMap(group => group.devices)) {
    for (const protocol of device.protocols) {
      const config = { ...createDeviceConnection(device.id), protocol, host: '192.168.10.20' }
      const code = generateRustExample(config)
      assert.ok(code.source.includes('fn main() -> Result<(), Box<dyn std::error::Error>>'))
      assert.ok(code.operation.includes('plc.connect().to_result()?'))
      assert.ok(!code.source.includes('@tauri-apps') && !code.source.includes('undefined'))
      assert.ok(code.creation.includes(protocol === 'modbus_rtu' || protocol === 'modbus_ascii' ? 'serialport::new' : '192.168.10.20'))
    }
  }
})

test('Rust operation follows actual address, type, count and encoding', () => {
  const config = createDeviceConnection('s7-1500')
  config.localTsap = '0X0100'; config.remoteTsap = '0301'
  const numeric = generateRustExample(config, { kind: 'read', request: { address: 'DB20.8', dataType: 'i64', length: 4 } })
  assert.ok(numeric.creation.includes('S7Type::S1500') && numeric.creation.includes('with_tsap'))
  assert.ok(numeric.operation.includes('plc.read::<i64>("DB20.8", 4)'))
  assert.ok(generateRustExample(config, { kind: 'read', request: { address: 'DB1.0', dataType: 's7_string' } }).operation.includes('read_s7_strings("DB1.0")'))
  const text = generateRustExample(config, { kind: 'read', request: { address: 'DB1.0', dataType: 'u8', length: 4 }, text: { encoding: 'utf-16le', reverse: true } })
  assert.ok(text.operation.includes('pair.swap(0, 1)') && text.operation.includes('u16::from_le_bytes'))
  const write = generateRustExample(config, { kind: 'write', request: { address: 'DB1.0', dataType: 'u64', mode: 'single', values: ['18446744073709551615'], confirmed: true } })
  assert.ok(write.operation.includes('let confirmed = false;'))
  assert.ok(write.operation.includes('"18446744073709551615".parse::<u64>()?'))
})

test('generated Rust string literals cannot inject code or invalid control escapes', () => {
  assert.equal(rustString('A"\\\n\t\0'), '"A\\"\\\\\\u{a}\\u{9}\\u{0}"')
  assert.equal(rustString('\ud800'), '"\\u{fffd}"')
  assert.equal(rustString('PLC中文'), '"PLC中文"')
})

test('address reference distinguishes supported protocols from screenshot dialects', () => {
  const modbus = addressExamples('modbus_tcp')
  assert.ok(modbus.some(row => row.address === 'x=2;100' && row.note.includes('不是功能码')))
  assert.ok(modbus.some(row => row.address === 'IR100' && row.note.includes('只读')))
  assert.ok(addressNotes('modbus_tcp').includes('不支持 s='))
  assert.ok(!addressExamples('a1e_binary').some(row => row.address.includes('SB10')))
  assert.ok(addressExamples('mc_binary').some(row => row.address.includes('SB10')))
  assert.ok(addressExamples('mc_r_binary').some(row => row.address === 'RD100'))
  assert.ok(!addressExamples('mc_binary').some(row => row.address === 'RD100'))
})
