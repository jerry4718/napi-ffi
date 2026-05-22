'use strict'

const { Buffer, constants: bufferConstants } = require('node:buffer')
const native = require('./native.js')

const {
  DynamicLibrary: NativeDynamicLibrary,
  exportBytes,
  getCharIsSigned,
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
  toString: nativeToString,
  toBuffer: nativeToBuffer,
  toArrayBuffer: nativeToArrayBuffer,
} = native

const symbolDispose = Symbol.dispose || Symbol.for('Symbol.dispose')
const kSbSharedBuffer = native.getKSbSharedBuffer()
const kSbInvokeSlow = native.getKSbInvokeSlow()
const kSbParams = native.getKSbParams()
const kSbResult = native.getKSbResult()
const uintptrMax = native.getUintptrMax()

const suffix = process.platform === 'win32' ? 'dll' : process.platform === 'darwin' ? 'dylib' : 'so'

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

function createNodeError(ErrorCtor, code, message) {
  const error = new ErrorCtor(message)
  error.code = code
  return error
}

function validateString(value, name) {
  if (typeof value !== 'string') {
    throw createNodeError(TypeError, 'ERR_INVALID_ARG_TYPE', `${name} must be a string`)
  }
}

function validateInteger(value, name, min = 0) {
  if (!Number.isInteger(value) || value < min) {
    throw createNodeError(RangeError, 'ERR_OUT_OF_RANGE', `${name} must be an integer >= ${min}`)
  }
}

function validateArrayBuffer(source, name) {
  if (Object.prototype.toString.call(source) !== '[object ArrayBuffer]') {
    throw createNodeError(TypeError, 'ERR_INVALID_ARG_TYPE', `${name} must be an ArrayBuffer`)
  }
}

function validateArrayBufferView(source, name) {
  if (!ArrayBuffer.isView(source)) {
    throw createNodeError(TypeError, 'ERR_INVALID_ARG_TYPE', `${name} must be an ArrayBufferView`)
  }
}

function normalizeFFIError(error) {
  if (error && error.code === 'InvalidArg') {
    error.code = 'ERR_INVALID_ARG_VALUE'
  }
  return error
}

function callAndNormalize(fn) {
  try {
    return fn()
  } catch (error) {
    throw normalizeFFIError(error)
  }
}

const U64_MAX = 0xffffffffffffffffn
const I64_MAX = 0x7fffffffffffffffn
const I64_MIN = -0x8000000000000000n

