use std::ffi::{c_char, c_void, CString};

use libffi::middle::{Arg, Cif, CodePtr, Type};
use napi::bindgen_prelude::*;

pub trait FfiTarget {
  fn type_name(&self) -> &'static str;
  fn ffi_type(&self) -> Type;
  fn js_to_ffi(&self, value: Unknown<'_>, index: usize) -> Result<PreparedArg>;
  fn ffi_to_js<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>>;
}

#[derive(Clone)]
pub enum PreparedArgStorage {
  I8(i8),
  U8(u8),
  I16(i16),
  U16(u16),
  I32(i32),
  U32(u32),
  I64(i64),
  U64(u64),
  F32(f32),
  F64(f64),
  Pointer(*mut c_void),
  CString(CString),
  CStringPtr(*const c_char),
}

pub struct PreparedArg {
  pub storage: PreparedArgStorage,
  pub auxiliary: Option<PreparedArgStorage>,
}

impl PreparedArg {
  pub fn as_arg(&self) -> Arg<'_> {
    match &self.storage {
      PreparedArgStorage::I8(value) => Arg::new(value),
      PreparedArgStorage::U8(value) => Arg::new(value),
      PreparedArgStorage::I16(value) => Arg::new(value),
      PreparedArgStorage::U16(value) => Arg::new(value),
      PreparedArgStorage::I32(value) => Arg::new(value),
      PreparedArgStorage::U32(value) => Arg::new(value),
      PreparedArgStorage::I64(value) => Arg::new(value),
      PreparedArgStorage::U64(value) => Arg::new(value),
      PreparedArgStorage::F32(value) => Arg::new(value),
      PreparedArgStorage::F64(value) => Arg::new(value),
      PreparedArgStorage::Pointer(value) => Arg::new(value),
      PreparedArgStorage::CStringPtr(value) => Arg::new(value),
      PreparedArgStorage::CString(_) => {
        unreachable!("CString storage must use CStringPtr as primary storage")
      }
    }
  }
}

fn prepared(storage: PreparedArgStorage) -> PreparedArg {
  PreparedArg {
    storage,
    auxiliary: None,
  }
}

fn prepared_with_auxiliary(storage: PreparedArgStorage, auxiliary: PreparedArgStorage) -> PreparedArg {
  PreparedArg {
    storage,
    auxiliary: Some(auxiliary),
  }
}

fn prepared_c_string(string: CString) -> PreparedArg {
  let ptr = string.as_ptr();
  prepared_with_auxiliary(PreparedArgStorage::CStringPtr(ptr), PreparedArgStorage::CString(string))
}

fn invalid_arg_value(message: impl Into<String>) -> Error {
  Error::new(Status::InvalidArg, message.into())
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

fn raw_pointer_from_unknown(
  value: Unknown<'_>,
  index: usize,
  allow_string: bool,
) -> Result<*mut c_void> {
  match value.get_type()? {
    ValueType::Null | ValueType::Undefined => Ok(std::ptr::null_mut()),
    ValueType::BigInt => {
      let bigint: BigInt = unsafe { value.cast()? };
      let raw = bigint_to_u64(
        &bigint,
        &format!("Argument {index} must be a non-negative pointer bigint"),
      )?;
      let ptr = usize::try_from(raw)
        .map_err(|_| invalid_arg_value("Argument exceeds the platform pointer range"))?;
      Ok(ptr as *mut c_void)
    }
    ValueType::Object => {
      if let Ok(buffer) = unsafe { value.cast::<Buffer>() } {
        return Ok(buffer.as_ref().as_ptr() as *mut c_void);
      }
      if let Ok(arraybuffer) = unsafe { value.cast::<ArrayBuffer>() } {
        return Ok(arraybuffer.as_ref().as_ptr() as *mut c_void);
      }
      if let Ok(typed) = unsafe { value.cast::<TypedArray>() } {
        return Ok(
          typed
            .arraybuffer
            .as_ref()
            .as_ptr()
            .wrapping_add(typed.byte_offset) as *mut c_void,
        );
      }
      Err(invalid_arg_value(
        "Argument must be a buffer, an ArrayBuffer, a string, or a bigint",
      ))
    }
    ValueType::String if allow_string => Err(invalid_arg_value(format!(
      "Argument {index} must be passed through a string-aware target"
    ))),
    _ => Err(invalid_arg_value(
      "Argument must be a buffer, an ArrayBuffer, a string, or a bigint",
    )),
  }
}

macro_rules! numeric_target {
  ($name:ident, $ffi_ty:expr, $rust_ty:ty, $label:expr, $js_to_ffi:expr, $ffi_to_js:expr) => {
    pub struct $name;

    impl FfiTarget for $name {
      fn type_name(&self) -> &'static str {
        $label
      }
      fn ffi_type(&self) -> Type {
        $ffi_ty
      }
      fn js_to_ffi(&self, value: Unknown<'_>, index: usize) -> Result<PreparedArg> {
        $js_to_ffi(value, index)
      }
      fn ffi_to_js<'env>(
        &self,
        env: &'env Env,
        cif: &Cif,
        fn_ptr: CodePtr,
        args: &[Arg<'_>],
      ) -> Result<Unknown<'env>> {
        let value: $rust_ty = unsafe { cif.call(fn_ptr, args) };
        $ffi_to_js(env, value)
      }
    }
  };
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
  Type::i8(),
  i8,
  "int8",
  |value: Unknown<'_>, index| {
    let number: f64 = unsafe { value.cast()? };
    if number.fract() != 0.0 || !(-128.0..=127.0).contains(&number) {
      return Err(invalid_arg_value(format!(
        "Argument {index} must be an int8"
      )));
    }
    Ok(prepared(PreparedArgStorage::I8(number as i8)))
  },
  i8_to_js
);

