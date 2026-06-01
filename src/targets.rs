use std::alloc::Layout;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

use libffi::middle::{Arg, Cif, CodePtr, Type as FFIType};
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

fn invalid_arg_value(message: impl Into<String>) -> Error {
  Error::new(Status::InvalidArg, message.into())
}

fn invalid_callback_return() -> Error {
  Error::new(
    Status::InvalidArg,
    "Callback returned invalid value for declared FFI type".to_owned(),
  )
}

fn bigint_to_u64(value: &BigInt, message: &str) -> Result<u64> {
  let (signed, raw, lossless) = value.get_u64();
  if signed || !lossless {
    return Err(invalid_arg_value(message));
  }
  Ok(raw)
}

fn bigint_to_i64(value: &BigInt, message: &str) -> Result<i64> {
  let (raw, lossless) = value.get_i64();
  if !lossless {
    return Err(invalid_arg_value(message));
  }
  Ok(raw)
}

fn cast_f64(value: Unknown<'_>) -> Result<f64> {
  unsafe { value.cast() }
}

fn cast_bigint(value: Unknown<'_>) -> Result<BigInt> {
  unsafe { value.cast() }
}

fn cast_string(value: Unknown<'_>) -> Result<String> {
  unsafe { value.cast() }
}

fn raw_pointer_from_unknown(value: Unknown<'_>, index: usize) -> Result<*mut c_void> {
  match value.get_type()? {
    ValueType::Null | ValueType::Undefined => Ok(ptr::null_mut()),
    ValueType::BigInt => {
      let bigint = cast_bigint(value)?;
      let raw = bigint_to_u64(
        &bigint,
        &format!("Argument {index} must be a non-negative pointer bigint"),
      )?;
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
    $name:ident,
    $ffi_type:expr,
    $rust_ty:ty,
    $type_name:literal,
    $arg_check:expr,
    $callback_check:expr,
    $to_js:expr
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
        value: Unknown<'env>,
        index: usize,
        storage: *mut u8,
      ) -> Result<()> {
        let parsed: $rust_ty = $arg_check(value, index)?;
        unsafe { ptr::write(storage.cast::<$rust_ty>(), parsed) };
        Ok(())
      }

      unsafe fn function_arg_as_ffi_arg<'a>(&self, storage: *const u8) -> Arg<'a> {
        Arg::new(unsafe { &*storage.cast::<$rust_ty>() })
      }

      unsafe fn drop_function_arg(&self, _storage: *mut u8) {}

      unsafe fn formalize_function_return<'env>(
        &self,
        env: &'env Env,
        cif: &Cif,
        fn_ptr: CodePtr,
        args: &[Arg<'_>],
      ) -> Result<Unknown<'env>> {
        let value: $rust_ty = unsafe { cif.call(fn_ptr, args) };
        $to_js(env, value)
      }

      unsafe fn formalize_callback_arg<'env>(
        &self,
        env: &'env Env,
        arg_ptr: *const c_void,
        _index: usize,
      ) -> Result<Unknown<'env>> {
        let value: $rust_ty = read_scalar!(arg_ptr, $rust_ty);
        $to_js(env, value)
      }

      fn callback_return_layout(&self) -> Layout {
        Layout::new::<$rust_ty>()
      }

      unsafe fn formalize_callback_return<'env>(
        &self,
        _env: &'env Env,
        value: Unknown<'env>,
        storage: *mut u8,
      ) -> Result<()> {
        let parsed: $rust_ty = $callback_check(value)?;
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

fn number_to_i8(value: Unknown<'_>, index: usize) -> Result<i8> {
  let number = cast_f64(value)?;
  if number.fract() != 0.0 || !(-128.0..=127.0).contains(&number) {
    return Err(invalid_arg_value(format!(
      "Argument {index} must be an int8"
    )));
  }
  Ok(number as i8)
}

fn number_to_u8(value: Unknown<'_>, index: usize) -> Result<u8> {
  let number = cast_f64(value)?;
  if number.fract() != 0.0 || !(0.0..=255.0).contains(&number) {
    return Err(invalid_arg_value(format!(
      "Argument {index} must be a uint8"
    )));
  }
  Ok(number as u8)
}

fn number_to_i16(value: Unknown<'_>, index: usize) -> Result<i16> {
  let number = cast_f64(value)?;
  if number.fract() != 0.0 || !(-32768.0..=32767.0).contains(&number) {
    return Err(invalid_arg_value(format!(
      "Argument {index} must be an int16"
    )));
  }
  Ok(number as i16)
}

fn number_to_u16(value: Unknown<'_>, index: usize) -> Result<u16> {
  let number = cast_f64(value)?;
  if number.fract() != 0.0 || !(0.0..=65535.0).contains(&number) {
    return Err(invalid_arg_value(format!(
      "Argument {index} must be a uint16"
    )));
  }
  Ok(number as u16)
}

fn number_to_i32(value: Unknown<'_>, index: usize) -> Result<i32> {
  let number = cast_f64(value)?;
  if !number.is_finite()
    || number.fract() != 0.0
    || !((i32::MIN as f64)..=(i32::MAX as f64)).contains(&number)
  {
    return Err(invalid_arg_value(format!(
      "Argument {index} must be an int32"
    )));
  }
  Ok(number as i32)
}

fn number_to_u32(value: Unknown<'_>, index: usize) -> Result<u32> {
  let number = cast_f64(value)?;
  if !number.is_finite() || number.fract() != 0.0 || !(0.0..=(u32::MAX as f64)).contains(&number) {
    return Err(invalid_arg_value(format!(
      "Argument {index} must be a uint32"
    )));
  }
  Ok(number as u32)
}

fn bigint_to_i64_arg(value: Unknown<'_>, index: usize) -> Result<i64> {
  let bigint = cast_bigint(value)?;
  bigint_to_i64(&bigint, &format!("Argument {index} must be an int64"))
}

fn bigint_to_u64_arg(value: Unknown<'_>, index: usize) -> Result<u64> {
  let bigint = cast_bigint(value)?;
  bigint_to_u64(&bigint, &format!("Argument {index} must be a uint64"))
}

fn number_to_f32(value: Unknown<'_>, _index: usize) -> Result<f32> {
  let number = cast_f64(value)?;
  Ok(number as f32)
}

fn number_to_f64(value: Unknown<'_>, _index: usize) -> Result<f64> {
  let number = cast_f64(value)?;
  Ok(number)
}

fn callback_to_i8(value: Unknown<'_>) -> Result<i8> {
  let number = cast_f64(value)?;
  if number.fract() != 0.0 || !(-128.0..=127.0).contains(&number) {
    return Err(invalid_callback_return());
  }
  Ok(number as i8)
}

fn callback_to_u8(value: Unknown<'_>) -> Result<u8> {
  let number = cast_f64(value)?;
  if number.fract() != 0.0 || !(0.0..=255.0).contains(&number) {
    return Err(invalid_callback_return());
  }
  Ok(number as u8)
}

fn callback_to_i16(value: Unknown<'_>) -> Result<i16> {
  let number = cast_f64(value)?;
  if number.fract() != 0.0 || !(-32768.0..=32767.0).contains(&number) {
    return Err(invalid_callback_return());
  }
  Ok(number as i16)
}

fn callback_to_u16(value: Unknown<'_>) -> Result<u16> {
  let number = cast_f64(value)?;
  if number.fract() != 0.0 || !(0.0..=65535.0).contains(&number) {
    return Err(invalid_callback_return());
  }
  Ok(number as u16)
}

fn callback_to_i32(value: Unknown<'_>) -> Result<i32> {
  let number = cast_f64(value)?;
  if !number.is_finite()
    || number.fract() != 0.0
    || !((i32::MIN as f64)..=(i32::MAX as f64)).contains(&number)
  {
    return Err(invalid_callback_return());
  }
  Ok(number as i32)
}

fn callback_to_u32(value: Unknown<'_>) -> Result<u32> {
  let number = cast_f64(value)?;
  if !number.is_finite() || number.fract() != 0.0 || !(0.0..=(u32::MAX as f64)).contains(&number) {
    return Err(invalid_callback_return());
  }
  Ok(number as u32)
}

fn callback_to_i64(value: Unknown<'_>) -> Result<i64> {
  let bigint = cast_bigint(value).map_err(|_| invalid_callback_return())?;
  bigint_to_i64(
    &bigint,
    "Callback returned invalid value for declared FFI type",
  )
  .map_err(|_| invalid_callback_return())
}

fn callback_to_u64(value: Unknown<'_>) -> Result<u64> {
  let bigint = cast_bigint(value).map_err(|_| invalid_callback_return())?;
  bigint_to_u64(
    &bigint,
    "Callback returned invalid value for declared FFI type",
  )
  .map_err(|_| invalid_callback_return())
}

fn callback_to_f32(value: Unknown<'_>) -> Result<f32> {
  let number = cast_f64(value).map_err(|_| invalid_callback_return())?;
  Ok(number as f32)
}

fn callback_to_f64(value: Unknown<'_>) -> Result<f64> {
  let number = cast_f64(value).map_err(|_| invalid_callback_return())?;
  Ok(number)
}

fn i8_to_js(env: &Env, value: i8) -> Result<Unknown<'_>> {
  value.into_unknown(env)
}

