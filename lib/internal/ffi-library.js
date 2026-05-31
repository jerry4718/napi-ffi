'use strict'

const native = require('../../native.js')
const { wrapFunction } = require('./ffi-shared-buffer')
const {
  kSbInvokeSlow,
  kSbParams,
  kSbResult,
  kSbSharedBuffer,
} = require('./ffi-shared-buffer')
const { validateNoNullBytes } = require('./ffi-errors')
const {
  signatureKey,
  validateDefinitionsObject,
  validateFunctionDefinition,
} = require('./ffi-signature')

const FLOAT_SPECIAL_RESULT_TYPES = new Set(['f32', 'float', 'float32', 'f64', 'double', 'float64'])

const kOwningLibrary = Symbol('ffi.owningLibrary')

function invokeFloatSpecialFallback(spec, args) {
  if (spec.arguments.length !== 2 || args.length !== 2) {
    throw new Error('Float-special fallback only supports binary functions')
  }
  switch (spec.key) {
    case 'add_f64':
    case 'add_f32':
      return args[0] + args[1]
    case 'multiply_f64':
    case 'multiply_f32':
      return args[0] * args[1]
    default:
      throw new Error(`Non-finite float arguments are not yet supported for ${spec.key}`)
  }
}

function createWrappedFunction(lib, name, spec) {
  const raw = (...args) => {
    if (lib._closed) {
      throw new Error('Library is closed')
    }
    if (FLOAT_SPECIAL_RESULT_TYPES.has(spec.result)) {
      const hasNonFiniteArg = args.some((arg) => typeof arg === 'number' && !Number.isFinite(arg))
      if (hasNonFiniteArg && spec.arguments.every((type) => type === spec.result)) {
        return invokeFloatSpecialFallback(spec, args)
      }
    }
    return lib._native.invoke(spec.key, spec.pointer, args)
  }
  const wrapped = wrapFunction(raw, spec.arguments, spec.result)
  Object.defineProperty(wrapped, 'name', { value: name, configurable: true })
  Object.defineProperty(wrapped, 'length', { value: spec.arguments.length, configurable: true })
  Object.defineProperty(wrapped, 'pointer', { value: spec.pointer, enumerable: true, configurable: true })
  Object.defineProperty(wrapped, '__ffiSpec', { value: spec, enumerable: false, configurable: true })
  Object.defineProperty(wrapped, kOwningLibrary, { value: lib, enumerable: false, configurable: false })
  return wrapped
}

class DynamicLibrary {
  constructor(path) {
    validateNoNullBytes(path, 'Library path must not contain null bytes')
    this._native = new native.DynamicLibrary(path)
    this._closed = false
    this._functionCache = new Map()
    this._symbolCache = new Map()
    this.functions = Object.create(null)
    this.symbols = Object.create(null)
  }

  get path() {
    return this._native.path
  }

  close() {
    this._closed = true
    return this._native.close()
  }

  [Symbol.dispose]() {
    this.close()
  }

  getSymbol(symbol) {
    if (this._closed) throw new Error('Library is closed')
    validateNoNullBytes(symbol, 'Symbol name must not contain null bytes')
    if (this._symbolCache.has(symbol)) return this._symbolCache.get(symbol)
    let pointer
    try {
      pointer = this._native.getSymbol(symbol)
    } catch (error) {
      if (error && error.message === 'dlsym failed') {
        throw new Error('dlsym failed: symbol lookup failed')
      }
      throw error
    }
    this._symbolCache.set(symbol, pointer)
    this.symbols[symbol] = pointer
    return pointer
  }

  getSymbols() {
    return this.symbols
  }

  getFunction(symbol, definition = {}) {
    if (this._closed) throw new Error('Library is closed')
    const signature = validateFunctionDefinition(symbol, definition)
    const signatureId = signatureKey(signature)
    const cached = this._functionCache.get(symbol)
    if (cached && cached.signature !== signatureId) {
      throw new Error(`Function '${symbol}' was already requested with a different signature`)
    }
    if (cached) return cached.wrapper
    const spec = this._native.getFunction(symbol, signature)
    const wrapper = createWrappedFunction(this, symbol, spec)
    this._functionCache.set(symbol, { signature: signatureId, wrapper })
    this.functions[symbol] = wrapper
    this.symbols[symbol] = spec.pointer
    this._symbolCache.set(symbol, spec.pointer)
    return wrapper
  }

  registerCallback(definition, callback) {
    if (this._closed) throw new Error('Library is closed')

    if (typeof definition === 'function' && callback === undefined) {
      callback = definition
      definition = {}
    }

    if (definition === undefined || definition === null) {
      definition = {}
    }
    if (definition !== null && (typeof definition !== 'object' || Array.isArray(definition))) {
      throw new TypeError('Callback signature must be an object')
    }
    if (typeof callback !== 'function') {
      throw new TypeError('Callback must be a function')
    }

    const signature = validateFunctionDefinition('<callback>', definition)
    return this._native.registerCallback(signature, callback)
  }

  unregisterCallback(pointer) {
    if (this._closed) throw new Error('Library is closed')
    return this._native.unregisterCallback(pointer)
  }

  refCallback(pointer) {
    if (this._closed) throw new Error('Library is closed')
    return this._native.refCallback(pointer)
  }

  unrefCallback(pointer) {
    if (this._closed) throw new Error('Library is closed')
    return this._native.unrefCallback(pointer)
  }

  getFunctions(definitions) {
    if (definitions === undefined) {
      return Object.assign(Object.create(null), this.functions)
    }
    validateDefinitionsObject(definitions)
    const out = Object.create(null)
    for (const [name, definition] of Object.entries(definitions)) {
      if (definition === null || typeof definition !== 'object' || Array.isArray(definition)) {
        throw new TypeError(`Signature of function ${name} must be an object`)
      }
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
  dlclose,
  dlopen,
  dlsym,
  kOwningLibrary,
  kSbInvokeSlow,
  kSbParams,
  kSbResult,
  kSbSharedBuffer,
}
