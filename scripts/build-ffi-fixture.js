#!/usr/bin/env node

const { spawnSync } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')

const repoRoot = path.resolve(__dirname, '..')
const zig = process.env.ZIG || 'zig'
const libraryBaseName = 'ffi_test_library'

const sourceFile = path.join(repoRoot, '__test__', 'fixture_library', 'ffi_test_library.c')
const fixtureBuildDir = path.join(repoRoot, '__test__', 'fixture_library')

function getPlatformConfig(platform) {
  switch (platform) {
    case 'win32':
      return {
        extension: '.dll',
        compilerArgs: ['-shared'],
      }
    case 'darwin':
      return {
        extension: '.dylib',
        compilerArgs: ['-dynamiclib'],
      }
    default:
      return {
        extension: '.so',
        compilerArgs: ['-shared', '-fPIC'],
      }
  }
}

const { extension, compilerArgs } = getPlatformConfig(process.platform)
const outputFileName = `${libraryBaseName}${extension}`
const outputPath = path.join(fixtureBuildDir, outputFileName)

fs.mkdirSync(fixtureBuildDir, { recursive: true })

const result = spawnSync(zig, ['cc', ...compilerArgs, sourceFile, '-o', outputPath], {
  cwd: repoRoot,
  stdio: 'inherit',
})

if (result.error) {
  console.error(`Failed to start ${zig}:`, result.error.message)
  process.exit(1)
}

if (result.status !== 0) {
  process.exit(result.status ?? 1)
}

console.log(`Built fixture library: ${path.relative(repoRoot, outputPath)}`)
