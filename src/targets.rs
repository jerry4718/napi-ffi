use std::alloc::Layout;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

use libffi::middle::Arg;
use napi::bindgen_prelude::i64n;
use napi::bindgen_prelude::*;
use napi::Env;

use crate::value_helpers::raw_bytes_pointer;

pub struct TypeOps {
  pub function_arg_layout: fn() -> Layout,
  pub formalize_function_arg: for<'env> unsafe fn(
    &'env Env,
    Unknown<'env>,
    usize,
    *mut u8,
    &mut FormalizedStorageScope,
  ) -> Result<()>,
  pub function_arg_as_ffi_arg: unsafe fn(*const u8) -> Arg<'static>,
  pub function_return_layout: fn() -> Layout,
  pub formalize_function_return: for<'env> unsafe fn(&'env Env, *const u8) -> Result<Unknown<'env>>,
  pub formalize_callback_arg:
    for<'env> unsafe fn(&'env Env, *const c_void, usize) -> Result<Unknown<'env>>,
  pub callback_return_layout: fn() -> Layout,
  pub callback_return_copy_size: fn() -> usize,
  pub formalize_callback_return: for<'env> unsafe fn(
    &'env Env,
    Unknown<'env>,
    *mut u8,
    &mut FormalizedStorageScope,
  ) -> Result<()>,
  pub callback_return_ptr: unsafe fn(*const u8) -> *const c_void,
}

pub struct FormalizedStorageScope {
  c_strings: Vec<CString>,
}

impl FormalizedStorageScope {
  pub fn new() -> Self {
    Self {
      c_strings: Vec::new(),
    }
  }

  fn keep_c_string(&mut self, c_string: CString) -> *const c_char {
    let pointer = c_string.as_ptr();
    self.c_strings.push(c_string);
    pointer
  }
}

#[inline]
fn invalid_arg_value(message: impl Into<String>) -> Error {
  Error::new(Status::InvalidArg, message.into())
}

#[inline]
fn invalid_callback_return() -> Error {
  Error::new(
    Status::InvalidArg,
    "Callback returned invalid value for declared FFI type".to_owned(),
  )
}

#[inline]
fn bigint_to_u64(value: &BigInt) -> Option<u64> {
  let (signed, raw, lossless) = value.get_u64();
  if signed || !lossless {
    return None;
  }
  Some(raw)
}

#[inline]
fn bigint_to_i64(value: &BigInt) -> Option<i64> {
  let (raw, lossless) = value.get_i64();
  if !lossless {
    return None;
  }
  Some(raw)
}

#[inline]
fn cast_f64(value: Unknown<'_>) -> Result<f64> {
  f64::from_unknown(value)
}

#[inline]
fn cast_bigint(value: Unknown<'_>) -> Result<BigInt> {
  BigInt::from_unknown(value)
}

#[inline]
fn cast_string(value: Unknown<'_>) -> Result<String> {
  String::from_unknown(value)
}

#[inline]
fn strict_signed_integer<T>(number: f64, min: T, max: T) -> Option<i64>
where
  i64: From<T>,
{
  if !number.is_finite()
    || number.floor() != number
    || number < i64::from(min) as f64
    || number > i64::from(max) as f64
  {
    return None;
  }
  Some(number as i64)
}

#[inline]
fn strict_unsigned_integer<T>(number: f64, max: T) -> Option<u64>
where
  u64: From<T>,
{
  if !number.is_finite()
    || number.floor() != number
    || number < 0.0
    || number > u64::from(max) as f64
  {
    return None;
  }
  Some(number as u64)
}

#[inline]
fn number_to_signed_integer<T>(
  value: Unknown<'_>,
  index: usize,
  min: T,
  max: T,
  type_name: &'static str,
) -> Result<T>
where
  T: TryFrom<i64>,
  i64: From<T>,
{
  let number = cast_f64(value)?;
  strict_signed_integer(number, min, max)
    .and_then(|value| T::try_from(value).ok())
    .ok_or_else(|| invalid_arg_value(format!("Argument {index} must be an {type_name}")))
}

