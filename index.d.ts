export type Buffer = Uint8Array

export type BufferEncoding =
  | 'ascii'
  | 'utf8'
  | 'utf-8'
  | 'utf16le'
  | 'utf-16le'
  | 'ucs2'
  | 'ucs-2'
  | 'base64'
  | 'base64url'
  | 'latin1'
  | 'binary'
  | 'hex'

export type FFIType =
  | 'void'
  | ('i8' | 'int8')
  | ('u8' | 'uint8' | 'bool' | 'char')
  | ('i16' | 'int16')
  | ('u16' | 'uint16')
  | ('i32' | 'int32')
  | ('u32' | 'uint32')
  | ('i64' | 'int64')
  | ('u64' | 'uint64')
  | ('f32' | 'float' | 'float32')
  | ('f64' | 'double' | 'float64')
  | ('ptr' | 'pointer')
  | ('str' | 'string')
  | 'buffer'
  | 'arraybuffer'
  | 'function'

export type FFISignature ={
  return: FFIType
  arguments: readonly FFIType[]
}

export type Definitions = Record<string, FFISignature>

export interface Types {
  readonly VOID: 'void'
  readonly POINTER: 'pointer'
  readonly BUFFER: 'buffer'
  readonly ARRAY_BUFFER: 'arraybuffer'
  readonly FUNCTION: 'function'
  readonly BOOL: 'bool'
  readonly CHAR: 'char'
  readonly STRING: 'string'
  readonly FLOAT: 'float'
  readonly DOUBLE: 'double'
  readonly INT_8: 'int8'
  readonly UINT_8: 'uint8'
  readonly INT_16: 'int16'
  readonly UINT_16: 'uint16'
  readonly INT_32: 'int32'
  readonly UINT_32: 'uint32'
  readonly INT_64: 'int64'
  readonly UINT_64: 'uint64'
  readonly FLOAT_32: 'float32'
  readonly FLOAT_64: 'float64'
}

type FFIArgValue<T extends FFIType> =
  T extends 'i8' | 'int8' | 'u8' | 'uint8' | 'bool' | 'char' | 'i16' | 'int16' | 'u16' | 'uint16' | 'i32' | 'int32' | 'u32' | 'uint32' | 'f32' | 'float' | 'float32' | 'f64' | 'double' | 'float64'
    ? number
    : T extends 'i64' | 'int64' | 'u64' | 'uint64'
      ? bigint
      : T extends 'pointer' | 'ptr' | 'buffer' | 'arraybuffer' | 'function'
        ? bigint | Buffer | ArrayBuffer | ArrayBufferView | null | undefined
        : T extends 'string' | 'str'
          ? string | bigint | Buffer | ArrayBuffer | ArrayBufferView | null | undefined
          : never

type FFIReturnValue<T extends FFIType | undefined> =
  T extends undefined | 'void'
    ? void
    : T extends 'i8' | 'int8' | 'u8' | 'uint8' | 'bool' | 'char' | 'i16' | 'int16' | 'u16' | 'uint16' | 'i32' | 'int32' | 'u32' | 'uint32' | 'f32' | 'float' | 'float32' | 'f64' | 'double' | 'float64'
      ? number
      : T extends 'i64' | 'int64' | 'u64' | 'uint64'
        ? bigint
        : bigint

type FFICallbackReturnValue<T extends FFIType | undefined> =
  T extends undefined | 'void'
    ? void
    : T extends 'pointer' | 'ptr' | 'buffer' | 'arraybuffer' | 'function'
      ? bigint | null | undefined
      : T extends 'string' | 'str'
        ? never
        : FFIReturnValue<T>

type SignatureReturn<S> = S extends { return: infer T extends FFIType } ? T : undefined
type SignatureArguments<S> = S extends { arguments: infer T extends readonly FFIType[] } ? T : readonly []

type MapFFIArgs<T> =
  T extends FFIType[] | []?
  T extends [infer H extends FFIType, ...infer R extends FFIType[]]
    ? [FFIArgValue<H>, ...MapFFIArgs<R>]
    : []
    :never

