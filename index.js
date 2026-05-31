'use strict'

const native = require('./native.js')
const {
  DynamicLibrary,
  dlclose,
  dlopen,
  dlsym,
  kSbInvokeSlow,
  kSbParams,
  kSbResult,
  kSbSharedBuffer,
} = require('./lib/internal/ffi-library')
const memory = require('./lib/internal/ffi-memory')

const suffix = native.suffix()
const types = Object.freeze({
  __proto__: null,
  VOID: 'void',
  POINTER: 'pointer',
  BUFFER: 'buffer',
  ARRAY_BUFFER: 'arraybuffer',
  FUNCTION: 'function',
  BOOL: 'bool',
  CHAR: 'char',
  STRING: 'string',
  FLOAT: 'float',
  DOUBLE: 'double',
  INT_8: 'int8',
  UINT_8: 'uint8',
  INT_16: 'int16',
  UINT_16: 'uint16',
  INT_32: 'int32',
  UINT_32: 'uint32',
  INT_64: 'int64',
  UINT_64: 'uint64',
  FLOAT_32: 'float32',
  FLOAT_64: 'float64',
})

module.exports = {
  DynamicLibrary,
  dlopen,
  dlclose,
  dlsym,
  ...memory,
  suffix,
  types,
}

Object.defineProperties(module.exports, {
  kSbInvokeSlow: { value: kSbInvokeSlow, enumerable: false, configurable: false, writable: false },
  kSbParams: { value: kSbParams, enumerable: false, configurable: false, writable: false },
  kSbResult: { value: kSbResult, enumerable: false, configurable: false, writable: false },
  kSbSharedBuffer: { value: kSbSharedBuffer, enumerable: false, configurable: false, writable: false },
})