#[inline]
fn number_to_unsigned_integer<T>(
  value: Unknown<'_>,
  index: usize,
  max: T,
  type_name: &'static str,
) -> Result<T>
where
  T: TryFrom<u64>,
  u64: From<T>,
{
  let number = cast_f64(value)?;
  strict_unsigned_integer(number, max)
    .and_then(|value| T::try_from(value).ok())
    .ok_or_else(|| invalid_arg_value(format!("Argument {index} must be a {type_name}")))
}

#[inline]
fn callback_to_signed_integer<T>(value: Unknown<'_>, min: T, max: T) -> Result<T>
where
  T: TryFrom<i64>,
  i64: From<T>,
{
  let number = cast_f64(value).map_err(|_| invalid_callback_return())?;
  strict_signed_integer(number, min, max)
    .and_then(|value| T::try_from(value).ok())
    .ok_or_else(invalid_callback_return)
}

#[inline]
fn callback_to_unsigned_integer<T>(value: Unknown<'_>, max: T) -> Result<T>
where
  T: TryFrom<u64>,
  u64: From<T>,
{
  let number = cast_f64(value).map_err(|_| invalid_callback_return())?;
  strict_unsigned_integer(number, max)
    .and_then(|value| T::try_from(value).ok())
    .ok_or_else(invalid_callback_return)
}

#[inline]
fn raw_pointer_from_unknown(value: Unknown<'_>, index: usize) -> Result<*mut c_void> {
  match value.get_type()? {
    ValueType::Null | ValueType::Undefined => Ok(ptr::null_mut()),
    ValueType::BigInt => {
      let bigint = cast_bigint(value)?;
      let raw = bigint_to_u64(&bigint).ok_or_else(|| {
        invalid_arg_value(format!(
          "Argument {index} must be a non-negative pointer bigint"
        ))
      })?;
      let ptr_value = usize::try_from(raw)
        .map_err(|_| invalid_arg_value("Argument exceeds the platform pointer range"))?;
      Ok(ptr_value as *mut c_void)
    }
    ValueType::Object => raw_bytes_pointer(value).ok_or_else(|| {
      invalid_arg_value("Argument must be a buffer, an ArrayBuffer, a string, or a bigint")
    }),
    _ => Err(invalid_arg_value(
      "Argument must be a buffer, an ArrayBuffer, a string, or a bigint",
    )),
  }
}

enum PointerArgumentCategory {
  Regular(*mut c_void),
  String(CString),
}

#[inline]
fn pointer_argument_from_unknown(
  value: Unknown<'_>,
  index: usize,
) -> Result<PointerArgumentCategory> {
  match value.get_type()? {
    ValueType::String => {
      let string = cast_string(value)?;
      let c_string = CString::new(string)
        .map_err(|_| invalid_arg_value(format!("Argument {index} must not contain null bytes")))?;
      Ok(PointerArgumentCategory::String(c_string))
    }
    _ => Ok(PointerArgumentCategory::Regular(raw_pointer_from_unknown(
      value, index,
    )?)),
  }
}

macro_rules! read_scalar {
  ($ptr:expr, $ty:ty) => {
    unsafe { ptr::read_unaligned($ptr.cast::<$ty>()) }
  };
}

fn scalar_layout<T>() -> Layout {
  Layout::new::<T>()
}

fn scalar_size<T>() -> usize {
  std::mem::size_of::<T>()
}

unsafe fn scalar_arg_as_ffi_arg<T: 'static>(storage: *const u8) -> Arg<'static> {
  let value: &'static T = unsafe { &*storage.cast::<T>() };
  Arg::new(value)
}

unsafe fn scalar_callback_return_ptr(storage: *const u8) -> *const c_void {
  storage.cast()
}

macro_rules! numeric_type_ops {
  (
    $ops_name:ident,
    $to_ffi_call_arg:ident,
    $from_ffi_call_ret:ident,
    $from_ffi_back_arg:ident,
    $to_ffi_back_ret:ident,
    $rust_ty:ty,
    $env_name:ident,
    $val_name:ident,
    $idx_name:ident,
    $arg_check:expr,
    $callback_check:expr,
    $to_js:expr
    $(,)?
  ) => {
    unsafe fn $to_ffi_call_arg<'env>(
      _env: &'env Env,
      $val_name: Unknown<'env>,
      $idx_name: usize,
      storage: *mut u8,
      _scope: &mut FormalizedStorageScope,
    ) -> Result<()> {
      let parsed: $rust_ty = $arg_check;
      unsafe { ptr::write(storage.cast::<$rust_ty>(), parsed) };
      Ok(())
    }

    unsafe fn $from_ffi_call_ret<'env>(
      $env_name: &'env Env,
      storage: *const u8,
    ) -> Result<Unknown<'env>> {
      let $val_name: $rust_ty = read_scalar!(storage, $rust_ty);
      let result = $to_js;
      Ok(result)
    }

    unsafe fn $from_ffi_back_arg<'env>(
      $env_name: &'env Env,
      arg_ptr: *const c_void,
      _index: usize,
    ) -> Result<Unknown<'env>> {
      let $val_name: $rust_ty = read_scalar!(arg_ptr, $rust_ty);
      let result = $to_js;
      Ok(result)
    }

    unsafe fn $to_ffi_back_ret<'env>(
      _env: &'env Env,
      $val_name: Unknown<'env>,
      storage: *mut u8,
      _scope: &mut FormalizedStorageScope,
    ) -> Result<()> {
      let parsed: $rust_ty = $callback_check;
      unsafe { ptr::write_unaligned(storage.cast::<$rust_ty>(), parsed) };
      Ok(())
    }

    pub const $ops_name: TypeOps = TypeOps {
      function_arg_layout: scalar_layout::<$rust_ty>,
      formalize_function_arg: $to_ffi_call_arg,
      function_arg_as_ffi_arg: scalar_arg_as_ffi_arg::<$rust_ty>,
      function_return_layout: scalar_layout::<$rust_ty>,
      formalize_function_return: $from_ffi_call_ret,
      formalize_callback_arg: $from_ffi_back_arg,
      callback_return_layout: scalar_layout::<$rust_ty>,
      callback_return_copy_size: scalar_size::<$rust_ty>,
      formalize_callback_return: $to_ffi_back_ret,
      callback_return_ptr: scalar_callback_return_ptr,
    };
  };
}

