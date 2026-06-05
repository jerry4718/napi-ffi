use std::alloc::Layout;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

use libffi::middle::{Arg, Cif, CodePtr, Type as FFIType};
use napi::bindgen_prelude::i64n;
use napi::bindgen_prelude::*;
use napi::Env;

use crate::value_helpers::raw_bytes_pointer;

pub trait TypedTarget: Send + Sync + 'static {
  fn type_name(&self) -> &'static str;
  fn ffi_type(&self) -> FFIType;

  fn function_arg_layout(&self) -> Layout;

  unsafe fn formalize_function_arg<'env>(
    &self,
    env: &'env Env,
    value: Unknown<'env>,
    index: usize,
    storage: *mut u8,
  ) -> Result<()>;

  unsafe fn function_arg_as_ffi_arg<'a>(&self, storage: *const u8) -> Arg<'a>;

  unsafe fn drop_function_arg(&self, storage: *mut u8);

  unsafe fn formalize_function_return<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>>;

  unsafe fn formalize_callback_arg<'env>(
    &self,
    env: &'env Env,
    arg_ptr: *const c_void,
    index: usize,
  ) -> Result<Unknown<'env>>;

  fn callback_return_layout(&self) -> Layout;

  fn callback_return_copy_size(&self) -> usize {
    self.callback_return_layout().size()
  }

  unsafe fn formalize_callback_return<'env>(
    &self,
    env: &'env Env,
    value: Unknown<'env>,
    storage: *mut u8,
  ) -> Result<()>;

  unsafe fn callback_return_ptr(&self, storage: *const u8) -> *const c_void;

  unsafe fn drop_callback_return(&self, storage: *mut u8);
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

macro_rules! numeric_target {
  (
    $name:ident ($type_name:literal, $rust_ty:ty, $ffi_type:expr, $env_name:ident, $val_name:ident, $idx_name:ident),
    $arg_check:expr,
    $callback_check:expr
    $(,)?
  ) => {
    numeric_target!(
      $name($type_name, $rust_ty, $ffi_type, $env_name, $val_name, $idx_name),
      $arg_check,
      $callback_check,
      { <$rust_ty>::into_unknown($val_name, $env_name)? },
    );
  };
  (
    $name:ident ($type_name:literal, $rust_ty:ty, $ffi_type:expr, $env_name:ident, $val_name:ident, $idx_name:ident),
    $arg_check:expr,
    $callback_check:expr,
    $to_js:expr
    $(,)?
  ) => {
    pub struct $name;

    impl TypedTarget for $name {
      fn type_name(&self) -> &'static str {
        $type_name
      }

      fn ffi_type(&self) -> FFIType {
        $ffi_type
      }

      fn function_arg_layout(&self) -> Layout {
        Layout::new::<$rust_ty>()
      }

      unsafe fn formalize_function_arg<'env>(
        &self,
        _env: &'env Env,
        $val_name: Unknown<'env>,
        $idx_name: usize,
        storage: *mut u8,
      ) -> Result<()> {
        let parsed: $rust_ty = $arg_check;
        unsafe { ptr::write(storage.cast::<$rust_ty>(), parsed) };
        Ok(())
      }

      unsafe fn function_arg_as_ffi_arg<'a>(&self, storage: *const u8) -> Arg<'a> {
        Arg::new(unsafe { &*storage.cast::<$rust_ty>() })
      }

      unsafe fn drop_function_arg(&self, _storage: *mut u8) {}

      unsafe fn formalize_function_return<'env>(
        &self,
        $env_name: &'env Env,
        cif: &Cif,
        fn_ptr: CodePtr,
        args: &[Arg<'_>],
      ) -> Result<Unknown<'env>> {
        let $val_name: $rust_ty = unsafe { cif.call(fn_ptr, args) };
        let result = $to_js;
        Ok(result)
      }

      unsafe fn formalize_callback_arg<'env>(
        &self,
        $env_name: &'env Env,
        arg_ptr: *const c_void,
        _index: usize,
      ) -> Result<Unknown<'env>> {
        let $val_name: $rust_ty = read_scalar!(arg_ptr, $rust_ty);
        let result = $to_js;
        Ok(result)
      }

      fn callback_return_layout(&self) -> Layout {
        Layout::new::<$rust_ty>()
      }

      unsafe fn formalize_callback_return<'env>(
        &self,
        _env: &'env Env,
        $val_name: Unknown<'env>,
        storage: *mut u8,
      ) -> Result<()> {
        let parsed: $rust_ty = $callback_check;
        unsafe { ptr::write_unaligned(storage.cast::<$rust_ty>(), parsed) };
        Ok(())
      }

      unsafe fn callback_return_ptr(&self, storage: *const u8) -> *const c_void {
        storage.cast()
      }

      unsafe fn drop_callback_return(&self, _storage: *mut u8) {}
    }
  };
}

