'use strict'

const U64_MAX = (1n << 64n) - 1n
const I64_MAX = (1n << 63n) - 1n
const I64_MIN = -(1n << 63n)

function createValidator(typeName) {
  switch (typeName) {
    case 'bool':
      return { kind: 'int', min: 0, max: 255, label: 'a uint8' }
    case 'char':
    case 'i8':
    case 'int8':
      return { kind: 'int', min: -128, max: 127, label: 'an int8' }
    case 'u8':
    case 'uint8':
      return { kind: 'int', min: 0, max: 255, label: 'a uint8' }
    case 'i16':
    case 'int16':
      return { kind: 'int', min: -32768, max: 32767, label: 'an int16' }
    case 'u16':
    case 'uint16':
      return { kind: 'int', min: 0, max: 65535, label: 'a uint16' }
    case 'i32':
    case 'int32':
      return { kind: 'int', min: -2147483648, max: 2147483647, label: 'an int32' }
    case 'u32':
    case 'uint32':
      return { kind: 'int', min: 0, max: 4294967295, label: 'a uint32' }
    case 'i64':
    case 'int64':
      return { kind: 'i64', min: I64_MIN, max: I64_MAX, label: 'an int64' }
    case 'u64':
    case 'uint64':
      return { kind: 'u64', min: 0n, max: U64_MAX, label: 'a uint64' }
    case 'f32':
    case 'float':
    case 'float32':
      return { kind: 'float', label: 'a float' }
    case 'f64':
    case 'double':
    case 'float64':
      return { kind: 'float', label: 'a double' }
    case 'pointer':
    case 'ptr':
    case 'function':
    case 'buffer':
    case 'arraybuffer':
    case 'string':
    case 'str':
      return { kind: 'pointer' }
    case 'void':
      return { kind: 'void' }
    default:
      return null
  }
}

module.exports = {
  createValidator,
  I64_MAX,
  I64_MIN,
  U64_MAX,
}
