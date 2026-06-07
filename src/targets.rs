use std::alloc::Layout;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

use libffi::middle::Arg;
use napi::bindgen_prelude::i64n;
use napi::bindgen_prelude::*;
use napi::Env;

use crate::value_helpers::raw_bytes_pointer;

pub struct TypeOps {
  pub function_arg_layout: Layout,
  pub to_ffi: for<'env> unsafe fn(
    Unknown<'env>,
    *mut u8,
    &mut FormalizedStorageScope,
    ToFfiContext,
  ) -> Result<()>,
  pub function_arg_as_ffi_arg: unsafe fn(*const u8) -> Arg<'static>,
  pub function_return_layout: Layout,
  pub from_ffi: for<'env> unsafe fn(&'env Env, *const c_void) -> Result<Unknown<'env>>,
  pub callback_return_layout: Layout,
  pub callback_return_copy_size: usize,
  pub callback_return_ptr: unsafe fn(*const u8) -> *const c_void,
}

#[derive(Clone, Copy)]
pub enum ToFfiContext {
  CallArg { index: usize },
  CallbackReturn,
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

unsafe fn scalar_arg_as_ffi_arg<T: 'static>(storage: *const u8) -> Arg<'static> {
  let value: &'static T = unsafe { &*storage.cast::<T>() };
  Arg::new(value)
}

unsafe fn scalar_callback_return_ptr(storage: *const u8) -> *const c_void {
  storage.cast()
}

macro_rules! numeric_type_ops {
  (
    $rust_ty:ty,
    $env_name:ident,
    $val_name:ident,
    $idx_name:ident,
    $arg_check:expr,
    $callback_check:expr,
    $to_js:expr
    $(,)?
  ) => {
    TypeOps {
      function_arg_layout: Layout::new::<$rust_ty>(),
      to_ffi: |$val_name, storage, _scope, context| {
        let parsed: $rust_ty = match context {
          ToFfiContext::CallArg { index: $idx_name } => $arg_check,
          ToFfiContext::CallbackReturn => $callback_check,
        };
        unsafe { ptr::write(storage.cast::<$rust_ty>(), parsed) };
        Ok(())
      },
      function_arg_as_ffi_arg: scalar_arg_as_ffi_arg::<$rust_ty>,
      function_return_layout: Layout::new::<$rust_ty>(),
      from_ffi: |$env_name, value_ptr| {
        let $val_name: $rust_ty = read_scalar!(value_ptr, $rust_ty);
        let result = $to_js;
        Ok(result)
      },
      callback_return_layout: Layout::new::<$rust_ty>(),
      callback_return_copy_size: std::mem::size_of::<$rust_ty>(),
      callback_return_ptr: scalar_callback_return_ptr,
    }
  };
}

pub const I8_OPS: TypeOps = numeric_type_ops!(
  i8,
  env,
  value,
  index,
  { number_to_signed_integer(value, index, i8::MIN, i8::MAX, "int8")? },
  { callback_to_signed_integer(value, i8::MIN, i8::MAX)? },
  { i8::into_unknown(value, env)? },
);

pub const U8_OPS: TypeOps = numeric_type_ops!(
  u8,
  env,
  value,
  index,
  { number_to_unsigned_integer(value, index, u8::MAX, "uint8")? },
  { callback_to_unsigned_integer(value, u8::MAX)? },
  { u8::into_unknown(value, env)? },
);

pub const I16_OPS: TypeOps = numeric_type_ops!(
  i16,
  env,
  value,
  index,
  { number_to_signed_integer(value, index, i16::MIN, i16::MAX, "int16")? },
  { callback_to_signed_integer(value, i16::MIN, i16::MAX)? },
  { i16::into_unknown(value, env)? },
);

pub const U16_OPS: TypeOps = numeric_type_ops!(
  u16,
  env,
  value,
  index,
  { number_to_unsigned_integer(value, index, u16::MAX, "uint16")? },
  { callback_to_unsigned_integer(value, u16::MAX)? },
  { u16::into_unknown(value, env)? },
);

pub const I32_OPS: TypeOps = numeric_type_ops!(
  i32,
  env,
  value,
  index,
  { number_to_signed_integer(value, index, i32::MIN, i32::MAX, "int32")? },
  { callback_to_signed_integer(value, i32::MIN, i32::MAX)? },
  { i32::into_unknown(value, env)? },
);

pub const U32_OPS: TypeOps = numeric_type_ops!(
  u32,
  env,
  value,
  index,
  { number_to_unsigned_integer(value, index, u32::MAX, "uint32")? },
  { callback_to_unsigned_integer(value, u32::MAX)? },
  { u32::into_unknown(value, env)? },
);

pub const I64_OPS: TypeOps = numeric_type_ops!(
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

pub const U64_OPS: TypeOps = numeric_type_ops!(
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

pub const F32_OPS: TypeOps = numeric_type_ops!(
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

pub const F64_OPS: TypeOps = numeric_type_ops!(
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

unsafe fn pointer_to_ffi<'env>(
  value: Unknown<'env>,
  storage: *mut u8,
  scope: &mut FormalizedStorageScope,
  context: ToFfiContext,
) -> Result<()> {
  let pointer = match context {
    ToFfiContext::CallArg { index } => match pointer_argument_from_unknown(value, index)? {
      PointerArgumentCategory::String(c_string) => scope.keep_c_string(c_string) as *mut c_void,
      PointerArgumentCategory::Regular(pointer) => pointer,
    },
    ToFfiContext::CallbackReturn => match value.get_type() {
      Ok(ValueType::Null | ValueType::Undefined) => ptr::null_mut(),
      _ => raw_pointer_from_unknown(value, 0).map_err(|_| invalid_callback_return())?,
    },
  };
  unsafe {
    ptr::write(
      storage.cast::<PointerArgStorageV2>(),
      PointerArgStorageV2 { pointer },
    )
  };
  Ok(())
}

unsafe fn pointer_arg_as_ffi_arg(storage: *const u8) -> Arg<'static> {
  let storage: &'static PointerArgStorageV2 = unsafe { &*storage.cast::<PointerArgStorageV2>() };
  Arg::new(&storage.pointer)
}

unsafe fn pointer_from_ffi<'env>(
  env: &'env Env,
  value_ptr: *const c_void,
) -> Result<Unknown<'env>> {
  BigInt::from(read_scalar!(value_ptr, usize) as u64).into_unknown(env)
}

pub const POINTER_OPS: TypeOps = TypeOps {
  function_arg_layout: Layout::new::<PointerArgStorageV2>(),
  to_ffi: pointer_to_ffi,
  function_arg_as_ffi_arg: pointer_arg_as_ffi_arg,
  function_return_layout: Layout::new::<*mut c_void>(),
  from_ffi: pointer_from_ffi,
  callback_return_layout: Layout::new::<PointerArgStorageV2>(),
  callback_return_copy_size: std::mem::size_of::<*mut c_void>(),
  callback_return_ptr: scalar_callback_return_ptr,
};

#[repr(C)]
struct CStringArgStorageV2 {
  pointer: *const c_char,
}

unsafe fn cstring_to_ffi<'env>(
  value: Unknown<'env>,
  storage: *mut u8,
  scope: &mut FormalizedStorageScope,
  context: ToFfiContext,
) -> Result<()> {
  let pointer = match context {
    ToFfiContext::CallArg { index } => match value.get_type()? {
      ValueType::String => {
        let string = cast_string(value)?;
        let c_string = CString::new(string).map_err(|_| {
          invalid_arg_value(format!("Argument {index} must not contain null bytes"))
        })?;
        scope.keep_c_string(c_string)
      }
      _ => raw_pointer_from_unknown(value, index)? as *const c_char,
    },
    ToFfiContext::CallbackReturn => match value.get_type() {
      Ok(ValueType::Null | ValueType::Undefined) => ptr::null(),
      Ok(ValueType::String) => {
        let string = cast_string(value).map_err(|_| invalid_callback_return())?;
        let c_string = CString::new(string).map_err(|_| invalid_callback_return())?;
        scope.keep_c_string(c_string)
      }
      _ => {
        raw_pointer_from_unknown(value, 0).map_err(|_| invalid_callback_return())? as *const c_char
      }
    },
  };
  unsafe {
    ptr::write(
      storage.cast::<CStringArgStorageV2>(),
      CStringArgStorageV2 { pointer },
    )
  };
  Ok(())
}

