'use strict'

const { validateNoNullBytes } = require('./ffi-errors')

function normalizeSignature(definition = {}) {
  const result = definition.returns ?? definition.return ?? definition.result ?? 'void'
  const args = definition.parameters ?? definition.arguments ?? []
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