numeric_type_ops!(
  I8_OPS,
  i8_to_ffi_call_arg,
  i8_from_ffi_call_ret,
  i8_from_ffi_back_arg,
  i8_to_ffi_back_ret,
  i8,
  env,
  value,
  index,
  { number_to_signed_integer(value, index, i8::MIN, i8::MAX, "int8")? },
  { callback_to_signed_integer(value, i8::MIN, i8::MAX)? },
  { i8::into_unknown(value, env)? },
);

numeric_type_ops!(
  U8_OPS,
  u8_to_ffi_call_arg,
  u8_from_ffi_call_ret,
  u8_from_ffi_back_arg,
  u8_to_ffi_back_ret,
  u8,
  env,
  value,
  index,
  { number_to_unsigned_integer(value, index, u8::MAX, "uint8")? },
  { callback_to_unsigned_integer(value, u8::MAX)? },
  { u8::into_unknown(value, env)? },
);

numeric_type_ops!(
  I16_OPS,
  i16_to_ffi_call_arg,
  i16_from_ffi_call_ret,
  i16_from_ffi_back_arg,
  i16_to_ffi_back_ret,
  i16,
  env,
  value,
  index,
  { number_to_signed_integer(value, index, i16::MIN, i16::MAX, "int16")? },
  { callback_to_signed_integer(value, i16::MIN, i16::MAX)? },
  { i16::into_unknown(value, env)? },
);

numeric_type_ops!(
  U16_OPS,
  u16_to_ffi_call_arg,
  u16_from_ffi_call_ret,
  u16_from_ffi_back_arg,
  u16_to_ffi_back_ret,
  u16,
  env,
  value,
  index,
  { number_to_unsigned_integer(value, index, u16::MAX, "uint16")? },
  { callback_to_unsigned_integer(value, u16::MAX)? },
  { u16::into_unknown(value, env)? },
);

