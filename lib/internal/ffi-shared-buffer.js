'use strict'

const { invalidArgValue } = require('./ffi-errors')
const { U64_MAX, I64_MAX, I64_MIN } = require('./ffi-semantics')

const kSbSharedBuffer = Symbol('ffi.kSbSharedBuffer')
const kSbInvokeSlow = Symbol('ffi.kSbInvokeSlow')
const kSbArguments = Symbol('ffi.kSbArguments')
const kSbReturn = Symbol('ffi.kSbReturn')

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

function hasPointerLikeType(args) {
  for (const type of args) {
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

function attachRawMetadata(rawFn, args, resultType) {
  const paramTypes = args.slice()
  Object.defineProperty(rawFn, kSbSharedBuffer, {
    value: Object.freeze({ eligible: true }),
    enumerable: false,
    configurable: false,
    writable: false,
  })
  Object.defineProperty(rawFn, kSbArguments, {
    value: paramTypes,
    enumerable: false,
    configurable: false,
    writable: false,
  })
  Object.defineProperty(rawFn, kSbReturn, {
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

function wrapFunction(rawFn, argumentTypes, returnType, options = {}) {
  const { exposeMetadata = true } = options
  const validators = argumentTypes.map((type, index) => compileValidator(type, index))
  const wrapped = (..._args) => {
    if (_args.length !== argumentTypes.length) {
      throw invalidArgValue(`Invalid argument count: expected ${argumentTypes.length}, got ${_args.length}`)
    }
    for (let i = 0; i < validators.length; i++) {
      validators[i]?.(_args[i])
    }
    return rawFn(..._args)
  }
  if (exposeMetadata) {
    attachRawMetadata(wrapped, argumentTypes, returnType)
  }
  return wrapped
}

module.exports = {
  kSbInvokeSlow,
  kSbArguments,
  kSbReturn,
  kSbSharedBuffer,
  wrapFunction,
}
