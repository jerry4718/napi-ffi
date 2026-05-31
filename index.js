'use strict'

const native = require('./native.js')

const suffix = native.suffix()
const kOwningLibrary = Symbol('ffi.owningLibrary')

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

function invalidArgValue(message) {
  const err = new TypeError(message)
  err.code = 'ERR_INVALID_ARG_VALUE'
  return err
}

function invalidArgType(message) {
  const err = new TypeError(message)
  err.code = 'ERR_INVALID_ARG_TYPE'
  return err
}

function validatePointer(pointer, name = 'pointer') {
  if (typeof pointer !== 'bigint') {
    throw new TypeError(`The ${name} must be a bigint`)
  }
  if (pointer < 0n) {
    throw new TypeError(`The ${name} must be a non-negative bigint`)
  }
}

function validateLength(len) {
  if (typeof len !== 'number') {
    throw new TypeError('The length must be a number')
  }
  if (!Number.isInteger(len) || len < 0) {
    throw new TypeError('The length must be a non-negative integer')
  }
}

function normalizeI64Value(value, label) {
  if (typeof value === 'bigint') return value
  if (typeof value === 'number' && Number.isSafeInteger(value)) return BigInt(value)
  throw new TypeError(`Value must be ${label}`)
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
    throw invalidArgType('The "string" argument must be of type string')
  }
  if (typeof encoding !== 'string') {
    throw invalidArgType('The "encoding" argument must be of type string')
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
    throw invalidArgType('The "buffer" argument must be an instance of Buffer')
  }
  exportBytes(source, pointer, len)
}

function exportArrayBuffer(source, pointer, len) {
  if (!(source instanceof ArrayBuffer)) {
    throw invalidArgType('The "arrayBuffer" argument must be an instance of ArrayBuffer')
  }
  exportBytes(Buffer.from(source), pointer, len)
}

function exportArrayBufferView(source, pointer, len) {
  if (!ArrayBuffer.isView(source)) {
    throw invalidArgType('The "arrayBufferView" argument must be an instance of ArrayBufferView')
  }
  exportBytes(Buffer.from(source.buffer, source.byteOffset, source.byteLength), pointer, len)
}

function toBuffer(pointer, len, copy) {
  validatePointer(pointer, 'first argument')
  validateLength(len)
  return native.toBuffer(pointer, len, copy)
}

function toArrayBuffer(pointer, len, copy) {
  validatePointer(pointer, 'first argument')
  validateLength(len)
  return native.toArrayBuffer(pointer, len, copy)
}

function getRawPointer(value) {
  if (!Buffer.isBuffer(value) && !(value instanceof ArrayBuffer) && !ArrayBuffer.isView(value)) {
    throw invalidArgType('The first argument must be a Buffer, ArrayBuffer, or TypedArray')
  }
  return native.getRawPointer(value)
}

function wrapMemorySetter(setter, kind) {
  return (pointer, offset, value) => {
    if (kind === 'int64') return setter(pointer, offset, normalizeI64Value(value, 'an int64'))
    if (kind === 'uint64') return setter(pointer, offset, normalizeI64Value(value, 'a uint64'))
    return setter(pointer, offset, value)
  }
}

function normalizeSignature(definition = {}) {
  const result = definition.returns ?? definition.return ?? definition.result ?? 'void'
  const args = definition.parameters ?? definition.arguments ?? []
  return { result, arguments: args }
}

function signatureKey(signature) {
  return JSON.stringify([signature.result, signature.arguments])
}

function callBySpec(lib, spec, args) {
  if (args.length !== spec.arguments.length) {
    throw invalidArgValue(`Invalid argument count: expected ${spec.arguments.length}, got ${args.length}`)
  }
  return lib._native.invoke(spec.key, spec.pointer, args)
}

function createWrappedFunction(lib, name, spec) {
  const wrapped = (...args) => callBySpec(lib, spec, args)
  Object.defineProperty(wrapped, 'name', { value: name, configurable: true })
  Object.defineProperty(wrapped, 'length', { value: spec.arguments.length, configurable: true })
  Object.defineProperty(wrapped, 'pointer', { value: spec.pointer, enumerable: true, configurable: true })
  Object.defineProperty(wrapped, '__ffiSpec', { value: spec, enumerable: false, configurable: true })
  Object.defineProperty(wrapped, kOwningLibrary, { value: lib, enumerable: false, configurable: false })
  return wrapped
}

class DynamicLibrary {
  constructor(path) {
    this._native = new native.DynamicLibrary(path)
    this._functionCache = new Map()
    this._symbolCache = new Map()
    this.functions = Object.create(null)
    this.symbols = Object.create(null)
  }

  get path() {
    return this._native.path
  }

  close() {
    return this._native.close()
  }

  [Symbol.dispose]() {
    this.close()
  }

  getSymbol(symbol) {
    if (this._symbolCache.has(symbol)) return this._symbolCache.get(symbol)
    const pointer = this._native.getSymbol(symbol)
    this._symbolCache.set(symbol, pointer)
    this.symbols[symbol] = pointer
    return pointer
  }

  getSymbols() {
    return this.symbols
  }

  getFunction(symbol, definition = {}) {
    const signature = normalizeSignature(definition)
    const key = `${symbol}:${signatureKey(signature)}`
    const cached = this._functionCache.get(symbol)
    if (cached && cached.signature !== signatureKey(signature)) {
      throw new Error(`Function '${symbol}' was already requested with a different signature`)
    }
    if (cached) return cached.wrapper
    const spec = this._native.getFunction(symbol, signature)
    const wrapper = createWrappedFunction(this, symbol, spec)
    this._functionCache.set(symbol, { signature: signatureKey(signature), wrapper })
    this.functions[symbol] = wrapper
    this.symbols[symbol] = spec.pointer
    this._symbolCache.set(symbol, spec.pointer)
    return wrapper
  }

  getFunctions(definitions) {
    if (definitions === undefined) {
      return Object.assign(Object.create(null), this.functions)
    }
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
  getRawPointer,
  getUint16: native.getUint16,
  getUint32: native.getUint32,
  getUint64: native.getUint64,
  getUint8: native.getUint8,
  setFloat32: native.setFloat32,
  setFloat64: native.setFloat64,
  setInt16: native.setInt16,
  setInt32: native.setInt32,
  setInt64: wrapMemorySetter(native.setInt64, 'int64'),
  setInt8: native.setInt8,
  setUint16: native.setUint16,
  setUint32: native.setUint32,
  setUint64: wrapMemorySetter(native.setUint64, 'uint64'),
  setUint8: native.setUint8,
  suffix,
  toArrayBuffer,
  toBuffer,
  toString: native.toString,
  types,
}
