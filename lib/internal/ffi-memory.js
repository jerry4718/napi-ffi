'use strict'

const native = require('../../native.js')
const { constants: bufferConstants } = require('node:buffer')
const {
  invalidArgType,
  invalidBufferTooLarge,
} = require('./ffi-errors')

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
  if (len > bufferConstants.MAX_LENGTH) {
    throw invalidBufferTooLarge()
  }
}

function normalizeI64Value(value, label, signed = true) {
  let bigint
  if (typeof value === 'bigint') bigint = value
  else if (typeof value === 'number' && Number.isSafeInteger(value)) bigint = BigInt(value)
  else throw new TypeError(`Value must be ${label}`)

  if (signed) {
    if (bigint < -(1n << 63n) || bigint > ((1n << 63n) - 1n)) throw new TypeError(`Value must be ${label}`)
  } else if (bigint < 0n || bigint > ((1n << 64n) - 1n)) {
    throw new TypeError(`Value must be ${label}`)
  }
  return bigint
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

function normalizeNumberValue(value, label, min, max) {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < min || value > max) {
    throw new TypeError(`Value must be ${label}`)
  }
  return value
}

function wrapMemorySetter(setter, kind) {
  return (pointer, offset, value) => {
    if (offset === undefined) {
      throw new TypeError('Expected an offset argument')
    }
    if (typeof offset !== 'number') {
      throw new TypeError('The offset must be a number')
    }
    if (value === undefined) {
      throw new TypeError('Expected a value argument')
    }
    switch (kind) {
      case 'int8':
        return setter(pointer, offset, normalizeNumberValue(value, 'an int8', -128, 127))
      case 'uint8':
        return setter(pointer, offset, normalizeNumberValue(value, 'a uint8', 0, 255))
      case 'int16':
        return setter(pointer, offset, normalizeNumberValue(value, 'an int16', -32768, 32767))
      case 'uint16':
        return setter(pointer, offset, normalizeNumberValue(value, 'a uint16', 0, 65535))
      case 'int32':
        return setter(pointer, offset, normalizeNumberValue(value, 'an int32', -2147483648, 2147483647))
      case 'uint32':
        return setter(pointer, offset, normalizeNumberValue(value, 'a uint32', 0, 4294967295))
      case 'int64':
        return setter(pointer, offset, normalizeI64Value(value, 'an int64', true))
      case 'uint64':
        return setter(pointer, offset, normalizeI64Value(value, 'a uint64', false))
      default:
        return setter(pointer, offset, value)
    }
  }
}

function wrapMemoryGetter(getter, kind) {
  return (pointer, offset) => {
    const value = getter(pointer, offset)
    if (kind === 'int64' || kind === 'uint64') {
      return typeof value === 'bigint' ? value : BigInt(value)
    }
    return value
  }
}

module.exports = {
  exportArrayBuffer,
  exportArrayBufferView,
  exportBuffer,
  exportString,
  getFloat32: native.getFloat32,
  getFloat64: native.getFloat64,
  getInt16: native.getInt16,
  getInt32: native.getInt32,
  getInt64: wrapMemoryGetter(native.getInt64, 'int64'),
  getInt8: native.getInt8,
  getRawPointer,
  getUint16: native.getUint16,
  getUint32: native.getUint32,
  getUint64: wrapMemoryGetter(native.getUint64, 'uint64'),
  getUint8: native.getUint8,
  setFloat32: wrapMemorySetter(native.setFloat32),
  setFloat64: wrapMemorySetter(native.setFloat64),
  setInt16: wrapMemorySetter(native.setInt16, 'int16'),
  setInt32: wrapMemorySetter(native.setInt32, 'int32'),
  setInt64: wrapMemorySetter(native.setInt64, 'int64'),
  setInt8: wrapMemorySetter(native.setInt8, 'int8'),
  setUint16: wrapMemorySetter(native.setUint16, 'uint16'),
  setUint32: wrapMemorySetter(native.setUint32, 'uint32'),
  setUint64: wrapMemorySetter(native.setUint64, 'uint64'),
  setUint8: wrapMemorySetter(native.setUint8, 'uint8'),
  toArrayBuffer,
  toBuffer,
  toString: native.toString,
}
