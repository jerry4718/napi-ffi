use std::ffi::{c_void, CStr};

use napi::bindgen_prelude::*;
use napi::{noop_finalize, Env, Unknown};
use napi_derive::napi;

use crate::types::{get_validated_pointer, validate_pointer_span};

/// Read a NUL-terminated C string from a pointer.
/// Mirrors node:ffi's toString().
#[napi]
pub fn to_string(ptr: BigInt) -> Result<String> {
  let addr = get_validated_pointer(&ptr, "first argument")?;
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
pub fn to_buffer(env: &Env, ptr: BigInt, len: u32, writable: Option<bool>) -> Result<BufferSlice<'_>> {
  let addr = get_validated_pointer(&ptr, "first argument")?;
  let length = len as usize;

  if addr == 0 && length > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot create a buffer from a null pointer".to_string(),
    ));
  }

  validate_pointer_span(addr, 0, length)?;

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
pub fn to_array_buffer(
  env: &Env,
  ptr: BigInt,
  len: u32,
  copy: Option<bool>,
) -> Result<ArrayBuffer<'_>> {
  let addr = get_validated_pointer(&ptr, "first argument")?;
  let length = len as usize;

  if addr == 0 && length > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot create an ArrayBuffer from a null pointer".to_string(),
    ));
  }

  validate_pointer_span(addr, 0, length)?;

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
pub fn export_bytes(source: Unknown, ptr: BigInt, len: u32) -> Result<()> {
  let addr = get_validated_pointer(&ptr, "pointer")?;
  let length = len as usize;

  let (src_ptr, src_len) = get_buffer_like_pointer_and_len(&source).ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "The first argument must be a Buffer, ArrayBuffer, or ArrayBufferView".to_string(),
    )
  })?;

  validate_pointer_span(addr, 0, length)?;
  if length < src_len {
    return Err(Error::new(
      Status::InvalidArg,
      "The length must be >= source byte length".to_string(),
    ));
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
  let (ptr, _) = get_buffer_like_pointer_and_len(&source).ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "The first argument must be a Buffer, ArrayBuffer, or ArrayBufferView".to_string(),
    )
  })?;
  Ok(BigInt::from(ptr as u64))
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
