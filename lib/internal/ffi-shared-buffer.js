'use strict'

const { invalidArgValue } = require('./ffi-errors')
const { U64_MAX, I64_MAX, I64_MIN } = require('./ffi-semantics')

const kSbSharedBuffer = Symbol('ffi.kSbSharedBuffer')
const kSbInvokeSlow = Symbol('ffi.kSbInvokeSlow')
const kSbParams = Symbol('ffi.kSbParams')
const kSbResult = Symbol('ffi.kSbResult')

function compileValidator(type, index) {
  switch (type) {
    case 'bool':
    case 'u8':
    case 'uint8':
      return (arg) => {
        if (typeof arg !== 'number' || !Number.isInteger(arg) || arg < 0 || arg > 255) {
          throw invalidArgValue(`Argument ${index} must be a uint8`)
        }
      }
    case 'char':
    case 'i8':
    case 'int8':
      return (arg) => {
        if (typeof arg !== 'number' || !Number.isInteger(arg) || arg < -128 || arg > 127) {
          throw invalidArgValue(`Argument ${index} must be an int8`)
        }
      }
    case 'i16':
    case 'int16':
      return (arg) => {
        if (typeof arg !== 'number' || !Number.isInteger(arg) || arg < -32768 || arg > 32767) {
          throw invalidArgValue(`Argument ${index} must be an int16`)
        }
      }
    case 'u16':
    case 'uint16':
      return (arg) => {
        if (typeof arg !== 'number' || !Number.isInteger(arg) || arg < 0 || arg > 65535) {
          throw invalidArgValue(`Argument ${index} must be a uint16`)
        }
      }
    case 'i32':
    case 'int32':
      return (arg) => {
        if (typeof arg !== 'number' || !Number.isInteger(arg) || arg < -2147483648 || arg > 2147483647) {
          throw invalidArgValue(`Argument ${index} must be an int32`)
        }
      }
    case 'u32':
    case 'uint32':
      return (arg) => {
        if (typeof arg !== 'number' || !Number.isInteger(arg) || arg < 0 || arg > 4294967295) {
          throw invalidArgValue(`Argument ${index} must be a uint32`)
        }
      }
    case 'i64':
    case 'int64':
      return (arg) => {
        if (typeof arg !== 'bigint' || arg < I64_MIN || arg > I64_MAX) {
          throw invalidArgValue(`Argument ${index} must be an int64`)
        }
      }
    case 'u64':
    case 'uint64':
      return (arg) => {
        if (typeof arg !== 'bigint' || arg < 0n || arg > U64_MAX) {
          throw invalidArgValue(`Argument ${index} must be a uint64`)
        }
      }
    case 'f32':
    case 'float':
    case 'float32':
      return (arg) => {
        if (typeof arg !== 'number') {
          throw invalidArgValue(`Argument ${index} must be a float`)
        }
      }
    case 'f64':
    case 'double':
    case 'float64':
      return (arg) => {
        if (typeof arg !== 'number') {
          throw invalidArgValue(`Argument ${index} must be a double`)
        }
      }
    case 'pointer':
    case 'ptr':
    case 'function':
    case 'buffer':
    case 'arraybuffer':
    case 'string':
    case 'str':
      return (arg) => {
        if (typeof arg === 'bigint') {
          if (arg < 0n || arg > U64_MAX) {
            throw invalidArgValue(`Argument ${index} must be a non-negative pointer bigint`)
          }
          return
        }
        if (arg === null || arg === undefined) return
        if (typeof arg === 'string') return
        if (Buffer.isBuffer(arg)) return
        if (arg instanceof ArrayBuffer) return
        if (ArrayBuffer.isView(arg)) return
        throw invalidArgValue('Argument must be a buffer, an ArrayBuffer, a string, or a bigint')
      }
    default:
      return null
  }
}

function hasPointerLikeType(parameters) {
  for (const type of parameters) {
    switch (type) {
      case 'pointer':
      case 'ptr':
      case 'function':
      case 'buffer':
      case 'arraybuffer':
      case 'string':
      case 'str':
        return true
      default:
        break
    }
  }
  return false
}

function attachRawMetadata(rawFn, parameters, resultType) {
  const paramTypes = parameters.slice()
  Object.defineProperty(rawFn, kSbSharedBuffer, {
    value: Object.freeze({ eligible: true }),
    enumerable: false,
    configurable: false,
    writable: false,
  })
  Object.defineProperty(rawFn, kSbParams, {
    value: paramTypes,
    enumerable: false,
    configurable: false,
    writable: false,
  })
  Object.defineProperty(rawFn, kSbResult, {
    value: resultType,
    enumerable: false,
    configurable: false,
    writable: false,
  })
  if (hasPointerLikeType(paramTypes)) {
    Object.defineProperty(rawFn, kSbInvokeSlow, {
      value: true,
      enumerable: false,
      configurable: false,
      writable: false,
    })
  }
  return rawFn
}

function wrapFunction(rawFn, parameters, resultType, options = {}) {
  const { exposeMetadata = true } = options
  const validators = parameters.map((type, index) => compileValidator(type, index))
  const wrapped = (...args) => {
    if (args.length !== parameters.length) {
      throw invalidArgValue(`Invalid argument count: expected ${parameters.length}, got ${args.length}`)
    }
    for (let i = 0; i < validators.length; i++) {
      validators[i]?.(args[i])
    }
    return rawFn(...args)
  }
  if (exposeMetadata) {
    attachRawMetadata(wrapped, parameters, resultType)
  }
  return wrapped
}

module.exports = {
  kSbInvokeSlow,
  kSbParams,
  kSbResult,
  kSbSharedBuffer,
  wrapFunction,
}
