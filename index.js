'use strict'

const native = require('./native.js')

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

function validatePointer(pointer, name = 'pointer') {
  if (typeof pointer !== 'bigint') {
    throw new TypeError(`The ${name} must be a bigint`)
  }
  if (pointer < 0n) {
    throw new TypeError(`The ${name} must be a non-negative bigint`)
  }
}

function validateLength(len) {
  if (typeof len !== 'number' || !Number.isInteger(len) || len < 0) {
    throw new TypeError('The length must be a non-negative integer')
  }
}

function exportBytes(source, pointer, len) {
  validatePointer(pointer, 'first argument')
  validateLength(len)
  if (pointer === 0n && len > 0) {
    throw new TypeError('Cannot write to a null pointer')
  }

  const target = native.toBuffer(pointer, len, false)
  if (source.length > target.length) {
    const err = new RangeError(`The value of "len" is out of range. It must be >= ${source.length}. Received ${len}`)
    err.code = 'ERR_OUT_OF_RANGE'
    throw err
  }
  source.copy(target, 0, 0, source.length)
}

function exportString(str, pointer, len, encoding = 'utf8') {
  if (typeof str !== 'string') {
    const err = new TypeError('The "string" argument must be of type string')
    err.code = 'ERR_INVALID_ARG_TYPE'
    throw err
  }
  if (typeof encoding !== 'string') {
    const err = new TypeError('The "encoding" argument must be of type string')
    err.code = 'ERR_INVALID_ARG_TYPE'
    throw err
  }

  const source = Buffer.from(str, encoding)
  const terminatorSize = /^(ucs2|ucs-2|utf16le|utf-16le)$/i.test(encoding) ? 2 : 1
  const requiredLength = source.length + terminatorSize
  if (len < requiredLength) {
    const err = new RangeError(`The value of "len" is out of range. It must be >= ${requiredLength}. Received ${len}`)
    err.code = 'ERR_OUT_OF_RANGE'
    throw err
  }

  const target = native.toBuffer(pointer, len, false)
  source.copy(target, 0, 0, source.length)
  target.fill(0, source.length, source.length + terminatorSize)
}

function exportBuffer(source, pointer, len) {
  if (!Buffer.isBuffer(source)) {
    const err = new TypeError('The "buffer" argument must be an instance of Buffer')
    err.code = 'ERR_INVALID_ARG_TYPE'
    throw err
  }
  exportBytes(source, pointer, len)
}

function exportArrayBuffer(source, pointer, len) {
  if (!(source instanceof ArrayBuffer)) {
    const err = new TypeError('The "arrayBuffer" argument must be an instance of ArrayBuffer')
    err.code = 'ERR_INVALID_ARG_TYPE'
    throw err
  }
  exportBytes(Buffer.from(source), pointer, len)
}

function exportArrayBufferView(source, pointer, len) {
  if (!ArrayBuffer.isView(source)) {
    const err = new TypeError('The "arrayBufferView" argument must be an instance of ArrayBufferView')
    err.code = 'ERR_INVALID_ARG_TYPE'
    throw err
  }
  exportBytes(Buffer.from(source.buffer, source.byteOffset, source.byteLength), pointer, len)
}

function normalizeSignature(definition = {}) {
  const result = definition.returns ?? definition.return ?? definition.result ?? 'void'
  const args = definition.parameters ?? definition.arguments ?? []
  return { result, arguments: args }
}

function callBySpec(lib, spec, args) {
  if (args.length !== spec.arguments.length) {
    throw new TypeError(`Invalid argument count: expected ${spec.arguments.length}, got ${args.length}`)
  }
  return lib._native.invoke(spec.key, spec.pointer, args)
}

function createWrappedFunction(lib, name, spec) {
  const wrapped = (...args) => callBySpec(lib, spec, args)
  Object.defineProperty(wrapped, 'name', { value: name, configurable: true })
  Object.defineProperty(wrapped, 'length', { value: spec.arguments.length, configurable: true })
  Object.defineProperty(wrapped, 'pointer', { value: spec.pointer, enumerable: true, configurable: true })
  Object.defineProperty(wrapped, '__ffiSpec', { value: spec, enumerable: false, configurable: true })
  return wrapped
}

class DynamicLibrary {
  constructor(path) {
    this._native = new native.DynamicLibrary(path)
  }

  get path() {
    return this._native.path
  }

  close() {
    return this._native.close()
  }

  getSymbol(symbol) {
    return this._native.getSymbol(symbol)
  }

  getFunction(symbol, definition = {}) {
    const signature = normalizeSignature(definition)
    const spec = this._native.getFunction(symbol, signature)
    return createWrappedFunction(this, symbol, spec)
  }

  getFunctions(definitions = {}) {
    const out = Object.create(null)
    for (const [name, definition] of Object.entries(definitions)) {
      out[name] = this.getFunction(name, definition)
    }
    return out
  }
}

function dlopen(path, definitions) {
  const lib = new DynamicLibrary(path)
  try {
    const functions = definitions === undefined ? Object.create(null) : lib.getFunctions(definitions)
    return {
      lib,
      functions,
      [Symbol.dispose]() {
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

module.exports = {
  DynamicLibrary,
  dlopen,
  dlclose,
  dlsym,
  exportArrayBuffer,
  exportArrayBufferView,
  exportBuffer,
  exportString,
  getFloat32: native.getFloat32,
  getFloat64: native.getFloat64,
  getInt16: native.getInt16,
  getInt32: native.getInt32,
  getInt64: native.getInt64,
  getInt8: native.getInt8,
  getRawPointer: native.getRawPointer,
  getUint16: native.getUint16,
  getUint32: native.getUint32,
  getUint64: native.getUint64,
  getUint8: native.getUint8,
  setFloat32: native.setFloat32,
  setFloat64: native.setFloat64,
  setInt16: native.setInt16,
  setInt32: native.setInt32,
  setInt64: native.setInt64,
  setInt8: native.setInt8,
  setUint16: native.setUint16,
  setUint32: native.setUint32,
  setUint64: native.setUint64,
  setUint8: native.setUint8,
  suffix,
  toArrayBuffer: native.toArrayBuffer,
  toBuffer: native.toBuffer,
  toString: native.toString,
  types,
}