numeric_target!(
  I8Target("int8", i8, FFIType::i8(), env, value, index),
  { number_to_signed_integer(value, index, i8::MIN, i8::MAX, "int8")? },
  { callback_to_signed_integer(value, i8::MIN, i8::MAX)? },
);

numeric_target!(
  U8Target("uint8", u8, FFIType::u8(), env, value, index),
  { number_to_unsigned_integer(value, index, u8::MAX, "uint8")? },
  { callback_to_unsigned_integer(value, u8::MAX)? },
);

numeric_target!(
  I16Target("int16", i16, FFIType::i16(), env, value, index),
  { number_to_signed_integer(value, index, i16::MIN, i16::MAX, "int16")? },
  { callback_to_signed_integer(value, i16::MIN, i16::MAX)? },
);

numeric_target!(
  U16Target("uint16", u16, FFIType::u16(), env, value, index),
  { number_to_unsigned_integer(value, index, u16::MAX, "uint16")? },
  { callback_to_unsigned_integer(value, u16::MAX)? },
);

numeric_target!(
  I32Target("int32", i32, FFIType::i32(), env, value, index),
  { number_to_signed_integer(value, index, i32::MIN, i32::MAX, "int32")? },
  { callback_to_signed_integer(value, i32::MIN, i32::MAX)? },
);

numeric_target!(
  U32Target("uint32", u32, FFIType::u32(), env, value, index),
  { number_to_unsigned_integer(value, index, u32::MAX, "uint32")? },
  { callback_to_unsigned_integer(value, u32::MAX)? },
);

numeric_target!(
  I64Target("int64", i64, FFIType::i64(), env, value, index),
  {
    let bigint = cast_bigint(value)?;
    bigint_to_i64(&bigint)
      .ok_or_else(|| invalid_arg_value(format!("Argument {index} must be an int64")))?
  },
  {
    let bigint = cast_bigint(value).map_err(|_| invalid_callback_return())?;
    bigint_to_i64(&bigint).ok_or_else(invalid_callback_return)?
  },
  { i64n(value).into_unknown(env)? }
);

numeric_target!(
  U64Target("uint64", u64, FFIType::u64(), env, value, index),
  {
    let bigint = cast_bigint(value)?;
    bigint_to_u64(&bigint)
      .ok_or_else(|| invalid_arg_value(format!("Argument {index} must be a uint64")))?
  },
  {
    let bigint = cast_bigint(value).map_err(|_| invalid_callback_return())?;
    bigint_to_u64(&bigint).ok_or_else(invalid_callback_return)?
  },
);

numeric_target!(
  F32Target("float32", f32, FFIType::f32(), env, value, index),
  {
    cast_f64(value).map_err(|_| invalid_arg_value(format!("Argument {index} must be a float")))?
      as f32
  },
  { cast_f64(value).map_err(|_| invalid_callback_return())? as f32 },
);

numeric_target!(
  F64Target("float64", f64, FFIType::f64(), env, value, index),
  { cast_f64(value).map_err(|_| invalid_arg_value(format!("Argument {index} must be a double")))? },
  { cast_f64(value).map_err(|_| invalid_callback_return())? },
);

#[repr(C)]
struct PointerArgStorage {
  pointer: *mut c_void,
  owned_c_string: Option<CString>,
}

pub struct PointerTarget;

impl TypedTarget for PointerTarget {
  fn type_name(&self) -> &'static str {
    "pointer"
  }

  fn ffi_type(&self) -> FFIType {
    FFIType::pointer()
  }

  fn function_arg_layout(&self) -> Layout {
    Layout::new::<PointerArgStorage>()
  }

  unsafe fn formalize_function_arg<'env>(
    &self,
    _env: &'env Env,
    value: Unknown<'env>,
    index: usize,
    storage: *mut u8,
  ) -> Result<()> {
    let storage = storage.cast::<PointerArgStorage>();
    match pointer_argument_from_unknown(value, index)? {
      PointerArgumentCategory::String(c_string) => unsafe {
        ptr::write(
          storage,
          PointerArgStorage {
            pointer: c_string.as_ptr() as *mut c_void,
            owned_c_string: Some(c_string),
          },
        )
      },
      PointerArgumentCategory::Regular(pointer) => unsafe {
        ptr::write(
          storage,
          PointerArgStorage {
            pointer,
            owned_c_string: None,
          },
        )
      },
    }
    Ok(())
  }

  unsafe fn function_arg_as_ffi_arg<'a>(&self, storage: *const u8) -> Arg<'a> {
    let storage = unsafe { &*storage.cast::<PointerArgStorage>() };
    Arg::new(&storage.pointer)
  }

  unsafe fn drop_function_arg(&self, storage: *mut u8) {
    unsafe { ptr::drop_in_place(storage.cast::<PointerArgStorage>()) };
  }

  unsafe fn formalize_function_return<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>> {
    let value: *mut c_void = unsafe { cif.call(fn_ptr, args) };
    BigInt::from(value as usize as u64).into_unknown(env)
  }

  unsafe fn formalize_callback_arg<'env>(
    &self,
    env: &'env Env,
    arg_ptr: *const c_void,
    _index: usize,
  ) -> Result<Unknown<'env>> {
    BigInt::from(read_scalar!(arg_ptr, usize) as u64).into_unknown(env)
  }

  fn callback_return_layout(&self) -> Layout {
    Layout::new::<*mut c_void>()
  }

  unsafe fn formalize_callback_return<'env>(
    &self,
    _env: &'env Env,
    value: Unknown<'env>,
    storage: *mut u8,
  ) -> Result<()> {
    let pointer = match value.get_type() {
      Ok(ValueType::Null | ValueType::Undefined) => ptr::null_mut(),
      _ => raw_pointer_from_unknown(value, 0).map_err(|_| invalid_callback_return())?,
    };
    unsafe { ptr::write(storage.cast::<*mut c_void>(), pointer) };
    Ok(())
  }

  unsafe fn callback_return_ptr(&self, storage: *const u8) -> *const c_void {
    storage.cast()
  }

  unsafe fn drop_callback_return(&self, _storage: *mut u8) {}
}

