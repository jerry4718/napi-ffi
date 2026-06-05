# `@mmty/napi-ffi`

A native FFI library for Node.js, implemented with `napi-rs`.

## Why this project exists

This project is intended to provide a userland implementation of `node:ffi`.

The goal is straightforward:

- let users write code in a `node:ffi`-style API today
- make that code easy to migrate later
- keep the surface area close enough that code built on this library can switch to `node:ffi` with minimal or no application-level changes

In other words, this package aims to be a practical compatibility layer for people who want the ergonomics of `node:ffi` before it is universally available or stable enough for their use case.

## Status

This package already supports the core building blocks needed for many FFI scenarios:

- dynamic library loading
- symbol lookup
- typed foreign function calls
- callbacks from native code into JavaScript
- raw pointer-based memory access helpers
- string, `Buffer`, `ArrayBuffer`, and typed-array interop

## Installation

```bash
pnpm add @mmty/napi-ffi
```

## Quick start

### Load a library and call native functions

```js
const ffi = require('@mmty/napi-ffi')

const { lib, functions } = ffi.dlopen('./libmath.so', {
  add_i32: {
    parameters: ['i32', 'i32'],
    result: 'i32',
  },
  multiply_f64: {
    parameters: ['f64', 'f64'],
    result: 'f64',
  },
})

try {
  console.log(functions.add_i32(20, 22))
  console.log(functions.multiply_f64(6, 7))
} finally {
  lib.close()
}
```

### Use strings and pointers

```js
const ffi = require('@mmty/napi-ffi')

const { lib, functions } = ffi.dlopen('./libstrings.so', {
  string_length: {
    parameters: ['pointer'],
    result: 'u64',
  },
  string_duplicate: {
    parameters: ['pointer'],
    result: 'pointer',
  },
  free_string: {
    parameters: ['pointer'],
    result: 'void',
  },
})

try {
  console.log(functions.string_length('hello ffi'))

  const ptr = functions.string_duplicate('copied from JS')
  console.log(ffi.toString(ptr))
  functions.free_string(ptr)
} finally {
  lib.close()
}
```

### Register a JavaScript callback

```js
const ffi = require('@mmty/napi-ffi')

const { lib, functions } = ffi.dlopen('./libcallbacks.so', {
  call_binary_int_callback: {
    parameters: ['function', 'i32', 'i32'],
    result: 'i32',
  },
})

const callback = lib.registerCallback(
  {
    parameters: ['i32', 'i32'],
    result: 'i32',
  },
  (a, b) => a + b,
)

try {
  console.log(functions.call_binary_int_callback(callback, 19, 23))
} finally {
  lib.unregisterCallback(callback)
  lib.close()
}
```

### Read and write native memory

```js
const ffi = require('@mmty/napi-ffi')

const { lib, functions } = ffi.dlopen('./libmemory.so', {
  allocate_memory: {
    parameters: ['u64'],
    result: 'pointer',
  },
  deallocate_memory: {
    parameters: ['pointer'],
    result: 'void',
  },
})

const ptr = functions.allocate_memory(16n)

try {
  ffi.setInt32(ptr, 0, 42)
  ffi.setFloat64(ptr, 8, 3.5)

  console.log(ffi.getInt32(ptr, 0))
  console.log(ffi.getFloat64(ptr, 8))
} finally {
  functions.deallocate_memory(ptr)
  lib.close()
}
```

## API overview

### Main exports

- `dlopen(path?, definitions?)`
- `dlclose(handle)`
- `dlsym(handle, symbol)`
- `DynamicLibrary`
- `types`
- memory helpers such as `getInt32`, `setInt32`, `toString`, `toBuffer`, `toArrayBuffer`, and `getRawPointer`

### Supported FFI types

- `void`
- `i8`, `u8`, `bool`, `char`
- `i16`, `u16`
- `i32`, `u32`
- `i64`, `u64`
- `f32`, `f64`
- `pointer`
- `string`
- `buffer`
- `arraybuffer`
- `function`

Aliases such as `int32`, `uint64`, `float`, `double`, `ptr`, and `str` are also supported.

## Compatibility notes

This project is designed around a `node:ffi`-like programming model, but it is not a byte-for-byte clone of every current or future Node core detail.

The compatibility goal is:

- similar concepts
- similar signatures
- similar calling style
- low-friction migration path for users who want to move to `node:ffi`

If you are building on top of this package, it is a good idea to keep your FFI definitions isolated behind a small adapter module. That makes future switching to `node:ffi` even easier.

## Type behavior

A few behaviors are intentionally strict:

- 64-bit integers use JavaScript `BigInt`
- smaller integer and floating-point types use JavaScript `Number`
- pointer-like arguments accept `bigint`, `Buffer`, `ArrayBuffer`, typed arrays, `null`, and `undefined`
- string returns are represented as pointers, so you control ownership and freeing explicitly

## Development

### Requirements

- Rust
- Node.js
- `pnpm`
- `corepack enable`

### Build

```bash
pnpm install
pnpm build
```

### Test

```bash
pnpm test
```

### Lint and format

```bash
pnpm lint
pnpm format
cargo fmt -- --check
cargo clippy
```

## Notes for contributors

- The native addon is implemented in Rust under `src/`.
- `index.js` is generated/wrapper-facing glue and should not be treated like hand-written API design documentation.
- Error wording matters in several validation paths because tests depend on it.

## License

MIT
