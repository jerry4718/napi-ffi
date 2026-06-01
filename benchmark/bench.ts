import { Bench } from 'tinybench'
import { createRequire } from 'node:module'
import path from 'node:path'

import ffi from '../index.js'
import { DataType, load as loadFfiRs, open as openFfiRs } from 'ffi-rs'

const require = createRequire(import.meta.url)
const { libraryPath } = require('../__test__/ffi-test-common.js')

const napiFfiLibrary = ffi.dlopen(libraryPath, {
  add_i32: { parameters: ['i32', 'i32'], result: 'i32' },
})
const napiFfiAddI32 = napiFfiLibrary.functions.add_i32

const ffiRsLibraryName = 'benchmark-fixture'
openFfiRs({
  library: ffiRsLibraryName,
  path: path.resolve(libraryPath),
})

function ffiRsAddI32(a: number, b: number) {
  return loadFfiRs({
    library: ffiRsLibraryName,
    funcName: 'add_i32',
    retType: DataType.I32,
    paramsType: [DataType.I32, DataType.I32],
    paramsValue: [a, b],
  })
}

function addI32InJs(a: number, b: number) {
  return a + b
}

const expected = 42
if (napiFfiAddI32(10, 32) !== expected) {
  throw new Error('napi-ffi benchmark setup failed')
}
if (ffiRsAddI32(10, 32) !== expected) {
  throw new Error('ffi-rs benchmark setup failed')
}
if (addI32InJs(10, 32) !== expected) {
  throw new Error('js benchmark setup failed')
}

for (let i = 0; i < 1000; i++) {
  napiFfiAddI32(10, 32)
  ffiRsAddI32(10, 32)
  addI32InJs(10, 32)
}

const bench = new Bench()

bench.add('napi-ffi add_i32', () => {
  napiFfiAddI32(10, 32)
})

bench.add('ffi-rs add_i32', () => {
  ffiRsAddI32(10, 32)
})

bench.add('JavaScript add_i32', () => {
  addI32InJs(10, 32)
})

await bench.run()

console.table(bench.table())

napiFfiLibrary.lib.close()