unsafe fn cstring_arg_as_ffi_arg(storage: *const u8) -> Arg<'static> {
  let storage: &'static CStringArgStorageV2 = unsafe { &*storage.cast::<CStringArgStorageV2>() };
  Arg::new(&storage.pointer)
}

unsafe fn cstring_from_ffi<'env>(
  env: &'env Env,
  value_ptr: *const c_void,
) -> Result<Unknown<'env>> {
  let pointer = read_scalar!(value_ptr, *const c_char);
  if pointer.is_null() {
    return ().into_unknown(env);
  }
  let value = unsafe { CStr::from_ptr(pointer) }
    .to_str()
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))?
    .to_owned();
  value.into_unknown(env)
}

unsafe fn cstring_callback_return_ptr(storage: *const u8) -> *const c_void {
  unsafe { ptr::addr_of!((*storage.cast::<CStringArgStorageV2>()).pointer).cast() }
}

pub const STRING_OPS: TypeOps = TypeOps {
  function_arg_layout: Layout::new::<CStringArgStorageV2>(),
  to_ffi: cstring_to_ffi,
  function_arg_as_ffi_arg: cstring_arg_as_ffi_arg,
  function_return_layout: Layout::new::<*const c_char>(),
  from_ffi: cstring_from_ffi,
  callback_return_layout: Layout::new::<CStringArgStorageV2>(),
  callback_return_copy_size: std::mem::size_of::<*const c_char>(),
  callback_return_ptr: cstring_callback_return_ptr,
};

unsafe fn void_to_ffi<'env>(
  _value: Unknown<'env>,
  _storage: *mut u8,
  _scope: &mut FormalizedStorageScope,
  context: ToFfiContext,
) -> Result<()> {
  match context {
    ToFfiContext::CallArg { index } => Err(invalid_arg_value(format!(
      "Argument {index} cannot use void as an argument type"
    ))),
    ToFfiContext::CallbackReturn => Ok(()),
  }
}

unsafe fn void_arg_as_ffi_arg(_storage: *const u8) -> Arg<'static> {
  unreachable!("void cannot be used as a function argument")
}

unsafe fn void_from_ffi<'env>(env: &'env Env, value_ptr: *const c_void) -> Result<Unknown<'env>> {
  if value_ptr.is_null() {
    ().into_unknown(env)
  } else {
    Err(Error::new(
      Status::InvalidArg,
      "Void cannot be formalized as a callback argument".to_owned(),
    ))
  }
}

unsafe fn void_callback_return_ptr(_storage: *const u8) -> *const c_void {
  ptr::null()
}

pub const VOID_OPS: TypeOps = TypeOps {
  function_arg_layout: Layout::new::<()>(),
  to_ffi: void_to_ffi,
  function_arg_as_ffi_arg: void_arg_as_ffi_arg,
  function_return_layout: Layout::new::<()>(),
  from_ffi: void_from_ffi,
  callback_return_layout: Layout::new::<()>(),
  callback_return_copy_size: std::mem::size_of::<()>(),
  callback_return_ptr: void_callback_return_ptr,
};