numeric_target!(
  U8Target,
  Type::u8(),
  u8,
  "uint8",
  |value: Unknown<'_>, index| {
    let number: f64 = unsafe { value.cast()? };
    if number.fract() != 0.0 || !(0.0..=255.0).contains(&number) {
      return Err(invalid_arg_value(format!(
        "Argument {index} must be a uint8"
      )));
    }
    Ok(prepared(PreparedArgStorage::U8(number as u8)))
  },
  u8_to_js
);

numeric_target!(
  I16Target,
  Type::i16(),
  i16,
  "int16",
  |value: Unknown<'_>, index| {
    let number: f64 = unsafe { value.cast()? };
    if number.fract() != 0.0 || !(-32768.0..=32767.0).contains(&number) {
      return Err(invalid_arg_value(format!(
        "Argument {index} must be an int16"
      )));
    }
    Ok(prepared(PreparedArgStorage::I16(number as i16)))
  },
  i16_to_js
);

numeric_target!(
  U16Target,
  Type::u16(),
  u16,
  "uint16",
  |value: Unknown<'_>, index| {
    let number: f64 = unsafe { value.cast()? };
    if number.fract() != 0.0 || !(0.0..=65535.0).contains(&number) {
      return Err(invalid_arg_value(format!(
        "Argument {index} must be a uint16"
      )));
    }
    Ok(prepared(PreparedArgStorage::U16(number as u16)))
  },
  u16_to_js
);

numeric_target!(
  I32Target,
  Type::i32(),
  i32,
  "int32",
  |value: Unknown<'_>, index| {
    let number: f64 = unsafe { value.cast()? };
    if !number.is_finite()
      || number.fract() != 0.0
      || !((i32::MIN as f64)..=(i32::MAX as f64)).contains(&number)
    {
      return Err(invalid_arg_value(format!(
        "Argument {index} must be an int32"
      )));
    }
    Ok(prepared(PreparedArgStorage::I32(number as i32)))
  },
  i32_to_js
);

numeric_target!(
  U32Target,
  Type::u32(),
  u32,
  "uint32",
  |value: Unknown<'_>, index| {
    let number: f64 = unsafe { value.cast()? };
    if !number.is_finite() || number.fract() != 0.0 || !(0.0..=(u32::MAX as f64)).contains(&number)
    {
      return Err(invalid_arg_value(format!(
        "Argument {index} must be a uint32"
      )));
    }
    Ok(prepared(PreparedArgStorage::U32(number as u32)))
  },
  u32_to_js
);

numeric_target!(
  I64Target,
  Type::i64(),
  i64,
  "int64",
  |value: Unknown<'_>, index| {
    let bigint: BigInt = unsafe { value.cast()? };
    Ok(prepared(PreparedArgStorage::I64(bigint_to_i64(
      &bigint,
      &format!("Argument {index} must be an int64"),
    )?)))
  },
  i64_to_js
);

numeric_target!(
  U64Target,
  Type::u64(),
  u64,
  "uint64",
  |value: Unknown<'_>, index| {
    let bigint: BigInt = unsafe { value.cast()? };
    Ok(prepared(PreparedArgStorage::U64(bigint_to_u64(
      &bigint,
      &format!("Argument {index} must be a uint64"),
    )?)))
  },
  u64_to_js
);

numeric_target!(
  F32Target,
  Type::f32(),
  f32,
  "float32",
  |value: Unknown<'_>, index| {
    let number: f64 = unsafe { value.cast()? };
    if !number.is_finite() {
      return Err(invalid_arg_value(format!(
        "Argument {index} must be a float"
      )));
    }
    Ok(prepared(PreparedArgStorage::F32(number as f32)))
  },
  f32_to_js
);

