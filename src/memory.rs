use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::args::{
  ensure_value_present, expect_bigint_value, validated_float, validated_pointer_and_offset, validated_required_offset,
  validated_signed_int,
};

macro_rules! define_get_fn {
  ($name:ident, $ty:ty, $ret_ty:ty) => {
    #[napi]
    pub fn $name(ptr: BigInt, offset: Option<i64>) -> Result<$ret_ty> {
      let p = validated_pointer_and_offset(
        &ptr,
        offset,
        std::mem::size_of::<$ty>(),
        "The accessed range exceeds the platform address range",
      )?;
      let val: $ty = unsafe { std::ptr::read(p as *const $ty) };
      Ok(val as $ret_ty)
    }
  };
}

macro_rules! define_get_bigint_fn {
  ($name:ident, $ty:ty) => {
    #[napi]
    pub fn $name(ptr: BigInt, offset: Option<i64>) -> Result<BigInt> {
      let p = validated_pointer_and_offset(
        &ptr,
        offset,
        std::mem::size_of::<$ty>(),
        "The accessed range exceeds the platform address range",
      )?;
      let val: $ty = unsafe { std::ptr::read(p as *const $ty) };
      Ok(BigInt::from(val as i64))
    }
  };
}

macro_rules! define_set_int_fn {
  ($name:ident, $ty:ty, $value_ty:ty, $min:expr, $max:expr, $label:expr) => {
    #[napi]
    pub fn $name(ptr: BigInt, offset: Unknown, value: Unknown) -> Result<()> {
      let offset = validated_required_offset(&offset)?;
      ensure_value_present(&value)?;
      let p = validated_pointer_and_offset(
        &ptr,
        Some(offset),
        std::mem::size_of::<$ty>(),
        "The accessed range exceeds the platform address range",
      )?;
      let v = validated_signed_int(&value, $min as i128, $max as i128, $label)?;
      unsafe { std::ptr::write(p as *mut $ty, v as $ty) };
      Ok(())
    }
  };
}

macro_rules! define_set_float_fn {
  ($name:ident, $ty:ty) => {
    #[napi]
    pub fn $name(ptr: BigInt, offset: Unknown, value: Unknown) -> Result<()> {
      let offset = validated_required_offset(&offset)?;
      ensure_value_present(&value)?;
      let p = validated_pointer_and_offset(
        &ptr,
        Some(offset),
        std::mem::size_of::<$ty>(),
        "The accessed range exceeds the platform address range",
      )?;
      let v = validated_float(&value, stringify!($ty))? as $ty;
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
pub fn set_int64(ptr: BigInt, offset: Unknown, value: Unknown) -> Result<()> {
  let offset = validated_required_offset(&offset)?;
  let value = expect_bigint_value(&value, "an int64")?;
  let p = validated_pointer_and_offset(
    &ptr,
    Some(offset),
    std::mem::size_of::<i64>(),
    "The accessed range exceeds the platform address range",
  )?;
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
pub fn set_uint64(ptr: BigInt, offset: Unknown, value: Unknown) -> Result<()> {
  let offset = validated_required_offset(&offset)?;
  let value = expect_bigint_value(&value, "a uint64")?;
  let p = validated_pointer_and_offset(
    &ptr,
    Some(offset),
    std::mem::size_of::<u64>(),
    "The accessed range exceeds the platform address range",
  )?;
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
