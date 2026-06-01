use std::ffi::{c_char, CStr};
use std::ptr;

use napi::bindgen_prelude::*;
use napi_derive::napi;

fn bigint_to_u64(value: &BigInt, message: &str) -> Result<u64> {
  let (signed, raw, lossless) = value.get_u64();
  if signed || !lossless {
    return Err(Error::new(Status::InvalidArg, message.to_owned()));
  }
  Ok(raw)
}

fn bigint_to_i64(value: &BigInt, message: &str) -> Result<i64> {
  let (raw, lossless) = value.get_i64();
  if !lossless {
    return Err(Error::new(Status::InvalidArg, message.to_owned()));
  }
  Ok(raw)
}

fn checked_addr(pointer: BigInt, offset: Option<i64>, access_size: usize) -> Result<usize> {
  let raw = bigint_to_u64(&pointer, "The pointer must be a non-negative bigint")?;
  if raw == 0 && access_size != 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot dereference a null pointer".to_owned(),
    ));
  }
  let offset = offset.unwrap_or(0);
  if offset < 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "The offset must be a number >= 0".to_owned(),
    ));
  }
  let base = usize::try_from(raw).map_err(|_| {
    Error::new(
      Status::InvalidArg,
      "The pointer exceeds the platform address range".to_owned(),
    )
  })?;
  let offset = usize::try_from(offset).map_err(|_| {
    Error::new(
      Status::InvalidArg,
      "The offset exceeds the platform address range".to_owned(),
    )
  })?;
  let base_plus_offset = base.checked_add(offset).ok_or_else(|| {
    Error::new(
      Status::InvalidArg,
      "The pointer and offset exceed the platform address range".to_owned(),
    )
  })?;
  base_plus_offset
    .checked_add(access_size.saturating_sub(1))
    .map(|_| base_plus_offset)
    .ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        "The accessed range exceeds the platform address range".to_owned(),
      )
    })
}

macro_rules! read_num {
  ($name:ident, $size:expr, $ty:ty) => {
    #[napi]
    pub fn $name(pointer: BigInt, offset: Option<i64>) -> Result<$ty> {
      let address = checked_addr(pointer, offset, $size)?;
      Ok(unsafe { ptr::read_unaligned(address as *const $ty) })
    }
  };
  ($name:ident, $size:expr, $iid:ident: $oty:ty, $rty:ty, $expr:expr) => {
    #[napi]
    pub fn $name(pointer: BigInt, offset: Option<i64>) -> Result<$oty> {
      let address = checked_addr(pointer, offset, $size)?;
      let $iid = unsafe { ptr::read_unaligned(address as *const $rty) };
      Ok($expr)
    }
  };
}

macro_rules! write_num {
  ($name:ident, $size:expr, $ty:ty) => {
    #[napi]
    pub fn $name(pointer: BigInt, offset: i64, value: $ty) -> Result<()> {
      let address = checked_addr(pointer, Some(offset), $size)?;
      unsafe { ptr::write_unaligned(address as *mut $ty, value) };
      Ok(())
    }
  };
  ($name:ident, $size:expr, $iid:ident: $ity:ty, $wty:ty, $expr:expr) => {
    #[napi]
    pub fn $name(pointer: BigInt, offset: i64, $iid: $ity) -> Result<()> {
      let address = checked_addr(pointer, Some(offset), $size)?;
      unsafe { ptr::write_unaligned(address as *mut $wty, $expr) };
      Ok(())
    }
  };
}

read_num!(get_int8, 1, i8);
read_num!(get_uint8, 1, u8);
read_num!(get_int16, 2, i16);
read_num!(get_uint16, 2, u16);
read_num!(get_int32, 4, i32);
read_num!(get_uint32, 4, u32);
read_num!(get_int64, 8, value: BigInt, i64, BigInt::from(value));
read_num!(get_uint64, 8, value: BigInt, u64, BigInt::from(value));
read_num!(get_float32, 4, f32);
read_num!(get_float64, 8, f64);

