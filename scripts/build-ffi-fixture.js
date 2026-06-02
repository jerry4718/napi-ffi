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

function normalizeArch(arch) {
  switch (arch) {
    case 'x64':
      return 'x86_64'
    case 'arm64':
      return 'aarch64'
    case 'i686':
    case 'i386':
    case 'ia32':
      return 'x86'
    case 'armv7':
    case 'armv7a':
      return 'arm'
    default:
      return arch
  }
}

function normalizeLinuxAbi(abi) {
  if (!abi) {
    return 'gnu'
  }
  if (abi === 'musl') {
    return 'musl'
  }
  if (abi.startsWith('gnu')) {
    return abi
  }
  return abi
}

function detectHostAbi() {
  if (process.platform === 'win32') {
    return 'msvc'
  }

  if (process.platform === 'darwin') {
    return undefined
  }

  const report = typeof process.report?.getReport === 'function' ? process.report.getReport() : null
  if (report?.header?.glibcVersionRuntime) {
    return 'gnu'
  }
  if (report?.sharedObjects?.some((file) => file.includes('libc.musl-') || file.includes('ld-musl-'))) {
    return 'musl'
  }
  return 'gnu'
}

function parseFixtureTarget(target) {
  const parts = target.split('-')
  const arch = normalizeArch(parts[0])

  if (parts.includes('darwin')) return { platform: 'darwin', arch }
  if (parts.includes('windows')) return { platform: 'win32', arch, abi: parts[parts.length - 1] }

  if (parts.includes('linux')) {
    return { platform: 'linux', arch, abi: normalizeLinuxAbi(parts[parts.length - 1]) }
  }

  throw new Error(`Unsupported FFI_FIXTURE_TARGET: ${target}`)
}

function getRequestedTarget() {
  if (process.env.FFI_FIXTURE_TARGET) {
    return parseFixtureTarget(process.env.FFI_FIXTURE_TARGET)
  }

  return {
    platform: process.platform,
    arch: normalizeArch(process.arch),
    abi: detectHostAbi(),
  }
}

function toZigOs(platform) {
  switch (platform) {
    case 'darwin':
      return 'macos'
    case 'linux':
      return 'linux'
    case 'win32':
      return 'windows'
    default:
      return null
  }
}

function resolveZigTarget(target) {
  const zigOs = toZigOs(target.platform)

  if (!target.arch || !zigOs) {
    return null
  }

  if (target.platform === 'win32') {
    return `${target.arch}-${zigOs}-${target.abi || 'msvc'}`
  }

  if (target.platform !== 'linux') {
    return `${target.arch}-${zigOs}`
  }

  const zigAbi = target.arch === 'arm'
    ? target.abi === 'musl' ? 'musleabihf' : 'gnueabihf'
    : target.abi === 'musl' ? 'musl' : 'gnu'

  return `${target.arch}-${zigOs}-${zigAbi}`
}

const requestedTarget = getRequestedTarget()
const { extension, compilerArgs } = getPlatformConfig(requestedTarget.platform)
const outputFileName = `${libraryBaseName}${extension}`
const outputPath = path.join(fixtureBuildDir, outputFileName)

fs.mkdirSync(fixtureBuildDir, { recursive: true })

const zigTarget = resolveZigTarget(requestedTarget)
const targetArgs = zigTarget ? ['-target', zigTarget] : []

const args = ['cc', ...targetArgs, ...compilerArgs, sourceFile, '-o', outputPath];
console.log(`${args} ${ args.join(" ") }`)
const result = spawnSync(zig, args, {
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