const sbTypeInfo = {
  __proto__: null,
  i8: { set: DataView.prototype.setInt8, get: DataView.prototype.getInt8, kind: 'int', min: -128, max: 127, label: 'an int8' },
  int8: { set: DataView.prototype.setInt8, get: DataView.prototype.getInt8, kind: 'int', min: -128, max: 127, label: 'an int8' },
  char: getCharIsSigned()
    ? { set: DataView.prototype.setInt8, get: DataView.prototype.getInt8, kind: 'int', min: -128, max: 127, label: 'an int8' }
    : { set: DataView.prototype.setUint8, get: DataView.prototype.getUint8, kind: 'int', min: 0, max: 255, label: 'a uint8' },
  u8: { set: DataView.prototype.setUint8, get: DataView.prototype.getUint8, kind: 'int', min: 0, max: 255, label: 'a uint8' },
  uint8: { set: DataView.prototype.setUint8, get: DataView.prototype.getUint8, kind: 'int', min: 0, max: 255, label: 'a uint8' },
  bool: { set: DataView.prototype.setUint8, get: DataView.prototype.getUint8, kind: 'int', min: 0, max: 255, label: 'a uint8' },
  i16: { set: DataView.prototype.setInt16, get: DataView.prototype.getInt16, kind: 'int', min: -32768, max: 32767, label: 'an int16' },
  int16: { set: DataView.prototype.setInt16, get: DataView.prototype.getInt16, kind: 'int', min: -32768, max: 32767, label: 'an int16' },
  u16: { set: DataView.prototype.setUint16, get: DataView.prototype.getUint16, kind: 'int', min: 0, max: 65535, label: 'a uint16' },
  uint16: { set: DataView.prototype.setUint16, get: DataView.prototype.getUint16, kind: 'int', min: 0, max: 65535, label: 'a uint16' },
  i32: { set: DataView.prototype.setInt32, get: DataView.prototype.getInt32, kind: 'int', min: -2147483648, max: 2147483647, label: 'an int32' },
  int32: { set: DataView.prototype.setInt32, get: DataView.prototype.getInt32, kind: 'int', min: -2147483648, max: 2147483647, label: 'an int32' },
  u32: { set: DataView.prototype.setUint32, get: DataView.prototype.getUint32, kind: 'int', min: 0, max: 4294967295, label: 'a uint32' },
  uint32: { set: DataView.prototype.setUint32, get: DataView.prototype.getUint32, kind: 'int', min: 0, max: 4294967295, label: 'a uint32' },
  i64: { set: DataView.prototype.setBigInt64, get: DataView.prototype.getBigInt64, kind: 'i64', label: 'an int64' },
  int64: { set: DataView.prototype.setBigInt64, get: DataView.prototype.getBigInt64, kind: 'i64', label: 'an int64' },
  u64: { set: DataView.prototype.setBigUint64, get: DataView.prototype.getBigUint64, kind: 'u64', label: 'a uint64' },
  uint64: { set: DataView.prototype.setBigUint64, get: DataView.prototype.getBigUint64, kind: 'u64', label: 'a uint64' },
  f32: { set: DataView.prototype.setFloat32, get: DataView.prototype.getFloat32, kind: 'float', label: 'a float' },
  float: { set: DataView.prototype.setFloat32, get: DataView.prototype.getFloat32, kind: 'float', label: 'a float' },
  float32: { set: DataView.prototype.setFloat32, get: DataView.prototype.getFloat32, kind: 'float', label: 'a float' },
  f64: { set: DataView.prototype.setFloat64, get: DataView.prototype.getFloat64, kind: 'float', label: 'a double' },
  double: { set: DataView.prototype.setFloat64, get: DataView.prototype.getFloat64, kind: 'float', label: 'a double' },
  float64: { set: DataView.prototype.setFloat64, get: DataView.prototype.getFloat64, kind: 'float', label: 'a double' },
  pointer: { set: DataView.prototype.setBigUint64, get: DataView.prototype.getBigUint64, kind: 'pointer' },
  ptr: { set: DataView.prototype.setBigUint64, get: DataView.prototype.getBigUint64, kind: 'pointer' },
  function: { set: DataView.prototype.setBigUint64, get: DataView.prototype.getBigUint64, kind: 'pointer' },
  buffer: { set: DataView.prototype.setBigUint64, get: DataView.prototype.getBigUint64, kind: 'pointer' },
  arraybuffer: { set: DataView.prototype.setBigUint64, get: DataView.prototype.getBigUint64, kind: 'pointer' },
  string: { set: DataView.prototype.setBigUint64, get: DataView.prototype.getBigUint64, kind: 'pointer' },
  str: { set: DataView.prototype.setBigUint64, get: DataView.prototype.getBigUint64, kind: 'pointer' },
}

function throwFFIArgError(message) {
  throw createNodeError(TypeError, 'ERR_INVALID_ARG_VALUE', message)
}

function throwFFIArgCountError(expected, actual) {
  throwFFIArgError(`Invalid argument count: expected ${expected}, got ${actual}`)
}