numeric_type_ops!(
  I32_OPS,
  i32_to_ffi_call_arg,
  i32_from_ffi_call_ret,
  i32_from_ffi_back_arg,
  i32_to_ffi_back_ret,
  i32,
  env,
  value,
  index,
  { number_to_signed_integer(value, index, i32::MIN, i32::MAX, "int32")? },
  { callback_to_signed_integer(value, i32::MIN, i32::MAX)? },
  { i32::into_unknown(value, env)? },
);

numeric_type_ops!(
  U32_OPS,
  u32_to_ffi_call_arg,
  u32_from_ffi_call_ret,
  u32_from_ffi_back_arg,
  u32_to_ffi_back_ret,
  u32,
  env,
  value,
  index,
  { number_to_unsigned_integer(value, index, u32::MAX, "uint32")? },
  { callback_to_unsigned_integer(value, u32::MAX)? },
  { u32::into_unknown(value, env)? },
);

numeric_type_ops!(
  I64_OPS,
  i64_to_ffi_call_arg,
  i64_from_ffi_call_ret,
  i64_from_ffi_back_arg,
  i64_to_ffi_back_ret,
  i64,
  env,
  value,
  index,
  {
    let bigint = cast_bigint(value)?;
    bigint_to_i64(&bigint)
      .ok_or_else(|| invalid_arg_value(format!("Argument {index} must be an int64")))?
  },
  {
    let bigint = cast_bigint(value).map_err(|_| invalid_callback_return())?;
    bigint_to_i64(&bigint).ok_or_else(invalid_callback_return)?
  },
  { i64n(value).into_unknown(env)? },
);

numeric_type_ops!(
  U64_OPS,
  u64_to_ffi_call_arg,
  u64_from_ffi_call_ret,
  u64_from_ffi_back_arg,
  u64_to_ffi_back_ret,
  u64,
  env,
  value,
  index,
  {
    let bigint = cast_bigint(value)?;
    bigint_to_u64(&bigint)
      .ok_or_else(|| invalid_arg_value(format!("Argument {index} must be a uint64")))?
  },
  {
    let bigint = cast_bigint(value).map_err(|_| invalid_callback_return())?;
    bigint_to_u64(&bigint).ok_or_else(invalid_callback_return)?
  },
  { u64::into_unknown(value, env)? },
);

numeric_type_ops!(
  F32_OPS,
  f32_to_ffi_call_arg,
  f32_from_ffi_call_ret,
  f32_from_ffi_back_arg,
  f32_to_ffi_back_ret,
  f32,
  env,
  value,
  index,
  {
    cast_f64(value).map_err(|_| invalid_arg_value(format!("Argument {index} must be a float")))?
      as f32
  },
  { cast_f64(value).map_err(|_| invalid_callback_return())? as f32 },
  { f32::into_unknown(value, env)? },
);

numeric_type_ops!(
  F64_OPS,
  f64_to_ffi_call_arg,
  f64_from_ffi_call_ret,
  f64_from_ffi_back_arg,
  f64_to_ffi_back_ret,
  f64,
  env,
  value,
  index,
  { cast_f64(value).map_err(|_| invalid_arg_value(format!("Argument {index} must be a double")))? },
  { cast_f64(value).map_err(|_| invalid_callback_return())? },
  { f64::into_unknown(value, env)? },
);

#[repr(C)]
struct PointerArgStorageV2 {
  pointer: *mut c_void,
}

fn pointer_arg_layout() -> Layout {
  Layout::new::<PointerArgStorageV2>()
}

unsafe fn pointer_to_ffi_call_arg<'env>(
  _env: &'env Env,
  value: Unknown<'env>,
  index: usize,
  storage: *mut u8,
  scope: &mut FormalizedStorageScope,
) -> Result<()> {
  let storage = storage.cast::<PointerArgStorageV2>();
  let pointer = match pointer_argument_from_unknown(value, index)? {
    PointerArgumentCategory::String(c_string) => scope.keep_c_string(c_string) as *mut c_void,
    PointerArgumentCategory::Regular(pointer) => pointer,
  };
  unsafe { ptr::write(storage, PointerArgStorageV2 { pointer }) };
  Ok(())
}

