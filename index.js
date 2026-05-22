'use strict'

const native = require('./native.js')

const {
  DynamicLibrary,
  exportArrayBuffer,
  exportArrayBufferView,
  exportBuffer,
  exportString,
  getFloat32,
  getFloat64,
  getInt16,
  getInt32,
  getInt64,
  getInt8,
  getRawPointer,
  getUint16,
  getUint32,
  getUint64,
  getUint8,
  setFloat32,
  setFloat64,
  setInt16,
  setInt32,
  setInt64,
  setInt8,
  setUint16,
  setUint32,
  setUint64,
  setUint8,
  toArrayBuffer,
  toBuffer,
  toString: nativeToString,
} = native

const symbolDispose = Symbol.dispose || Symbol.for('Symbol.dispose')
const suffix = native.getSuffix()

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

DynamicLibrary.prototype[symbolDispose] = function() {
  this.close()
}

function dlopen(path, definitions) {
  const lib = new DynamicLibrary(path)
  try {
    const functions = definitions === undefined ? Object.freeze({ __proto__: null }) : lib.getFunctions(definitions)
    return {
      lib,
      functions,
      [symbolDispose]() {
        lib.close()
      },
    }
  } catch (error) {
    lib.close()
    throw error
  }
}

function dlclose(handle) {
  handle.close()
}

function dlsym(handle, symbol) {
  return handle.getSymbol(symbol)
}

function ffiToString(ptr) {
  return ptr === 0n ? null : nativeToString(ptr)
}

module.exports = {
  DynamicLibrary,
  dlopen,
  dlclose,
  dlsym,
  exportArrayBuffer,
  exportArrayBufferView,
  exportString,
  exportBuffer,
  getInt8,
  getUint8,
  getInt16,
  getUint16,
  getInt32,
  getUint32,
  getInt64,
  getUint64,
  getFloat32,
  getFloat64,
  getRawPointer,
  setInt8,
  setUint8,
  setInt16,
  setUint16,
  setInt32,
  setUint32,
  setInt64,
  setUint64,
  setFloat32,
  setFloat64,
  suffix,
  toString: ffiToString,
  toArrayBuffer,
  toBuffer,
  types,
}
