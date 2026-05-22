use std::ffi::{c_void, CStr};

use napi::bindgen_prelude::*;
use napi::{noop_finalize, Env, Unknown};
use napi_derive::napi;

use crate::args::{
  expect_string, validate_pointer_span_with_message, validated_pointer, validated_pointer_from_unknown, validated_size,
  validated_size_with_code,
};
use crate::errors::{throw_coded_error, JsErrorKind};
use crate::types::validate_pointer_span;

/// Read a NUL-terminated C string from a pointer.
/// Mirrors node:ffi's toString().
#[napi]
pub fn to_string(ptr: BigInt) -> Result<String> {
  let addr = validated_pointer(&ptr, "first argument")?;
  if addr == 0 {
    return Ok(String::new());
  }
  let cstr = unsafe { CStr::from_ptr(addr as *const i8) };
  cstr.to_str().map(str::to_owned).map_err(|_| {
    Error::new(
      Status::InvalidArg,
      "Invalid UTF-8 string at pointer".to_string(),
    )
  })
}

/// Create a Buffer from a pointer and length. `writable === false` returns a borrowed raw-memory view like node:ffi.
#[napi]
pub fn to_buffer<'env>(
  env: &'env Env,
  ptr: Unknown<'env>,
  len: Unknown<'env>,
  writable: Option<bool>,
) -> Result<BufferSlice<'env>> {
  let addr = validated_pointer_from_unknown(&ptr, "first argument")?;
  let length = validated_size(&len, "length")?;
  validate_buffer_length(env, length)?;

  if addr == 0 && length > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot create a buffer from a null pointer".to_string(),
    ));
  }

  validate_buffer_pointer_span(addr, length)?;

  if !writable.unwrap_or(true) && length > 0 {
    return unsafe { BufferSlice::from_external(env, addr as *mut u8, length, (), noop_finalize) };
  }

  let mut out = vec![0u8; length];
  if length > 0 {
    unsafe {
      std::ptr::copy_nonoverlapping(addr as *const u8, out.as_mut_ptr(), length);
    }
  }
  BufferSlice::from_data(env, out)
}

/// Create an ArrayBuffer from a pointer and length. `copy === false` returns a borrowed raw-memory view like node:ffi.
#[napi]
pub fn to_array_buffer<'env>(
  env: &'env Env,
  ptr: Unknown<'env>,
  len: Unknown<'env>,
  copy: Option<bool>,
) -> Result<ArrayBuffer<'env>> {
  let addr = validated_pointer_from_unknown(&ptr, "first argument")?;
  let length = validated_size(&len, "length")?;
  validate_buffer_length(env, length)?;

  if addr == 0 && length > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot create an ArrayBuffer from a null pointer".to_string(),
    ));
  }

  validate_buffer_pointer_span(addr, length)?;

  if !copy.unwrap_or(true) && length > 0 {
    return unsafe { ArrayBuffer::from_external(env, addr as *mut u8, length, (), noop_finalize) };
  }

  let mut out = vec![0u8; length];
  if length > 0 {
    unsafe {
      std::ptr::copy_nonoverlapping(addr as *const u8, out.as_mut_ptr(), length);
    }
  }
  ArrayBuffer::from_data(env, out)
}

/// Export bytes from a Buffer/ArrayBuffer/TypedArray to a target pointer.
/// Mirrors node:ffi's exportBytes().
#[napi]
pub fn export_bytes(env: &Env, source: Unknown, ptr: BigInt, len: Unknown) -> Result<()> {
  export_bytes_impl(env, source, ptr, len, SourceKind::Any)
}

/// Export a Buffer to a target pointer with Node's public API validation moved native-side.
#[napi]
pub fn export_buffer(env: &Env, source: Unknown, ptr: BigInt, len: Unknown) -> Result<()> {
  export_bytes_impl(env, source, ptr, len, SourceKind::Buffer)
}

/// Export an ArrayBuffer to a target pointer with Node's public API validation moved native-side.
#[napi]
pub fn export_array_buffer(env: &Env, source: Unknown, ptr: BigInt, len: Unknown) -> Result<()> {
  export_bytes_impl(env, source, ptr, len, SourceKind::ArrayBuffer)
}

