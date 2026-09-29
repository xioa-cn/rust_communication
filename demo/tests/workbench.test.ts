import assert from 'node:assert/strict'
import test from 'node:test'
import { createSerialRunner, csvCell, decodeText, displayValue, encodeText, parseBatch, parseHex, parseValues, reversePairs, samplesToCsv } from '../src/workbench.ts'

test('array expressions expand without evaluating code', () => {
  assert.deepEqual(parseValues('[1, 2, 3]', 'u16'), ['1', '2', '3'])
  assert.deepEqual(parseValues('[3:1]', 'i16'), ['3', '2', '1'])
  assert.deepEqual(parseValues('[-2:1]', 'i16'), ['-2', '-1', '0', '1'])
  assert.deepEqual(parseValues('[7*3]', 'u16'), ['7', '7', '7'])
  assert.throws(() => parseValues('[1:100000000]', 'i32'), /1024/)
  assert.throws(() => parseValues('[1*0]', 'u16'))
  assert.throws(() => parseValues('alert(1)', 'u16'))
})

test('numeric validation preserves 64 bit precision and bounds', () => {
  assert.deepEqual(parseValues('18446744073709551615', 'u64'), ['18446744073709551615'])
  assert.deepEqual(parseValues('-9223372036854775808', 'i64'), ['-9223372036854775808'])
  for (const [value, type] of [['256', 'u8'], ['-1', 'u16'], ['128', 'i8'], ['18446744073709551616', 'u64'], ['NaN', 'f64'], ['1e40', 'f32'], ['0xff', 'f64']] as const) assert.throws(() => parseValues(value, type))
  assert.deepEqual(parseValues('1.2e3, -.5', 'f64'), ['1.2e3', '-.5'])
  assert.deepEqual(parseValues('True,false,1,0', 'bool'), ['true', 'false', 'true', 'false'])
  assert.throws(() => parseValues('2', 'bool'))
})

test('strings are never interpreted as array expressions', () => {
  assert.deepEqual(parseValues(' [1:100] ', 'raw_string'), [' [1:100] '])
  assert.deepEqual(parseValues('', 's7_string'), [''])
})

test('display conversion preserves large integers and non-numeric text', () => {
  assert.equal(displayValue('18446744073709551615', 'hex'), '0xFFFFFFFFFFFFFFFF')
  assert.equal(displayValue('-10', 'bin'), '-0b1010')
  assert.equal(displayValue('1.5', 'hex'), '1.5')
})

test('HEX parsing checks malformed and oversized buffers', () => {
  assert.deepEqual([...parseHex('0x01, 0x02:0A FF')], [1, 2, 10, 255])
  for (const text of ['', 'A', 'GG', 'AA'.repeat(1025)]) assert.throws(() => parseHex(text))
})

test('text encoders handle Unicode and reject invalid byte sequences', () => {
  for (const encoding of ['utf-8', 'utf-16le', 'utf-16be'] as const) assert.equal(decodeText(encodeText('PLC 中文 🚀', encoding), encoding), 'PLC 中文 🚀')
  assert.deepEqual([...encodeText('ABC', 'ascii')], [65, 66, 67])
  assert.throws(() => encodeText('中文', 'ascii'))
  assert.throws(() => decodeText(Uint8Array.from([255]), 'ascii'))
  assert.throws(() => decodeText(Uint8Array.from([65]), 'utf-16le'))
  const original = Uint8Array.from([1, 2, 3])
  assert.deepEqual([...reversePairs(original)], [2, 1, 3])
  assert.deepEqual([...original], [1, 2, 3])
})

test('batch parsing validates supported types, lengths and station prefixes', () => {
  assert.deepEqual(parseBatch('DB1.0,u16,10\nDB2.0,s7_string,1', 's7'), [{ address: 'DB1.0', dataType: 'u16', length: 10 }, { address: 'DB2.0', dataType: 's7_string' }])
  assert.equal(parseBatch('x=2;HR0,u16,2', 'modbus_tcp')[0].address, 'x=2;HR0')
  for (const text of ['', 'HR0,u8,1', 'HR0,u16,0', 'HR0,u16,1.5', 'HR0,u16,1,extra']) assert.throws(() => parseBatch(text, 'modbus_tcp'))
})

test('CSV escapes content and neutralizes spreadsheet formulas', () => {
  assert.equal(csvCell('a,"b"'), '"a,""b"""')
  assert.equal(csvCell('=SUM(A1)'), '"\'=SUM(A1)"')
  assert.equal(csvCell('  @danger'), '"\'  @danger"')
  assert.ok(samplesToCsv([{ id: 1, time: '2026-09-29T00:00:00Z', protocol: 's7', device: 'S1500', endpoint: '127.0.0.1:102', address: 'DB1.0', dataType: 'u64', values: ['18446744073709551615'], elapsedMs: 3 }]).includes('18446744073709551615'))
})

test('serial runner never overlaps requests and observes bounded count', async () => {
  const states: boolean[] = [], indices: number[] = []
  const runner = createSerialRunner(value => states.push(value))
  let active = 0
  await runner.start(async index => {
    active++
    assert.equal(active, 1)
    indices.push(index)
    await new Promise(resolve => setTimeout(resolve, 1))
    active--
  }, 0, 3)
  assert.deepEqual(indices, [0, 1, 2])
  assert.deepEqual(states, [true, false])
})

test('stop waits for the in-flight request and never sends the next one', async () => {
  const states: boolean[] = [], runner = createSerialRunner(value => states.push(value))
  let release: () => void = () => {}, calls = 0
  const pending = runner.start(async () => { calls++; await new Promise<void>(resolve => { release = resolve }) }, 0, 10)
  runner.stop()
  assert.equal(runner.running, true)
  assert.equal(runner.stopping, true)
  release()
  await pending
  assert.equal(calls, 1)
  assert.deepEqual(states, [true, false])
})

test('failed jobs stop the runner and permit a later explicit restart', async () => {
  const runner = createSerialRunner(() => {})
  let calls = 0
  await runner.start(async () => { calls++; return false }, 0, 10)
  assert.equal(calls, 1)
  await assert.rejects(runner.start(async () => { throw new Error('failed') }, 0), /failed/)
  assert.equal(runner.running, false)
  await runner.start(async () => { calls++ }, 0, 1)
  assert.equal(calls, 2)
})
