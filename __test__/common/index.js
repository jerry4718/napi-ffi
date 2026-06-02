'use strict'

const buildType = process.env.BUILDTYPE || 'Debug'
const isWindows = process.platform === 'win32'
function mustCall(fn) { return fn }
function nodeProcessAborted(status, signal) { return signal !== null || status !== 0 }
module.exports = { buildType, isWindows, mustCall, nodeProcessAborted }