/// Export an ArrayBufferView/TypedArray/DataView to a target pointer with Node's public API validation moved native-side.
#[napi]
pub fn export_array_buffer_view(env: &Env, source: Unknown, ptr: BigInt, len: Unknown) -> Result<()> {
  export_bytes_impl(env, source, ptr, len, SourceKind::ArrayBufferView)
}

/// Encode and export a NUL-terminated string to a target pointer.
#[napi]
pub fn export_string(env: &Env, value: Unknown, ptr: BigInt, len: Unknown, encoding: Option<Unknown>) -> Result<()> {
  let value = expect_string(env, &value, "source")?;
  let length = validated_size_with_code(env, &len, "length")?;
  let encoding = match encoding {
    Some(value) => expect_string(env, &value, "encoding")?,
    None => "utf8".to_string(),
  };
  let terminator_size = match encoding.to_ascii_lowercase().as_str() {
    "ucs2" | "ucs-2" | "utf16le" | "utf-16le" => 2usize,
    _ => 1usize,
  };

  let bytes = encode_string(env, &value, &encoding)?;
  let required_len = bytes.len().checked_add(terminator_size).ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "The encoded string length exceeds the platform address range".to_string(),
    )
  })?;
  if length < required_len {
    return throw_coded_error(
      env,
      JsErrorKind::RangeError,
      "ERR_OUT_OF_RANGE",
      format!("len must be >= {required_len}"),
    );
  }

  let addr = validated_pointer(&ptr, "pointer")?;
  validate_pointer_span(addr, 0, length)?;
  if addr == 0 && required_len > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot copy to a null pointer".to_string(),
    ));
  }

  unsafe {
    if !bytes.is_empty() {
      std::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len());
    }
    std::ptr::write_bytes((addr + bytes.len()) as *mut u8, 0, terminator_size);
  }
  Ok(())
}

#[derive(Clone, Copy)]
enum SourceKind {
  Any,
  Buffer,
  ArrayBuffer,
  ArrayBufferView,
}

fn export_bytes_impl(env: &Env, source: Unknown, ptr: BigInt, len: Unknown, kind: SourceKind) -> Result<()> {
  let addr = validated_pointer(&ptr, "pointer")?;
  let length = validated_size_with_code(env, &len, "length")?;

  let (src_ptr, src_len) = get_typed_source_pointer_and_len(env, &source, kind)?;

  validate_pointer_span(addr, 0, length)?;
  if length < src_len {
    return throw_coded_error(
      env,
      JsErrorKind::RangeError,
      "ERR_OUT_OF_RANGE",
      format!("len must be >= {src_len}"),
    );
  }
  if addr == 0 && src_len > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot copy to a null pointer".to_string(),
    ));
  }
  if src_len > 0 {
    unsafe {
      std::ptr::copy_nonoverlapping(src_ptr, addr as *mut u8, src_len);
    }
  }
  Ok(())
}

/// Get the raw pointer address of a Buffer, ArrayBuffer, or TypedArray.
/// Mirrors node:ffi's getRawPointer().
#[napi]
pub fn get_raw_pointer(source: Unknown) -> Result<BigInt> {
  let Some((ptr, _)) = get_buffer_like_pointer_and_len(&source) else {
    let env = env_from_unknown(&source);
    return throw_coded_error(
      &env,
      JsErrorKind::TypeError,
      "ERR_INVALID_ARG_TYPE",
      "The first argument must be a Buffer, ArrayBuffer, or ArrayBufferView",
    );
  };
  Ok(BigInt::from(ptr as u64))
}

fn get_typed_source_pointer_and_len(env: &Env, value: &Unknown, kind: SourceKind) -> Result<(*const u8, usize)> {
  let result = match kind {
    SourceKind::Any => get_buffer_like_pointer_and_len(value),
    SourceKind::Buffer => get_buffer_pointer_and_len(value),
    SourceKind::ArrayBuffer => get_arraybuffer_pointer_and_len(value),
    SourceKind::ArrayBufferView => get_typedarray_pointer_and_len(value),
  };

  if let Some(result) = result {
    return Ok(result);
  }

  let message = match kind {
    SourceKind::Any => "The first argument must be a Buffer, ArrayBuffer, or ArrayBufferView",
    SourceKind::Buffer => "buffer must be a Buffer",
    SourceKind::ArrayBuffer => "arrayBuffer must be an ArrayBuffer",
    SourceKind::ArrayBufferView => "arrayBufferView must be an ArrayBufferView",
  };
  throw_coded_error(env, JsErrorKind::TypeError, "ERR_INVALID_ARG_TYPE", message)
}

