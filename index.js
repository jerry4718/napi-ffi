'use strict'

const native = require('./native.js')

const {
  DynamicLibrary: NativeDynamicLibrary,
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
const kSbSharedBuffer = Symbol('ffi.kSbSharedBuffer')
const kSbInvokeSlow = Symbol('ffi.kSbInvokeSlow')
const kSbParams = Symbol('ffi.kSbParams')
const kSbResult = Symbol('ffi.kSbResult')
class DynamicLibrary extends NativeDynamicLibrary {}

const rawGetFunction = DynamicLibrary.prototype.getFunction
const rawGetFunctions = DynamicLibrary.prototype.getFunctions
const functionsDescriptor = Object.getOwnPropertyDescriptor(DynamicLibrary.prototype, 'functions')

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

function attachSharedBufferMetadata(rawFn, parameters, resultType) {
  if (rawFn === undefined || rawFn === null || typeof rawFn !== 'function') return rawFn

  const params = parameters ?? rawFn.__ffiArgTypes
  const result = resultType ?? rawFn.__ffiReturnType
  if (params === undefined || result === undefined) return rawFn

  Object.defineProperty(rawFn, kSbSharedBuffer, {
    value: new ArrayBuffer((params.length + 1) * 8),
    enumerable: false,
    configurable: false,
    writable: false,
  })
  Object.defineProperty(rawFn, kSbParams, {
    value: params,
    enumerable: false,
    configurable: false,
    writable: false,
  })
  Object.defineProperty(rawFn, kSbResult, {
    value: result,
    enumerable: false,
    configurable: false,
    writable: false,
  })
  if (params.includes('pointer') || params.includes('buffer') || params.includes('arraybuffer') || params.includes('string')) {
    Object.defineProperty(rawFn, kSbInvokeSlow, {
      value: rawFn,
      enumerable: false,
      configurable: false,
      writable: false,
    })
  }
  return rawFn
}

function sigParams(sig) {
  return sig.parameters ?? sig.arguments ?? []
}

function sigResult(sig) {
  return sig.result ?? sig.return ?? sig.returns ?? 'void'
}

function inheritMetadata(wrapper, rawFn, nargs) {
  Object.defineProperty(wrapper, 'name', {
    value: rawFn.name,
    configurable: true,
  })
  Object.defineProperty(wrapper, 'length', {
    value: nargs,
    configurable: true,
  })
  Object.defineProperty(wrapper, 'pointer', {
    value: rawFn.pointer,
    writable: true,
    configurable: true,
    enumerable: true,
  })
  return wrapper
}

function wrapWithSharedBuffer(rawFn, parameters, resultType) {
  attachSharedBufferMetadata(rawFn, parameters, resultType)
  const params = parameters ?? rawFn.__ffiArgTypes
  if (params === undefined) return rawFn
  return inheritMetadata(function(...args) {
    return rawFn(...args)
  }, rawFn, params.length)
}

DynamicLibrary.prototype.getFunction = function getFunction(name, sig) {
  const raw = rawGetFunction.call(this, name, sig)
  attachSharedBufferMetadata(raw, sigParams(sig), sigResult(sig))
  return raw
}

DynamicLibrary.prototype.getFunctions = function getFunctions(definitions) {
  const raw = definitions === undefined ? rawGetFunctions.call(this) : rawGetFunctions.call(this, definitions)
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

if (functionsDescriptor?.get) {
  Object.defineProperty(DynamicLibrary.prototype, 'functions', {
    configurable: true,
    enumerable: functionsDescriptor.enumerable,
    get() {
      const raw = functionsDescriptor.get.call(this)
      if (raw === undefined || raw === null) return raw
      const out = { __proto__: null }
      for (const name of Object.keys(raw)) {
        out[name] = wrapWithSharedBuffer(raw[name])
      }
      return out
    },
  })
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

Object.defineProperties(module.exports, {
  kSbSharedBuffer: { value: kSbSharedBuffer },
  kSbInvokeSlow: { value: kSbInvokeSlow },
  kSbParams: { value: kSbParams },
  kSbResult: { value: kSbResult },
})