export type ForeignFunction<S> =
  & ((...args: MapFFIArgs<SignatureArguments<S>>) => FFIReturnValue<SignatureReturn<S>>)
  & { readonly pointer: bigint }

export type ResolvedFunctions<T extends Definitions> = {
  [K in keyof T]: ForeignFunction<T[K]>
}

export type ResolvedSymbols = Record<string, bigint>

export interface DlOpenResult<T extends Definitions | undefined = undefined> {
  lib: DynamicLibrary
  functions: T extends Definitions ? ResolvedFunctions<T> : Record<string, never>
  [Symbol.dispose](): void
}

export declare class CallbackRef {}

export declare class DynamicLibrary {
  constructor(path?: string | null)

  get path(): string | null
  functions: Record<string, ForeignFunction<unknown>>
  symbols: Record<string, bigint>

  close(): void
  [Symbol.dispose](): void

  getFunction<S extends FFISignature>(name: string, signature: S): ForeignFunction<S>
  getFunctions(): Record<string, ForeignFunction<unknown>>
  getFunctions<T extends Definitions>(definitions: T): ResolvedFunctions<T>

  getSymbol(name: string): bigint
  getSymbols(): Record<string, bigint>

  registerCallback(callback: () => void): bigint
  registerCallback<S extends FFISignature>(
    signature: S,
    callback: (...args: MapFFIArgs<SignatureArguments<S>>) => FFICallbackReturnValue<SignatureReturn<S>>,
  ): bigint

  unregisterCallback(pointer: bigint): void
  refCallback(pointer: bigint): void
  unrefCallback(pointer: bigint): void
}

export declare const suffix: string
export declare const types: Types

export declare function dlopen(path?: string | null): DlOpenResult
export declare function dlopen<T extends Definitions>(path: string | null | undefined, definitions: T): DlOpenResult<T>
export declare function dlclose(handle: DynamicLibrary): void
export declare function dlsym(handle: DynamicLibrary, symbol: string): bigint

export declare function getInt8(pointer: bigint, offset?: number | null): number
export declare function getUint8(pointer: bigint, offset?: number | null): number
export declare function getInt16(pointer: bigint, offset?: number | null): number
export declare function getUint16(pointer: bigint, offset?: number | null): number
export declare function getInt32(pointer: bigint, offset?: number | null): number
export declare function getUint32(pointer: bigint, offset?: number | null): number
export declare function getInt64(pointer: bigint, offset?: number | null): bigint
export declare function getUint64(pointer: bigint, offset?: number | null): bigint
export declare function getFloat32(pointer: bigint, offset?: number | null): number
export declare function getFloat64(pointer: bigint, offset?: number | null): number

export declare function setInt8(pointer: bigint, offset: number, value: number): void
export declare function setUint8(pointer: bigint, offset: number, value: number): void
export declare function setInt16(pointer: bigint, offset: number, value: number): void
export declare function setUint16(pointer: bigint, offset: number, value: number): void
export declare function setInt32(pointer: bigint, offset: number, value: number): void
export declare function setUint32(pointer: bigint, offset: number, value: number): void
export declare function setInt64(pointer: bigint, offset: number, value: bigint | number): void
export declare function setUint64(pointer: bigint, offset: number, value: bigint | number): void
export declare function setFloat32(pointer: bigint, offset: number, value: number): void
export declare function setFloat64(pointer: bigint, offset: number, value: number): void

export declare function toString(pointer: bigint): string | null
export declare function toBuffer(pointer: bigint, length: number, copy?: boolean | null): Buffer
export declare function toArrayBuffer(pointer: bigint, length: number, copy?: boolean | null): ArrayBuffer

export declare function exportString(string: string, pointer: bigint, length: number, encoding?: BufferEncoding): void
export declare function exportBuffer(buffer: Buffer, pointer: bigint, length: number): void
export declare function exportArrayBuffer(arrayBuffer: ArrayBuffer, pointer: bigint, length: number): void
export declare function exportArrayBufferView(arrayBufferView: ArrayBufferView, pointer: bigint, length: number): void

export declare function getRawPointer(source: Buffer | ArrayBuffer | ArrayBufferView): bigint