fn u8_to_js(env: &Env, value: u8) -> Result<Unknown<'_>> {
  value.into_unknown(env)
}

fn i16_to_js(env: &Env, value: i16) -> Result<Unknown<'_>> {
  value.into_unknown(env)
}

fn u16_to_js(env: &Env, value: u16) -> Result<Unknown<'_>> {
  value.into_unknown(env)
}

fn i32_to_js(env: &Env, value: i32) -> Result<Unknown<'_>> {
  value.into_unknown(env)
}

fn u32_to_js(env: &Env, value: u32) -> Result<Unknown<'_>> {
  value.into_unknown(env)
}

fn i64_to_js(env: &Env, value: i64) -> Result<Unknown<'_>> {
  BigInt::from(value).into_unknown(env)
}

fn u64_to_js(env: &Env, value: u64) -> Result<Unknown<'_>> {
  BigInt::from(value).into_unknown(env)
}

fn f32_to_js(env: &Env, value: f32) -> Result<Unknown<'_>> {
  f64::from(value).into_unknown(env)
}

fn f64_to_js(env: &Env, value: f64) -> Result<Unknown<'_>> {
  value.into_unknown(env)
}

numeric_target!(
  I8Target,
  FFIType::i8(),
  i8,
  "int8",
  number_to_i8,
  callback_to_i8,
  i8_to_js
);
numeric_target!(
  U8Target,
  FFIType::u8(),
  u8,
  "uint8",
  number_to_u8,
  callback_to_u8,
  u8_to_js
);
numeric_target!(
  I16Target,
  FFIType::i16(),
  i16,
  "int16",
  number_to_i16,
  callback_to_i16,
  i16_to_js
);
numeric_target!(
  U16Target,
  FFIType::u16(),
  u16,
  "uint16",
  number_to_u16,
  callback_to_u16,
  u16_to_js
);
numeric_target!(
  I32Target,
  FFIType::i32(),
  i32,
  "int32",
  number_to_i32,
  callback_to_i32,
  i32_to_js
);
numeric_target!(
  U32Target,
  FFIType::u32(),
  u32,
  "uint32",
  number_to_u32,
  callback_to_u32,
  u32_to_js
);
numeric_target!(
  I64Target,
  FFIType::i64(),
  i64,
  "int64",
  bigint_to_i64_arg,
  callback_to_i64,
  i64_to_js
);
numeric_target!(
  U64Target,
  FFIType::u64(),
  u64,
  "uint64",
  bigint_to_u64_arg,
  callback_to_u64,
  u64_to_js
);
numeric_target!(
  F32Target,
  FFIType::f32(),
  f32,
  "float32",
  number_to_f32,
  callback_to_f32,
  f32_to_js
);
numeric_target!(
  F64Target,
  FFIType::f64(),
  f64,
  "float64",
  number_to_f64,
  callback_to_f64,
  f64_to_js
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
