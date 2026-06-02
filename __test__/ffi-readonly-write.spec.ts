import test from 'ava'
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
// Flags: 
const { isWindows } = require('./common');
const assert = require('node:assert');
const { spawnSync } = require('node:child_process');

const { fixtureSymbols, libraryPath } = require('./ffi-test-common');

test('writing to readonly memory via buffer fails', (t) => {
  if (isWindows) {
    t.pass()
    return
  }
  const symbols = JSON.stringify(fixtureSymbols);
  const libPath = JSON.stringify(libraryPath);
  const { stdout, status } = spawnSync(process.execPath, [
        '-p',
    `
    const ffi = require('../index.js');
    const { functions } = ffi.dlopen(${libPath}, ${symbols})
    const p = functions.readonly_memory();
    const b = ffi.toBuffer(p, 4096, false);
    b[0] = 42;
    console.log('success');
    `,
  ]);
  assert.notStrictEqual(status, 0);
  assert.strictEqual(stdout.length, 0);
});
