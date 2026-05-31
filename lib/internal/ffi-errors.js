'use strict'

function invalidArgValue(message) {
  const err = new TypeError(message)
  err.code = 'ERR_INVALID_ARG_VALUE'
  return err
}

function invalidArgType(message) {
  const err = new TypeError(message)
  err.code = 'ERR_INVALID_ARG_TYPE'
  return err
}

function invalidBufferTooLarge() {
  const err = new RangeError('Cannot create a Buffer larger than the maximum size')
  err.code = 'ERR_BUFFER_TOO_LARGE'
  return err
}

function validateNoNullBytes(value, message) {
  if (typeof value === 'string' && value.includes('\0')) {
    throw new Error(message)
  }
}

module.exports = {
  invalidArgType,
  invalidArgValue,
  invalidBufferTooLarge,
  validateNoNullBytes,
}
