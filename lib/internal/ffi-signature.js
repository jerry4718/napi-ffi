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

function normalizeSignature(definition = {}) {
  const result = canonicalTypeName(definition.returns ?? definition.return ?? definition.result ?? 'void')
  const args = (definition.parameters ?? definition.arguments ?? []).map(canonicalTypeName)
  return { result, arguments: args }
}

function signatureKey(signature) {
  return JSON.stringify([signature.result, signature.arguments])
}

function validateFunctionDefinition(symbol, definition) {
  if (definition === null || typeof definition !== 'object' || Array.isArray(definition)) {
    throw new TypeError(`Signature of function ${symbol} must be an object`)
  }

  const signature = normalizeSignature(definition)
  validateNoNullBytes(symbol, `Function name must not contain null bytes`)
  validateNoNullBytes(signature.result, `Return value type of function ${symbol} must not contain null bytes`)
  signature.arguments.forEach((arg, index) => {
    validateNoNullBytes(arg, `Argument ${index} of function ${symbol} must not contain null bytes`)
  })
  return signature
}

function validateDefinitionsObject(definitions) {
  if (definitions === null || typeof definitions !== 'object' || Array.isArray(definitions)) {
    throw new TypeError('Functions signatures must be an object')
  }
}

module.exports = {
  normalizeSignature,
  signatureKey,
  validateDefinitionsObject,
  validateFunctionDefinition,
}