#[repr(C)]
struct CStringArgStorage {
  pointer: *const c_char,
  owned: Option<CString>,
}

pub struct StringTarget;

impl TypedTarget for StringTarget {
  fn type_name(&self) -> &'static str {
    "string"
  }

  fn ffi_type(&self) -> FFIType {
    FFIType::pointer()
  }

  fn function_arg_layout(&self) -> Layout {
    Layout::new::<CStringArgStorage>()
  }

  unsafe fn formalize_function_arg<'env>(
    &self,
    _env: &'env Env,
    value: Unknown<'env>,
    index: usize,
    storage: *mut u8,
  ) -> Result<()> {
    let storage = storage.cast::<CStringArgStorage>();
    match value.get_type()? {
      ValueType::String => {
        let string = cast_string(value)?;
        let c_string = CString::new(string).map_err(|_| {
          invalid_arg_value(format!("Argument {index} must not contain null bytes"))
        })?;
        unsafe {
          ptr::write(
            storage,
            CStringArgStorage {
              pointer: c_string.as_ptr(),
              owned: Some(c_string),
            },
          )
        };
      }
      _ => unsafe {
        ptr::write(
          storage,
          CStringArgStorage {
            pointer: raw_pointer_from_unknown(value, index)? as *const c_char,
            owned: None,
          },
        )
      },
    }
    Ok(())
  }

  unsafe fn function_arg_as_ffi_arg<'a>(&self, storage: *const u8) -> Arg<'a> {
    let storage = unsafe { &*storage.cast::<CStringArgStorage>() };
    Arg::new(&storage.pointer)
  }

  unsafe fn drop_function_arg(&self, storage: *mut u8) {
    unsafe { ptr::drop_in_place(storage.cast::<CStringArgStorage>()) };
  }

  unsafe fn formalize_function_return<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>> {
    let pointer: *const c_char = unsafe { cif.call(fn_ptr, args) };
    if pointer.is_null() {
      return ().into_unknown(env);
    }
    let value = unsafe { CStr::from_ptr(pointer) }
      .to_str()
      .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))?
      .to_owned();
    value.into_unknown(env)
  }

  unsafe fn formalize_callback_arg<'env>(
    &self,
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

  fn callback_return_layout(&self) -> Layout {
    Layout::new::<CStringArgStorage>()
  }

  fn callback_return_copy_size(&self) -> usize {
    Layout::new::<*const c_char>().size()
  }

  unsafe fn formalize_callback_return<'env>(
    &self,
    _env: &'env Env,
    value: Unknown<'env>,
    storage: *mut u8,
  ) -> Result<()> {
    let storage = storage.cast::<CStringArgStorage>();
    match value.get_type() {
      Ok(ValueType::Null | ValueType::Undefined) => unsafe {
        ptr::write(
          storage,
          CStringArgStorage {
            pointer: ptr::null(),
            owned: None,
          },
        )
      },
      Ok(ValueType::String) => {
        let string = cast_string(value).map_err(|_| invalid_callback_return())?;
        let c_string = CString::new(string).map_err(|_| invalid_callback_return())?;
        unsafe {
          ptr::write(
            storage,
            CStringArgStorage {
              pointer: c_string.as_ptr(),
              owned: Some(c_string),
            },
          )
        };
      }
      _ => unsafe {
        ptr::write(
          storage,
          CStringArgStorage {
            pointer: raw_pointer_from_unknown(value, 0).map_err(|_| invalid_callback_return())?
              as *const c_char,
            owned: None,
          },
        )
      },
    }
    Ok(())
  }

  unsafe fn callback_return_ptr(&self, storage: *const u8) -> *const c_void {
    unsafe { ptr::addr_of!((*storage.cast::<CStringArgStorage>()).pointer).cast() }
  }

  unsafe fn drop_callback_return(&self, storage: *mut u8) {
    unsafe { ptr::drop_in_place(storage.cast::<CStringArgStorage>()) };
  }
}