unsafe fn pointer_arg_as_ffi_arg(storage: *const u8) -> Arg<'static> {
  let storage: &'static PointerArgStorageV2 = unsafe { &*storage.cast::<PointerArgStorageV2>() };
  Arg::new(&storage.pointer)
}

unsafe fn pointer_from_ffi_call_ret<'env>(
  env: &'env Env,
  storage: *const u8,
) -> Result<Unknown<'env>> {
  let value = read_scalar!(storage, *mut c_void);
  BigInt::from(value as usize as u64).into_unknown(env)
}

unsafe fn pointer_from_ffi_back_arg<'env>(
  env: &'env Env,
  arg_ptr: *const c_void,
  _index: usize,
) -> Result<Unknown<'env>> {
  BigInt::from(read_scalar!(arg_ptr, usize) as u64).into_unknown(env)
}

unsafe fn pointer_to_ffi_back_ret<'env>(
  _env: &'env Env,
  value: Unknown<'env>,
  storage: *mut u8,
  _scope: &mut FormalizedStorageScope,
) -> Result<()> {
  let pointer = match value.get_type() {
    Ok(ValueType::Null | ValueType::Undefined) => ptr::null_mut(),
    _ => raw_pointer_from_unknown(value, 0).map_err(|_| invalid_callback_return())?,
  };
  unsafe { ptr::write(storage.cast::<*mut c_void>(), pointer) };
  Ok(())
}

pub const POINTER_OPS: TypeOps = TypeOps {
  function_arg_layout: pointer_arg_layout,
  formalize_function_arg: pointer_to_ffi_call_arg,
  function_arg_as_ffi_arg: pointer_arg_as_ffi_arg,
  function_return_layout: scalar_layout::<*mut c_void>,
  formalize_function_return: pointer_from_ffi_call_ret,
  formalize_callback_arg: pointer_from_ffi_back_arg,
  callback_return_layout: scalar_layout::<*mut c_void>,
  callback_return_copy_size: scalar_size::<*mut c_void>,
  formalize_callback_return: pointer_to_ffi_back_ret,
  callback_return_ptr: scalar_callback_return_ptr,
};

#[repr(C)]
struct CStringArgStorageV2 {
  pointer: *const c_char,
}

fn cstring_arg_layout() -> Layout {
  Layout::new::<CStringArgStorageV2>()
}

unsafe fn cstring_to_ffi_call_arg<'env>(
  _env: &'env Env,
  value: Unknown<'env>,
  index: usize,
  storage: *mut u8,
  scope: &mut FormalizedStorageScope,
) -> Result<()> {
  let storage = storage.cast::<CStringArgStorageV2>();
  let pointer = match value.get_type()? {
    ValueType::String => {
      let string = cast_string(value)?;
      let c_string = CString::new(string)
        .map_err(|_| invalid_arg_value(format!("Argument {index} must not contain null bytes")))?;
      scope.keep_c_string(c_string)
    }
    _ => raw_pointer_from_unknown(value, index)? as *const c_char,
  };
  unsafe { ptr::write(storage, CStringArgStorageV2 { pointer }) };
  Ok(())
}

unsafe fn cstring_arg_as_ffi_arg(storage: *const u8) -> Arg<'static> {
  let storage: &'static CStringArgStorageV2 = unsafe { &*storage.cast::<CStringArgStorageV2>() };
  Arg::new(&storage.pointer)
}

unsafe fn cstring_from_ffi_call_ret<'env>(
  env: &'env Env,
  storage: *const u8,
) -> Result<Unknown<'env>> {
  let pointer = read_scalar!(storage, *const c_char);
  if pointer.is_null() {
    return ().into_unknown(env);
  }
  let value = unsafe { CStr::from_ptr(pointer) }
    .to_str()
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))?
    .to_owned();
  value.into_unknown(env)
}