numeric_target!(
  F64Target,
  Type::f64(),
  f64,
  "float64",
  |value: Unknown<'_>, index| {
    let number: f64 = unsafe { value.cast()? };
    if !number.is_finite() {
      return Err(invalid_arg_value(format!(
        "Argument {index} must be a double"
      )));
    }
    Ok(prepared(PreparedArgStorage::F64(number)))
  },
  f64_to_js
);

pub struct PointerTarget;
pub struct StringTarget;
pub struct BufferTarget;
pub struct ArrayBufferTarget;
pub struct FunctionTarget;

impl FfiTarget for PointerTarget {
  fn type_name(&self) -> &'static str {
    "pointer"
  }
  fn ffi_type(&self) -> Type {
    Type::pointer()
  }
  fn js_to_ffi(&self, value: Unknown<'_>, index: usize) -> Result<PreparedArg> {
    match value.get_type()? {
      ValueType::String => {
        let string: String = unsafe { value.cast()? };
        let c_string = CString::new(string).map_err(|_| {
          invalid_arg_value(format!("Argument {index} must not contain null bytes"))
        })?;
        Ok(prepared_c_string(c_string))
      }
      _ => Ok(prepared(PreparedArgStorage::Pointer(
        raw_pointer_from_unknown(value, index, false)?,
      ))),
    }
  }
  fn ffi_to_js<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>> {
    let value: u64 = unsafe { cif.call(fn_ptr, args) };
    BigInt::from(value).into_unknown(env)
  }
}

impl FfiTarget for StringTarget {
  fn type_name(&self) -> &'static str {
    "string"
  }
  fn ffi_type(&self) -> Type {
    Type::pointer()
  }
  fn js_to_ffi(&self, value: Unknown<'_>, index: usize) -> Result<PreparedArg> {
    match value.get_type()? {
      ValueType::String => {
        let string: String = unsafe { value.cast()? };
        let c_string = CString::new(string).map_err(|_| {
          invalid_arg_value(format!("Argument {index} must not contain null bytes"))
        })?;
        Ok(prepared_c_string(c_string))
      }
      _ => Ok(prepared(PreparedArgStorage::Pointer(
        raw_pointer_from_unknown(value, index, false)?,
      ))),
    }
  }
  fn ffi_to_js<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>> {
    let value: u64 = unsafe { cif.call(fn_ptr, args) };
    BigInt::from(value).into_unknown(env)
  }
}

impl FfiTarget for BufferTarget {
  fn type_name(&self) -> &'static str {
    "buffer"
  }
  fn ffi_type(&self) -> Type {
    Type::pointer()
  }
  fn js_to_ffi(&self, value: Unknown<'_>, index: usize) -> Result<PreparedArg> {
    Ok(prepared(PreparedArgStorage::Pointer(
      raw_pointer_from_unknown(value, index, false)?,
    )))
  }
  fn ffi_to_js<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>> {
    let value: u64 = unsafe { cif.call(fn_ptr, args) };
    BigInt::from(value).into_unknown(env)
  }
}

impl FfiTarget for ArrayBufferTarget {
  fn type_name(&self) -> &'static str {
    "arraybuffer"
  }
  fn ffi_type(&self) -> Type {
    Type::pointer()
  }
  fn js_to_ffi(&self, value: Unknown<'_>, index: usize) -> Result<PreparedArg> {
    Ok(prepared(PreparedArgStorage::Pointer(
      raw_pointer_from_unknown(value, index, false)?,
    )))
  }
  fn ffi_to_js<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>> {
    let value: u64 = unsafe { cif.call(fn_ptr, args) };
    BigInt::from(value).into_unknown(env)
  }
}

impl FfiTarget for FunctionTarget {
  fn type_name(&self) -> &'static str {
    "function"
  }
  fn ffi_type(&self) -> Type {
    Type::pointer()
  }
  fn js_to_ffi(&self, value: Unknown<'_>, index: usize) -> Result<PreparedArg> {
    Ok(prepared(PreparedArgStorage::Pointer(
      raw_pointer_from_unknown(value, index, false)?,
    )))
  }
  fn ffi_to_js<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>> {
    let value: u64 = unsafe { cif.call(fn_ptr, args) };
    BigInt::from(value).into_unknown(env)
  }
}

pub struct VoidTarget;

impl FfiTarget for VoidTarget {
  fn type_name(&self) -> &'static str {
    "void"
  }
  fn ffi_type(&self) -> Type {
    Type::void()
  }
  fn js_to_ffi(&self, _value: Unknown<'_>, index: usize) -> Result<PreparedArg> {
    Err(invalid_arg_value(format!(
      "Argument {index} cannot use void as an argument type"
    )))
  }
  fn ffi_to_js<'env>(
    &self,
    env: &'env Env,
    cif: &Cif,
    fn_ptr: CodePtr,
    args: &[Arg<'_>],
  ) -> Result<Unknown<'env>> {
    let _: () = unsafe { cif.call(fn_ptr, args) };
    ().into_unknown(env)
  }
}