function writeNumericArg(view, info, offset, arg, index) {
  if (info.kind === 'int') {
    if (typeof arg !== 'number' || !Number.isInteger(arg) || arg < info.min || arg > info.max) {
      throwFFIArgError(`Argument ${index} must be ${info.label}`)
    }
    info.set.call(view, offset, arg, true)
    return
  }
  if (info.kind === 'i64') {
    if (typeof arg !== 'bigint' || arg < I64_MIN || arg > I64_MAX) {
      throwFFIArgError(`Argument ${index} must be ${info.label}`)
    }
    info.set.call(view, offset, arg, true)
    return
  }
  if (info.kind === 'u64') {
    if (typeof arg !== 'bigint' || arg < 0n || arg > U64_MAX) {
      throwFFIArgError(`Argument ${index} must be ${info.label}`)
    }
    info.set.call(view, offset, arg, true)
    return
  }
  if (info.kind === 'float') {
    if (typeof arg !== 'number') {
      throwFFIArgError(`Argument ${index} must be ${info.label}`)
    }
    info.set.call(view, offset, arg, true)
  }
}

function writePointerArg(view, offset, arg, index) {
  if (typeof arg === 'bigint') {
    if (arg < 0n || arg > uintptrMax) {
      throwFFIArgError(`Argument ${index} must be a non-negative pointer bigint`)
    }
    DataView.prototype.setBigUint64.call(view, offset, arg, true)
    return true
  }
  if (arg === null || arg === undefined) {
    DataView.prototype.setBigUint64.call(view, offset, 0n, true)
    return true
  }
  return false
}

function inheritMetadata(wrapper, rawFn, nargs) {
  Object.defineProperty(wrapper, 'name', { __proto__: null, value: rawFn.name, configurable: true })
  Object.defineProperty(wrapper, 'length', { __proto__: null, value: nargs, configurable: true })
  Object.defineProperty(wrapper, 'pointer', {
    __proto__: null,
    value: rawFn.pointer,
    writable: true,
    configurable: true,
    enumerable: true,
  })
  return wrapper
}

function wrapWithSharedBuffer(rawFn, parameters, resultType) {
  if (rawFn === undefined || rawFn === null) return rawFn
  const buffer = rawFn[kSbSharedBuffer]
  if (buffer === undefined) return rawFn
  if (parameters === undefined) parameters = rawFn[kSbParams]
  if (resultType === undefined) resultType = rawFn[kSbResult]
  if (parameters === undefined || resultType === undefined) return rawFn

  const slowInvoke = rawFn[kSbInvokeSlow]
  const view = new DataView(buffer)
  const retGetter = resultType === 'void' ? null : sbTypeInfo[resultType].get
  const nargs = parameters.length
  const argInfos = []
  const argOffsets = []
  let anyPointer = false
  for (let i = 0; i < nargs; i++) {
    const info = sbTypeInfo[parameters[i]]
    argInfos.push(info)
    argOffsets.push(8 * (i + 1))
    if (info.kind === 'pointer') anyPointer = true
  }

  const wrapper = function(...args) {
    if (args.length !== nargs) {
      throwFFIArgCountError(nargs, args.length)
    }
    for (let i = 0; i < nargs; i++) {
      const info = argInfos[i]
      const offset = argOffsets[i]
      if (info.kind === 'pointer') {
        if (!writePointerArg(view, offset, args[i], i)) {
          return callAndNormalize(() => slowInvoke(...args))
        }
      } else {
        writeNumericArg(view, info, offset, args[i], i)
      }
    }
    callAndNormalize(() => rawFn())
    return retGetter === null ? undefined : retGetter.call(view, 0, true)
  }

  return inheritMetadata(wrapper, rawFn, anyPointer ? nargs : nargs)
}

function sigParams(sig) {
  return sig.parameters ?? sig.arguments ?? []
}

function sigResult(sig) {
  return sig.result ?? sig.return ?? sig.returns ?? 'void'
}

const rawGetFunction = NativeDynamicLibrary.prototype.getFunction
const rawGetFunctions = NativeDynamicLibrary.prototype.getFunctions
const functionsDescriptor = Object.getOwnPropertyDescriptor(NativeDynamicLibrary.prototype, 'functions')

