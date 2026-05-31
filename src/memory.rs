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

fn checked_addr(pointer: BigInt, offset: Option<i64>, access_size: usize) -> Result<usize> {
  let raw = bigint_to_u64(&pointer, "The pointer must be a non-negative bigint")?;
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
  base
    .checked_add(offset)
    .and_then(|addr| addr.checked_add(access_size.saturating_sub(1)).map(|_| addr))
    .ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        "The accessed range exceeds the platform address range".to_owned(),
      )
    })
}

macro_rules! read_num {
  ($name:ident, $ty:ty, $size:expr) => {
    #[napi]
    pub fn $name(pointer: BigInt, offset: Option<i64>) -> Result<$ty> {
      let address = checked_addr(pointer, offset, $size)?;
      Ok(unsafe { ptr::read_unaligned(address as *const $ty) })
    }
  };
}

macro_rules! write_num {
  ($name:ident, $ty:ty, $size:expr) => {
    #[napi]
    pub fn $name(pointer: BigInt, offset: i64, value: $ty) -> Result<()> {
      let address = checked_addr(pointer, Some(offset), $size)?;
      unsafe { ptr::write_unaligned(address as *mut $ty, value) };
      Ok(())
    }
  };
}

read_num!(get_int8, i8, 1);
read_num!(get_uint8, u8, 1);
read_num!(get_int16, i16, 2);
read_num!(get_uint16, u16, 2);
read_num!(get_int32, i32, 4);
read_num!(get_uint32, u32, 4);
read_num!(get_int64, i64, 8);
read_num!(get_uint64, u64, 8);
read_num!(get_float32, f32, 4);
read_num!(get_float64, f64, 8);

write_num!(set_int8, i8, 1);
write_num!(set_uint8, u8, 1);
write_num!(set_int16, i16, 2);
write_num!(set_uint16, u16, 2);
write_num!(set_int32, i32, 4);
write_num!(set_uint32, u32, 4);
write_num!(set_int64, i64, 8);

#[napi]
pub fn set_uint64(pointer: BigInt, offset: i64, value: BigInt) -> Result<()> {
  let raw = bigint_to_u64(&value, "Value must be a uint64")?;
  let address = checked_addr(pointer, Some(offset), 8)?;
  unsafe { ptr::write_unaligned(address as *mut u64, raw) };
  Ok(())
}

#[napi]
pub fn set_float32(pointer: BigInt, offset: i64, value: f64) -> Result<()> {
  let address = checked_addr(pointer, Some(offset), 4)?;
  unsafe { ptr::write_unaligned(address as *mut f32, value as f32) };
  Ok(())
}

write_num!(set_float64, f64, 8);

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
pub fn to_buffer(env: &Env, pointer: BigInt, len: u32, _copy: Option<bool>) -> Result<Buffer> {
  let raw = bigint_to_u64(&pointer, "The first argument must be a non-negative bigint")?;
  if raw == 0 && len > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot create a buffer from a null pointer".to_owned(),
    ));
  }
  let slice = unsafe { std::slice::from_raw_parts(raw as usize as *const u8, len as usize) };
  BufferSlice::copy_from(env, slice)?.into_buffer(env)
}

#[napi]
pub fn to_array_buffer<'env>(env: &'env Env, pointer: BigInt, len: u32, _copy: Option<bool>) -> Result<ArrayBuffer<'env>> {
  let raw = bigint_to_u64(&pointer, "The first argument must be a non-negative bigint")?;
  if raw == 0 && len > 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot create an ArrayBuffer from a null pointer".to_owned(),
    ));
  }
  let slice = unsafe { std::slice::from_raw_parts(raw as usize as *const u8, len as usize) };
  ArrayBuffer::from_data(env, slice)
}

#[napi]
pub fn get_raw_pointer(value: Buffer) -> BigInt {
  BigInt::from(value.as_ref().as_ptr() as u64)
}
