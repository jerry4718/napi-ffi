'use strict'
const assert = require('node:assert')
const { spawnSync } = require('node:child_process')
function spawnSyncAndAssert(...args) {
  const options = args.at(-1) && !Array.isArray(args.at(-1)) && typeof args.at(-1) === 'object' ? args.pop() : {}
  const result = spawnSync(...args, { encoding: 'utf8' })
  if ('stdout' in options) assert.strictEqual(result.stdout, options.stdout)
  if ('stderr' in options) assert.match(result.stderr, options.stderr)
  return result
}
module.exports = { spawnSyncAndAssert }