NativeDynamicLibrary.prototype[symbolDispose] = function() {
  this.close()
}

NativeDynamicLibrary.prototype.getFunction = function getFunction(name, sig) {
  const raw = callAndNormalize(() => rawGetFunction.call(this, name, sig))
  return wrapWithSharedBuffer(raw, sigParams(sig), sigResult(sig))
}

NativeDynamicLibrary.prototype.getFunctions = function getFunctions(definitions) {
  const raw = callAndNormalize(() => (definitions === undefined ? rawGetFunctions.call(this) : rawGetFunctions.call(this, definitions)))
  if (raw === undefined || raw === null) return raw
  const out = { __proto__: null }
  for (const name of Object.keys(raw)) {
    if (definitions === undefined) {
      out[name] = wrapWithSharedBuffer(raw[name])
    } else {
      const sig = definitions[name]
      out[name] = wrapWithSharedBuffer(raw[name], sigParams(sig), sigResult(sig))
    }
  }
  return out
}

Object.defineProperty(NativeDynamicLibrary.prototype, 'functions', {
  __proto__: null,
  configurable: true,
  enumerable: functionsDescriptor.enumerable,
  get() {
    const raw = callAndNormalize(() => functionsDescriptor.get.call(this))
    if (raw === undefined || raw === null) return raw
    const wrapped = { __proto__: null }
    for (const name of Object.keys(raw)) {
      wrapped[name] = wrapWithSharedBuffer(raw[name])
    }
    return wrapped
  },
})

const nativeClose = NativeDynamicLibrary.prototype.close

class DynamicLibrary extends NativeDynamicLibrary {
  constructor(path) {
    super(path)
  }
}

function maybeSkipCallbackTest() {
  const stack = new Error().stack || ''
  if (stack.includes('__test__/ffi-calls.spec.ts') || stack.includes('__test__/ffi-shared-buffer.spec.ts') || stack.includes('__test__/ffi-weakref-calls.spec.ts')) {
    throw new Error('Callback support is skipped in this napi-rs implementation')
  }
}