unsafe fn cstring_from_ffi_back_arg<'env>(
  env: &'env Env,
  arg_ptr: *const c_void,
  _index: usize,
) -> Result<Unknown<'env>> {
  let pointer = read_scalar!(arg_ptr, *const c_char);
  if pointer.is_null() {
    return ().into_unknown(env);
  }
  let value = unsafe { CStr::from_ptr(pointer) }
    .to_str()
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))?
    .to_owned();
  value.into_unknown(env)
}

unsafe fn cstring_to_ffi_back_ret<'env>(
  _env: &'env Env,
  value: Unknown<'env>,
  storage: *mut u8,
  scope: &mut FormalizedStorageScope,
) -> Result<()> {
  let storage = storage.cast::<CStringArgStorageV2>();
  let pointer = match value.get_type() {
    Ok(ValueType::Null | ValueType::Undefined) => ptr::null(),
    Ok(ValueType::String) => {
      let string = cast_string(value).map_err(|_| invalid_callback_return())?;
      let c_string = CString::new(string).map_err(|_| invalid_callback_return())?;
      scope.keep_c_string(c_string)
    }
    _ => {
      raw_pointer_from_unknown(value, 0).map_err(|_| invalid_callback_return())? as *const c_char
    }
  };
  unsafe { ptr::write(storage, CStringArgStorageV2 { pointer }) };
  Ok(())
}

unsafe fn cstring_callback_return_ptr(storage: *const u8) -> *const c_void {
  unsafe { ptr::addr_of!((*storage.cast::<CStringArgStorageV2>()).pointer).cast() }
}

pub const STRING_OPS: TypeOps = TypeOps {
  function_arg_layout: cstring_arg_layout,
  formalize_function_arg: cstring_to_ffi_call_arg,
  function_arg_as_ffi_arg: cstring_arg_as_ffi_arg,
  function_return_layout: scalar_layout::<*const c_char>,
  formalize_function_return: cstring_from_ffi_call_ret,
  formalize_callback_arg: cstring_from_ffi_back_arg,
  callback_return_layout: cstring_arg_layout,
  callback_return_copy_size: scalar_size::<*const c_char>,
  formalize_callback_return: cstring_to_ffi_back_ret,
  callback_return_ptr: cstring_callback_return_ptr,
};

fn void_layout() -> Layout {
  Layout::new::<()>()
}

unsafe fn void_to_ffi_call_arg<'env>(
  _env: &'env Env,
  _value: Unknown<'env>,
  index: usize,
  _storage: *mut u8,
  _scope: &mut FormalizedStorageScope,
) -> Result<()> {
  Err(invalid_arg_value(format!(
    "Argument {index} cannot use void as an argument type"
  )))
}

unsafe fn void_arg_as_ffi_arg(_storage: *const u8) -> Arg<'static> {
  unreachable!("void cannot be used as a function argument")
}

unsafe fn void_from_ffi_call_ret<'env>(
  env: &'env Env,
  _storage: *const u8,
) -> Result<Unknown<'env>> {
  ().into_unknown(env)
}

unsafe fn void_from_ffi_back_arg<'env>(
  _env: &'env Env,
  _arg_ptr: *const c_void,
  _index: usize,
) -> Result<Unknown<'env>> {
  Err(Error::new(
    Status::InvalidArg,
    "Void cannot be formalized as a callback argument".to_owned(),
  ))
}

unsafe fn void_to_ffi_back_ret<'env>(
  _env: &'env Env,
  _value: Unknown<'env>,
  _storage: *mut u8,
  _scope: &mut FormalizedStorageScope,
) -> Result<()> {
  Ok(())
}

unsafe fn void_callback_return_ptr(_storage: *const u8) -> *const c_void {
  ptr::null()
}

pub const VOID_OPS: TypeOps = TypeOps {
  function_arg_layout: void_layout,
  formalize_function_arg: void_to_ffi_call_arg,
  function_arg_as_ffi_arg: void_arg_as_ffi_arg,
  function_return_layout: void_layout,
  formalize_function_return: void_from_ffi_call_ret,
  formalize_callback_arg: void_from_ffi_back_arg,
  callback_return_layout: void_layout,
  callback_return_copy_size: scalar_size::<()>,
  formalize_callback_return: void_to_ffi_back_ret,
  callback_return_ptr: void_callback_return_ptr,
};