write_num!(set_int8, 1, i8);
write_num!(set_uint8, 1, u8);
write_num!(set_int16, 2, i16);
write_num!(set_uint16, 2, u16);
write_num!(set_int32, 4, i32);
write_num!(set_uint32, 4, u32);
write_num!(set_int64, 8, value: BigInt, i64, bigint_to_i64(&value, "Value must be an int64")?);
write_num!(set_uint64, 8, value: BigInt, u64, bigint_to_u64(&value, "Value must be a uint64")?);
write_num!(set_float32, 4, value: f64, f32, value as f32);
write_num!(set_float64, 8, f64);

#[napi]
pub fn to_string(pointer: BigInt) -> Result<Option<String>> {
  let raw = bigint_to_u64(&pointer, "The pointer must be a non-negative bigint")?;
  if raw == 0 {
    return Ok(None);
  }
  let value = unsafe { CStr::from_ptr(raw as usize as *const c_char) }
    .to_str()
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))?
    .to_owned();
  Ok(Some(value))
}

#[napi]
pub fn to_buffer(env: &Env, pointer: BigInt, len: u32, copy: Option<bool>) -> Result<Buffer> {
  let raw = bigint_to_u64(&pointer, "The first argument must be a non-negative bigint")?;
  if usize::BITS < 64 && raw > usize::MAX as u64 {
    return Err(Error::new(
      Status::InvalidArg,
      "The pointer exceeds the platform address range".to_owned(),
    ));
  }
  if raw == 0 && len > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot create a buffer from a null pointer".to_owned(),
    ));
  }
  let len = len as usize;
  (raw as usize)
    .checked_add(len.saturating_sub(1))
    .ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        "The pointer and length exceed the platform address range".to_owned(),
      )
    })?;
  let slice = if len == 0 {
    &[]
  } else {
    unsafe { std::slice::from_raw_parts(raw as usize as *const u8, len) }
  };
  if copy == Some(false) {
    unsafe {
      BufferSlice::from_external(env, raw as usize as *mut u8, len, (), |_, _| {})?.into_buffer(env)
    }
  } else {
    BufferSlice::copy_from(env, slice)?.into_buffer(env)
  }
}

#[napi]
pub fn to_array_buffer<'env>(
  env: &'env Env,
  pointer: BigInt,
  len: u32,
  copy: Option<bool>,
) -> Result<ArrayBuffer<'env>> {
  let raw = bigint_to_u64(&pointer, "The first argument must be a non-negative bigint")?;
  if usize::BITS < 64 && raw > usize::MAX as u64 {
    return Err(Error::new(
      Status::InvalidArg,
      "The pointer exceeds the platform address range".to_owned(),
    ));
  }
  if raw == 0 && len > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot create an ArrayBuffer from a null pointer".to_owned(),
    ));
  }
  let len = len as usize;
  (raw as usize)
    .checked_add(len.saturating_sub(1))
    .ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        "The pointer and length exceed the platform address range".to_owned(),
      )
    })?;
  let slice = if len == 0 {
    &[]
  } else {
    unsafe { std::slice::from_raw_parts(raw as usize as *const u8, len) }
  };
  if copy == Some(false) {
    unsafe { ArrayBuffer::from_external(env, raw as usize as *mut u8, len, (), |_, _| {}) }
  } else {
    ArrayBuffer::from_data(env, slice)
  }
}

#[napi]
pub fn get_raw_pointer(value: Unknown<'_>) -> Result<BigInt> {
  match value.get_type()? {
    ValueType::Object => {
      if let Ok(buffer) = unsafe { value.cast::<Buffer>() } {
        return Ok(BigInt::from(buffer.as_ref().as_ptr() as u64));
      }
      if let Ok(arraybuffer) = unsafe { value.cast::<ArrayBuffer>() } {
        return Ok(BigInt::from(arraybuffer.as_ref().as_ptr() as u64));
      }
      if let Ok(typed) = unsafe { value.cast::<TypedArray>() } {
        return Ok(BigInt::from(
          typed
            .arraybuffer
            .as_ref()
            .as_ptr()
            .wrapping_add(typed.byte_offset) as u64,
        ));
      }
      Err(Error::new(
        Status::InvalidArg,
        "Expected a Buffer, ArrayBuffer, or TypedArray".to_owned(),
      ))
    }
    _ => Err(Error::new(
      Status::InvalidArg,
      "Expected a Buffer, ArrayBuffer, or TypedArray".to_owned(),
    )),
  }
}
