use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::types::validate_pointer_span;

/// Helper: validate pointer bigint + optional offset and return raw pointer.
fn validate_ptr_offset(ptr: &BigInt, offset: Option<i64>) -> Result<*mut u8> {
  let (signed, addr, lossless) = ptr.get_u64();
  if signed || !lossless || addr > usize::MAX as u64 {
    return Err(Error::new(
      Status::InvalidArg,
      "The pointer must be a non-negative bigint".to_string(),
    ));
  }
  if addr == 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot dereference a null pointer".to_string(),
    ));
  }
  let base = addr as usize;
  let off = offset.unwrap_or(0) as usize;
  validate_pointer_span(base, off, 1)?;
  Ok((base + off) as *mut u8)
}

macro_rules! define_get_fn {
  ($name:ident, $ty:ty, $ret_ty:ty) => {
    #[napi]
    pub fn $name(ptr: BigInt, offset: Option<i64>) -> Result<$ret_ty> {
      let p = validate_ptr_offset(&ptr, offset)?;
      let val: $ty = unsafe { std::ptr::read(p as *const $ty) };
      Ok(val as $ret_ty)
    }
  };
}

macro_rules! define_get_bigint_fn {
  ($name:ident, $ty:ty) => {
    #[napi]
    pub fn $name(ptr: BigInt, offset: Option<i64>) -> Result<BigInt> {
      let p = validate_ptr_offset(&ptr, offset)?;
      let val: $ty = unsafe { std::ptr::read(p as *const $ty) };
      Ok(BigInt::from(val as i64))
    }
  };
}

macro_rules! define_set_int_fn {
  ($name:ident, $ty:ty, $value_ty:ty, $min:expr, $max:expr, $label:expr) => {
    #[napi]
    pub fn $name(ptr: BigInt, offset: i64, value: $value_ty) -> Result<()> {
      let p = validate_ptr_offset(&ptr, Some(offset))?;
      let v = value as i128;
      if !($min as i128..=$max as i128).contains(&v) {
        return Err(Error::new(
          Status::InvalidArg,
          format!("Value must be {}", $label),
        ));
      }
      unsafe { std::ptr::write(p as *mut $ty, value as $ty) };
      Ok(())
    }
  };
}

macro_rules! define_set_float_fn {
  ($name:ident, $ty:ty) => {
    #[napi]
    pub fn $name(ptr: BigInt, offset: i64, value: f64) -> Result<()> {
      let p = validate_ptr_offset(&ptr, Some(offset))?;
      let v = value as $ty;
      unsafe { std::ptr::write(p as *mut $ty, v) };
      Ok(())
    }
  };
}

// Get functions
define_get_fn!(get_int8, i8, i32);
define_get_fn!(get_uint8, u8, i32);
define_get_fn!(get_int16, i16, i32);
define_get_fn!(get_uint16, u16, i32);
define_get_fn!(get_int32, i32, i32);
define_get_fn!(get_uint32, u32, u32);
define_get_bigint_fn!(get_int64, i64);
define_get_bigint_fn!(get_uint64, u64);
define_get_fn!(get_float32, f32, f64);
define_get_fn!(get_float64, f64, f64);

// Set functions
define_set_int_fn!(set_int8, i8, i32, -128, 127, "an int8");
define_set_int_fn!(set_uint8, u8, i32, 0, 255, "a uint8");
define_set_int_fn!(set_int16, i16, i32, -32768, 32767, "an int16");
define_set_int_fn!(set_uint16, u16, i32, 0, 65535, "a uint16");
define_set_int_fn!(
  set_int32,
  i32,
  i32,
  i32::MIN,
  i32::MAX,
  "an int32"
);
define_set_int_fn!(set_uint32, u32, u32, 0u32, u32::MAX, "a uint32");

// Set 64-bit functions.
#[napi]
pub fn set_int64(ptr: BigInt, offset: i64, value: BigInt) -> Result<()> {
  let p = validate_ptr_offset(&ptr, Some(offset))?;
  let (v, lossless) = value.get_i64();
  if !lossless {
    return Err(Error::new(
      Status::InvalidArg,
      "Value must be an int64".to_string(),
    ));
  }
  unsafe { std::ptr::write(p as *mut i64, v) };
  Ok(())
}

#[napi]
pub fn set_uint64(ptr: BigInt, offset: i64, value: BigInt) -> Result<()> {
  let p = validate_ptr_offset(&ptr, Some(offset))?;
  let (signed, v, lossless) = value.get_u64();
  if signed || !lossless {
    return Err(Error::new(
      Status::InvalidArg,
      "Value must be a uint64".to_string(),
    ));
  }
  unsafe { std::ptr::write(p as *mut u64, v) };
  Ok(())
}

define_set_float_fn!(set_float32, f32);
define_set_float_fn!(set_float64, f64);