macro_rules! pointer_alias_target {
  ($name:ident, $type_name:literal) => {
    pub struct $name;

    impl TypedTarget for $name {
      fn type_name(&self) -> &'static str {
        $type_name
      }

      fn ffi_type(&self) -> FFIType {
        FFIType::pointer()
      }

      fn function_arg_layout(&self) -> Layout {
        PointerTarget.function_arg_layout()
      }

      unsafe fn formalize_function_arg<'env>(
        &self,
        env: &'env Env,
        value: Unknown<'env>,
        index: usize,
        storage: *mut u8,
      ) -> Result<()> {
        PointerTarget.formalize_function_arg(env, value, index, storage)
      }

      unsafe fn function_arg_as_ffi_arg<'a>(&self, storage: *const u8) -> Arg<'a> {
        PointerTarget.function_arg_as_ffi_arg(storage)
      }

      unsafe fn drop_function_arg(&self, storage: *mut u8) {
        PointerTarget.drop_function_arg(storage)
      }

      unsafe fn formalize_function_return<'env>(
        &self,
        env: &'env Env,
        cif: &Cif,
        fn_ptr: CodePtr,
        args: &[Arg<'_>],
      ) -> Result<Unknown<'env>> {
        PointerTarget.formalize_function_return(env, cif, fn_ptr, args)
      }

      unsafe fn formalize_callback_arg<'env>(
        &self,
        env: &'env Env,
        arg_ptr: *const c_void,
        index: usize,
      ) -> Result<Unknown<'env>> {
        PointerTarget.formalize_callback_arg(env, arg_ptr, index)
      }

      fn callback_return_layout(&self) -> Layout {
        PointerTarget.callback_return_layout()
      }

      unsafe fn formalize_callback_return<'env>(
        &self,
        env: &'env Env,
        value: Unknown<'env>,
        storage: *mut u8,
      ) -> Result<()> {
        PointerTarget.formalize_callback_return(env, value, storage)
      }

      unsafe fn callback_return_ptr(&self, storage: *const u8) -> *const c_void {
        PointerTarget.callback_return_ptr(storage)
      }

      unsafe fn drop_callback_return(&self, storage: *mut u8) {
        PointerTarget.drop_callback_return(storage)
      }
    }
  };
}

pointer_alias_target!(BufferTarget, "buffer");
pointer_alias_target!(ArrayBufferTarget, "arraybuffer");
pointer_alias_target!(FunctionTarget, "function");

pub struct VoidTarget;

impl TypedTarget for VoidTarget {
  fn type_name(&self) -> &'static str {
    "void"
  }

  fn ffi_type(&self) -> FFIType {
    FFIType::void()
  }

  fn function_arg_layout(&self) -> Layout {
    Layout::new::<()>()
  }

  unsafe fn formalize_function_arg<'env>(
    &self,
    _env: &'env Env,
    _value: Unknown<'env>,
    index: usize,
    _storage: *mut u8,
  ) -> Result<()> {
    Err(invalid_arg_value(format!(
      "Argument {index} cannot use void as an argument type"
    )))
  }

  unsafe fn function_arg_as_ffi_arg<'a>(&self, _storage: *const u8) -> Arg<'a> {
    unreachable!("void cannot be used as a function argument")
  }

  unsafe fn drop_function_arg(&self, _storage: *mut u8) {}

  unsafe fn formalize_function_return<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>> {
    let _: () = unsafe { cif.call(fn_ptr, args) };
    ().into_unknown(env)
  }

  unsafe fn formalize_callback_arg<'env>(
    &self,
    _env: &'env Env,
    _arg_ptr: *const c_void,
    _index: usize,
  ) -> Result<Unknown<'env>> {
    Err(Error::new(
      Status::InvalidArg,
      "Void cannot be formalized as a callback argument".to_owned(),
    ))
  }

  fn callback_return_layout(&self) -> Layout {
    Layout::new::<()>()
  }

  unsafe fn formalize_callback_return<'env>(
    &self,
    _env: &'env Env,
    _value: Unknown<'env>,
    _storage: *mut u8,
  ) -> Result<()> {
    Ok(())
  }

  unsafe fn callback_return_ptr(&self, _storage: *const u8) -> *const c_void {
    ptr::null()
  }

  unsafe fn drop_callback_return(&self, _storage: *mut u8) {}
}