fn get_buffer_like_pointer_and_len(value: &Unknown) -> Option<(*const u8, usize)> {
  if let Some(result) = get_buffer_pointer_and_len(value) {
    return Some(result);
  }
  if let Some(result) = get_arraybuffer_pointer_and_len(value) {
    return Some(result);
  }
  get_typedarray_pointer_and_len(value)
}

fn validate_buffer_pointer_span(addr: usize, length: usize) -> Result<()> {
  validate_pointer_span_with_message(
    addr,
    0,
    length,
    "The pointer and length exceed the platform address range",
  )
}

fn validate_buffer_length(env: &Env, length: usize) -> Result<()> {
  const MAX_BUFFER_LENGTH: usize = 0x1f_ffff_ffff_ffff;
  if length > MAX_BUFFER_LENGTH {
    return throw_coded_error(
      env,
      JsErrorKind::RangeError,
      "ERR_BUFFER_TOO_LARGE",
      "Cannot create a Buffer larger than buffer.constants.MAX_LENGTH",
    );
  }
  Ok(())
}

fn env_from_unknown(value: &Unknown) -> Env {
  Env::from_raw(value.value().env)
}

fn encode_string(env: &Env, value: &str, encoding: &str) -> Result<Vec<u8>> {
  let mut buffer = std::ptr::null_mut();
  let mut global = std::ptr::null_mut();
  let mut from = std::ptr::null_mut();
  let mut result = std::ptr::null_mut();
  let mut data = std::ptr::null_mut::<c_void>();
  let mut len = 0usize;
  let buffer_name = std::ffi::CString::new("Buffer").expect("static string has no nul bytes");
  let from_name = std::ffi::CString::new("from").expect("static string has no nul bytes");

  check_status!(unsafe { napi::sys::napi_get_global(env.raw(), &mut global) })?;
  check_status!(unsafe {
    napi::sys::napi_get_named_property(env.raw(), global, buffer_name.as_ptr(), &mut buffer)
  })?;
  check_status!(unsafe { napi::sys::napi_get_named_property(env.raw(), buffer, from_name.as_ptr(), &mut from) })?;

  let js_value = env.create_string(value)?;
  let js_encoding = env.create_string(encoding)?;
  let argv = [js_value.raw(), js_encoding.raw()];
  check_status!(unsafe {
    napi::sys::napi_call_function(env.raw(), buffer, from, argv.len(), argv.as_ptr(), &mut result)
  })?;
  check_status!(unsafe { napi::sys::napi_get_buffer_info(env.raw(), result, &mut data, &mut len) })?;
  Ok(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) }.to_vec())
}

fn get_buffer_pointer_and_len(value: &Unknown) -> Option<(*const u8, usize)> {
  let mut data = std::ptr::null_mut::<c_void>();
  let mut len = 0usize;
  napi::check_status!(unsafe {
    napi::sys::napi_get_buffer_info(value.value().env, value.raw(), &mut data, &mut len)
  })
  .ok()?;
  Some((data.cast::<u8>(), len))
}

fn get_arraybuffer_pointer_and_len(value: &Unknown) -> Option<(*const u8, usize)> {
  let mut data = std::ptr::null_mut::<c_void>();
  let mut len = 0usize;
  napi::check_status!(unsafe {
    napi::sys::napi_get_arraybuffer_info(value.value().env, value.raw(), &mut data, &mut len)
  })
  .ok()?;
  Some((data.cast::<u8>(), len))
}

fn get_typedarray_pointer_and_len(value: &Unknown) -> Option<(*const u8, usize)> {
  let mut typedarray_type = 0;
  let mut len = 0usize;
  let mut data = std::ptr::null_mut::<c_void>();
  let mut arraybuffer = std::ptr::null_mut();
  let mut byte_offset = 0usize;
  napi::check_status!(unsafe {
    napi::sys::napi_get_typedarray_info(
      value.value().env,
      value.raw(),
      &mut typedarray_type,
      &mut len,
      &mut data,
      &mut arraybuffer,
      &mut byte_offset,
    )
  })
  .ok()?;
  Some((data.cast::<u8>(), len))
}
