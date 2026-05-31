'use strict'

const native = require('../../native.js')
const { invalidArgValue, validateNoNullBytes } = require('./ffi-errors')
const {
  signatureKey,
  validateDefinitionsObject,
  validateFunctionDefinition,
} = require('./ffi-signature')

const kOwningLibrary = Symbol('ffi.owningLibrary')

function callBySpec(lib, spec, args) {
  if (args.length !== spec.arguments.length) {
    throw invalidArgValue(`Invalid argument count: expected ${spec.arguments.length}, got ${args.length}`)
  }
  if (lib._closed) {
    throw new Error('Library is closed')
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
    const pointer = this._native.getSymbol(symbol)
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
}
