use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::args::{
  ensure_value_present, expect_bigint_value, validated_float, validated_pointer_and_offset,
  validated_required_offset, validated_signed_int,
};

fn read_unaligned_at<T: Copy>(ptr: *const u8) -> T {
  // SAFETY: callers validate that ptr..ptr+size_of::<T>() is a readable platform address range.
  // Unaligned access is intentional because these memory helpers mirror node:ffi raw pointer reads.
  unsafe { std::ptr::read_unaligned(ptr.cast::<T>()) }
}

fn write_unaligned_at<T>(ptr: *mut u8, value: T) {
  // SAFETY: callers validate that ptr..ptr+size_of::<T>() is a writable platform address range.
  // Unaligned access is intentional because these memory helpers mirror node:ffi raw pointer writes.
  unsafe { std::ptr::write_unaligned(ptr.cast::<T>(), value) };
}

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
      let val: $ty = read_unaligned_at(p);
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
      let val: $ty = read_unaligned_at(p);
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
      write_unaligned_at(p, v as $ty);
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
      write_unaligned_at(p, v);
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
define_set_int_fn!(set_int32, i32, i32, i32::MIN, i32::MAX, "an int32");
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
  write_unaligned_at(p, v);
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
  write_unaligned_at(p, v);
  Ok(())
}

define_set_float_fn!(set_float32, f32);
define_set_float_fn!(set_float64, f64);
