import assert from 'node:assert/strict'
import test from 'node:test'
import { batchBytes, batchRequest, batchWriteRequest, formatBatch } from '../src/batchBuffer.ts'

test('continuous requests use protocol-specific units and bounded lengths', () => {
  assert.deepEqual(batchRequest('s7', ' DB1.100 ', 10), { address: 'DB1.100', dataType: 'u8', length: 10 })
  assert.equal(batchRequest('mc_binary', 'D100', 10).dataType, 'u8')
  assert.equal(batchRequest('modbus_rtu', 'x=2;100', 10).dataType, 'u16')
  assert.equal(batchRequest('modbus_tcp', 'x=2;DI100', 10).dataType, 'bool')
  for (const length of [0, -1, 1.5, 1025, NaN]) assert.throws(() => batchRequest('s7', 'DB1.0', length))
  assert.throws(() => batchRequest('modbus_tcp', 'HR0', 513))
  assert.throws(() => batchRequest('s7', '', 1))
})

test('byte interpretation and word normalization preserve original data', () => {
  const request = batchRequest('modbus_tcp', 'HR0', 2)
  const bytes = batchBytes(request, ['16706', '17220'])
  assert.equal(formatBatch(bytes, 'hex', false, 2), '41 42\n43 44')
  assert.equal(formatBatch(bytes, 'ascii', false, 10), 'ABCD')
  assert.equal(formatBatch(bytes, 'ascii', true, 10), 'BADC')
  assert.equal(formatBatch(bytes, 'u16', false, 10), '16706 17220')
  assert.equal(formatBatch(bytes, 'u16', true, 10), '16961 17475')
  assert.deepEqual([...bytes], [65, 66, 67, 68])
  assert.throws(() => formatBatch(bytes.slice(0, 3), 'f32', false, 10))
  assert.throws(() => formatBatch(bytes, 'hex', false, 0))
})

test('write-back is bounded to the captured block and rejects read-only areas', () => {
  const request = batchRequest('modbus_tcp', 'x=2;HR100', 2)
  assert.deepEqual(batchWriteRequest('modbus_tcp', request, 4, '41 42 43 44'), { address: 'x=2;HR100', dataType: 'u16', mode: 'array', values: ['16706', '17220'], confirmed: true })
  assert.throws(() => batchWriteRequest('modbus_tcp', request, 4, '00 01'))
  assert.throws(() => batchWriteRequest('modbus_tcp', batchRequest('modbus_tcp', 'x=2;IR100', 2), 4, '00 01 00 02'))
  const coils = batchRequest('modbus_rtu', 'C0', 2)
  assert.deepEqual(batchWriteRequest('modbus_rtu', coils, 2, '00 01').values, ['false', 'true'])
  assert.throws(() => batchWriteRequest('modbus_rtu', coils, 2, '00 02'))
})
