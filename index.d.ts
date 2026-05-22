export type TypedArray =
  | Int8Array
  | Uint8Array
  | Uint8ClampedArray
  | Int16Array
  | Uint16Array
  | Int32Array
  | Uint32Array
  | Float32Array
  | Float64Array
  | BigInt64Array
  | BigUint64Array

export type FFITypeName =
  | 'void'
  | 'pointer'
  | 'buffer'
  | 'arraybuffer'
  | 'function'
  | 'bool'
  | 'char'
  | 'string'
  | 'float'
  | 'double'
  | 'int8'
  | 'uint8'
  | 'int16'
  | 'uint16'
  | 'int32'
  | 'uint32'
  | 'int64'
  | 'uint64'
  | 'float32'
  | 'float64'

export interface FFISignature {
  returns?: FFITypeName
  return?: FFITypeName
  result?: FFITypeName
  parameters?: FFITypeName[]
  arguments?: FFITypeName[]
}

export interface DynamicLibraryHandle {
  lib: DynamicLibrary
  functions: Record<string, (...args: unknown[]) => unknown>
  [Symbol.dispose](): void
}

export declare class DynamicLibrary {
  constructor(path?: string | null)
  get path(): string
  get symbols(): object
  get functions(): object
  close(): void
  getSymbol(name: string): bigint
  getSymbols(): object
  getFunction(name: string, sig: object): (...args: unknown[]) => unknown
  getFunctions(definitions?: object | null): object
  registerCallback(sigOrFn: unknown, fnOpt?: unknown | null): bigint
  unregisterCallback(ptr: bigint): void
  refCallback(ptr: bigint): void
  unrefCallback(ptr: bigint): void
}

export declare const types: Readonly<{
  VOID: 'void'
  POINTER: 'pointer'
  BUFFER: 'buffer'
  ARRAY_BUFFER: 'arraybuffer'
  FUNCTION: 'function'
  BOOL: 'bool'
  CHAR: 'char'
  STRING: 'string'
  FLOAT: 'float'
  DOUBLE: 'double'
  INT_8: 'int8'
  UINT_8: 'uint8'
  INT_16: 'int16'
  UINT_16: 'uint16'
  INT_32: 'int32'
  UINT_32: 'uint32'
  INT_64: 'int64'
  UINT_64: 'uint64'
  FLOAT_32: 'float32'
  FLOAT_64: 'float64'
}>

export declare const suffix: 'dll' | 'dylib' | 'so'

export declare function dlopen(path?: string | null, definitions?: object): DynamicLibraryHandle
export declare function dlclose(handle: DynamicLibrary): void
export declare function dlsym(handle: DynamicLibrary, symbol: string): bigint

export declare function exportString(str: string, data: bigint, len: number, encoding?: BufferEncoding): void
export declare function exportBuffer(source: Buffer, data: bigint, len: number): void
export declare function exportArrayBuffer(source: ArrayBuffer, data: bigint, len: number): void
export declare function exportArrayBufferView(source: ArrayBufferView, data: bigint, len: number): void

export declare function getRawPointer(source: unknown): bigint
export declare function toString(ptr: bigint): string
export declare function toBuffer(ptr: bigint, len: number, writable?: boolean | null): Buffer
export declare function toArrayBuffer(ptr: bigint, len: number, copy?: boolean | null): ArrayBuffer

export declare function getInt8(ptr: bigint, offset?: number | null): number
export declare function getUint8(ptr: bigint, offset?: number | null): number
export declare function getInt16(ptr: bigint, offset?: number | null): number
export declare function getUint16(ptr: bigint, offset?: number | null): number
export declare function getInt32(ptr: bigint, offset?: number | null): number
export declare function getUint32(ptr: bigint, offset?: number | null): number
export declare function getInt64(ptr: bigint, offset?: number | null): bigint
export declare function getUint64(ptr: bigint, offset?: number | null): bigint
export declare function getFloat32(ptr: bigint, offset?: number | null): number
export declare function getFloat64(ptr: bigint, offset?: number | null): number

export declare function setInt8(ptr: bigint, offset: number, value: number): void
export declare function setUint8(ptr: bigint, offset: number, value: number): void
export declare function setInt16(ptr: bigint, offset: number, value: number): void
export declare function setUint16(ptr: bigint, offset: number, value: number): void
export declare function setInt32(ptr: bigint, offset: number, value: number): void
export declare function setUint32(ptr: bigint, offset: number, value: number): void
export declare function setInt64(ptr: bigint, offset: number, value: bigint): void
export declare function setUint64(ptr: bigint, offset: number, value: bigint): void
export declare function setFloat32(ptr: bigint, offset: number, value: number): void
export declare function setFloat64(ptr: bigint, offset: number, value: number): void

export declare function plus100(input: number): number
