'use strict'

const { validateNoNullBytes } = require('./ffi-errors')

function canonicalTypeName(typeName) {
  switch (typeName) {
    case 'bool':
      return 'uint8'
    case 'char':
      return 'char'
    case 'i8':
      return 'int8'
    case 'u8':
      return 'uint8'
    case 'i16':
      return 'int16'
    case 'u16':
      return 'uint16'
    case 'i32':
      return 'int32'
    case 'u32':
      return 'uint32'
    case 'i64':
      return 'int64'
    case 'u64':
      return 'uint64'
    case 'f32':
    case 'float':
      return 'float32'
    case 'f64':
    case 'double':
      return 'float64'
    case 'ptr':
    case 'str':
      return typeName === 'ptr' ? 'pointer' : 'string'
    default:
      return typeName
  }
}

const SUPPORTED_TYPES = new Set([
  'void',
  'bool',
  'char',
  'int8',
  'uint8',
  'int16',
  'uint16',
  'int32',
  'uint32',
  'int64',
  'uint64',
  'float32',
  'float64',
  'pointer',
  'buffer',
  'arraybuffer',
  'function',
  'string',
])

function signatureKey(signature) {
  return JSON.stringify([signature.return, signature.arguments])
}

function validateTypeName(typeName) {
  if (!SUPPORTED_TYPES.has(canonicalTypeName(typeName))) {
    throw new Error(`Unsupported FFI type: ${typeName}`)
  }
}

const DefaultSignature = { return: 'void', arguments: [] }

function normalizeSignatureDefinition(definition) {
  return {
    return: 'return' in definition ? definition.return : DefaultSignature.return,
    arguments: 'arguments' in definition ? definition.arguments : DefaultSignature.arguments,
  }
}

function validateFunctionDefinition(symbol, definition) {
  if (definition === null || typeof definition !== 'object' || Array.isArray(definition)) {
    throw new TypeError(`Signature of function ${symbol} must be an object`)
  }

  // Preserve the older proxy trap behavior expected by tests while
  // normalizing to the current return+arguments signature shape.
  const signature = normalizeSignatureDefinition(definition)
  validateNoNullBytes(symbol, `Function name must not contain null bytes`)
  validateNoNullBytes(signature.return, `Return value type of function ${symbol} must not contain null bytes`)
  validateTypeName(signature.return)
  signature.arguments.forEach((arg, index) => {
    validateNoNullBytes(arg, `Argument ${index} of function ${symbol} must not contain null bytes`)
    validateTypeName(arg)
  })
  return signature
}

function validateDefinitionsObject(definitions) {
  if (definitions === null || typeof definitions !== 'object' || Array.isArray(definitions)) {
    throw new TypeError('Functions signatures must be an object')
  }
}

module.exports = {
  signatureKey,
  validateDefinitionsObject,
  validateFunctionDefinition,
}