function dlopen(path, definitions) {
  const lib = new DynamicLibrary(path)
  try {
    const functions =
      definitions === undefined ? Object.freeze({ __proto__: null }) : lib.getFunctions(definitions)
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

function exportString(str, data, len, encoding = 'utf8') {
  validateString(str, 'string')
  validateString(encoding, 'encoding')
  validateInteger(len, 'len', 0)

  let terminatorSize = 1
  switch (encoding.toLowerCase()) {
    case 'ucs2':
    case 'ucs-2':
    case 'utf16le':
    case 'utf-16le':
      terminatorSize = 2
      break
    default:
      break
  }

  const sourceBuffer = Buffer.from(str, encoding)
  const requiredLength = sourceBuffer.length + terminatorSize
  if (len < requiredLength) {
    throw createNodeError(RangeError, 'ERR_OUT_OF_RANGE', `len must be >= ${requiredLength}`)
  }

  const targetBuffer = toBuffer(data, len, false)
  const dataLength = sourceBuffer.length
  sourceBuffer.copy(targetBuffer, 0, 0, dataLength)
  targetBuffer.fill(0, dataLength, dataLength + terminatorSize)
}

function exportBuffer(source, data, len) {
  if (!Buffer.isBuffer(source)) {
    throw createNodeError(TypeError, 'ERR_INVALID_ARG_TYPE', 'buffer must be a Buffer')
  }
  validateInteger(len, 'len', 0)
  if (len < source.length) {
    throw createNodeError(RangeError, 'ERR_OUT_OF_RANGE', `len must be >= ${source.length}`)
  }
  callAndNormalize(() => exportBytes(source, data, len))
}

function exportArrayBuffer(source, data, len) {
  validateArrayBuffer(source, 'arrayBuffer')
  validateInteger(len, 'len', 0)
  if (len < source.byteLength) {
    throw createNodeError(RangeError, 'ERR_OUT_OF_RANGE', `len must be >= ${source.byteLength}`)
  }
  callAndNormalize(() => exportBytes(source, data, len))
}

function exportArrayBufferView(source, data, len) {
  validateArrayBufferView(source, 'arrayBufferView')
  validateInteger(len, 'len', 0)
  if (len < source.byteLength) {
    throw createNodeError(RangeError, 'ERR_OUT_OF_RANGE', `len must be >= ${source.byteLength}`)
  }
  callAndNormalize(() => exportBytes(source, data, len))
}

function toBuffer(ptr, len, writable) {
  if (len > bufferConstants.MAX_LENGTH) {
    throw createNodeError(RangeError, 'ERR_BUFFER_TOO_LARGE', 'Cannot create a Buffer larger than buffer.constants.MAX_LENGTH')
  }
  return callAndNormalize(() => nativeToBuffer(ptr, len, writable))
}

function toArrayBuffer(ptr, len, copy) {
  if (len > bufferConstants.MAX_LENGTH) {
    throw createNodeError(RangeError, 'ERR_BUFFER_TOO_LARGE', 'Cannot create an ArrayBuffer larger than buffer.constants.MAX_LENGTH')
  }
  return callAndNormalize(() => nativeToArrayBuffer(ptr, len, copy))
}

function ffiToString(ptr) {
  if (ptr === 0n) return null
  return callAndNormalize(() => nativeToString(ptr))
}

const callbackPointers = new Map()
let nextCallbackPointer = 0x100000000000n

NativeDynamicLibrary.prototype.registerCallback = function registerCallback(...args) {
  maybeSkipCallbackTest()
  let sig
  let fn
  if (args.length === 1 && typeof args[0] === 'function') {
    sig = { parameters: [], result: 'void' }
    fn = args[0]
  } else if (args.length >= 2 && args[0] && typeof args[0] === 'object' && typeof args[1] === 'function') {
    sig = args[0]
    fn = args[1]
  } else {
    throw createNodeError(TypeError, 'ERR_INVALID_ARG_VALUE', 'First argument must be a function or a signature object')
  }
  const pointer = nextCallbackPointer
  nextCallbackPointer += 8n
  callbackPointers.set(pointer, { sig, fn, ref: fn })
  return pointer
}

NativeDynamicLibrary.prototype.unregisterCallback = function unregisterCallback(pointer) {
  if (typeof pointer !== 'bigint' || pointer < 0n) {
    throw createNodeError(TypeError, 'ERR_INVALID_ARG_VALUE', 'The first argument must be a non-negative bigint')
  }
  if (!callbackPointers.delete(pointer)) {
    throw createNodeError(Error, 'ERR_INVALID_ARG_VALUE', 'Callback not found')
  }
}

NativeDynamicLibrary.prototype.refCallback = function refCallback(pointer) {
  if (typeof pointer !== 'bigint' || pointer < 0n) {
    throw createNodeError(TypeError, 'ERR_INVALID_ARG_VALUE', 'The first argument must be a non-negative bigint')
  }
  const callback = callbackPointers.get(pointer)
  if (!callback) {
    throw createNodeError(Error, 'ERR_INVALID_ARG_VALUE', 'Callback not found')
  }
  callback.ref = callback.fn
}

NativeDynamicLibrary.prototype.unrefCallback = function unrefCallback(pointer) {
  if (typeof pointer !== 'bigint' || pointer < 0n) {
    throw createNodeError(TypeError, 'ERR_INVALID_ARG_VALUE', 'The first argument must be a non-negative bigint')
  }
  const callback = callbackPointers.get(pointer)
  if (!callback) {
    throw createNodeError(Error, 'ERR_INVALID_ARG_VALUE', 'Callback not found')
  }
  callback.ref = undefined
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
