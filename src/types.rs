use std::ffi::c_void;

use libffi::middle::Type;
use napi::bindgen_prelude::*;
/// Represents the complete set of C types supported by node:ffi.
/// Maps to libffi::middle::Type for CIF creation and holds metadata for JS↔C marshalling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FFIType {
  Void,
  Sint8,
  Uint8,
  Sint16,
  Uint16,
  Sint32,
  Uint32,
  Sint64,
  Uint64,
  Float,
  Double,
  Pointer,
}

impl FFIType {
  /// Parse a type string per node:ffi's `ToFFIType` rules.
  pub fn from_str(s: &str) -> Result<Self> {
    match s {
      "void" => Ok(FFIType::Void),
      "i8" | "int8" => Ok(FFIType::Sint8),
      "u8" | "uint8" | "bool" => Ok(FFIType::Uint8),
      "char" => {
        if std::os::raw::c_char::MIN < 0 {
          Ok(FFIType::Sint8)
        } else {
          Ok(FFIType::Uint8)
        }
      }
      "i16" | "int16" => Ok(FFIType::Sint16),
      "u16" | "uint16" => Ok(FFIType::Uint16),
      "i32" | "int32" => Ok(FFIType::Sint32),
      "u32" | "uint32" => Ok(FFIType::Uint32),
      "i64" | "int64" => Ok(FFIType::Sint64),
      "u64" | "uint64" => Ok(FFIType::Uint64),
      "f32" | "float" | "float32" => Ok(FFIType::Float),
      "f64" | "double" | "float64" => Ok(FFIType::Double),
      "buffer" | "arraybuffer" | "string" | "str" | "pointer" | "ptr" | "function" => {
        Ok(FFIType::Pointer)
      }
      _ => Err(Error::new(
        Status::InvalidArg,
        format!("Unsupported FFI type: {s}"),
      )),
    }
  }

  /// Convert to libffi::middle::Type for CIF construction.
  pub fn middle_type(self) -> Type {
    match self {
      FFIType::Void => Type::void(),
      FFIType::Sint8 => Type::i8(),
      FFIType::Uint8 => Type::u8(),
      FFIType::Sint16 => Type::i16(),
      FFIType::Uint16 => Type::u16(),
      FFIType::Sint32 => Type::i32(),
      FFIType::Uint32 => Type::u32(),
      FFIType::Sint64 => Type::i64(),
      FFIType::Uint64 => Type::u64(),
      FFIType::Float => Type::f32(),
      FFIType::Double => Type::f64(),
      FFIType::Pointer => Type::pointer(),
    }
  }
}

/// Return value storage that can hold any FFI type.
pub type FFIStorage = u64;

fn read_number(value: &Unknown, error: impl Into<String>) -> Result<f64> {
  if value.get_type()? != ValueType::Number {
    return Err(Error::new(Status::InvalidArg, error.into()));
  }

  let mut out = 0.0;
  check_status!(unsafe {
    napi::sys::napi_get_value_double(value.value().env, value.raw(), &mut out)
  })?;
  Ok(out)
}

/// Validate a JS Number argument to a signed integer in range [min, max].
pub fn validate_signed_int(n: f64, min: i64, max: i64, type_name: &str) -> Result<i64> {
  if !n.is_finite() || n.fract() != 0.0 || n < (min as f64) || n > (max as f64) {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Value must be an {type_name}"),
    ));
  }
  Ok(n as i64)
}

/// Validate a JS Number argument to an unsigned integer in range [0, max].
pub fn validate_unsigned_int(n: f64, max: u64, type_name: &str) -> Result<u64> {
  if !n.is_finite() || n.fract() != 0.0 || n < 0.0 || n > (max as f64) {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Value must be a {type_name}"),
    ));
  }
  Ok(n as u64)
}

/// Marshal a JS value to a C-compatible u64 storage slot.
/// Returns Some(string) for String pointer args (caller must CString it).
pub fn marshal_js_to_c(
  _env: &Env,
  arg: &Unknown,
  ffitype: FFIType,
  index: usize,
  storage: &mut FFIStorage,
) -> Result<Option<String>> {
  match ffitype {
    FFIType::Void => {
      *storage = 0;
      Ok(None)
    }
    FFIType::Sint8 => {
      let val = read_number(arg, format!("Argument {index} must be an int8"))?;
      let n = validate_signed_int(val, i8::MIN as i64, i8::MAX as i64, "int8")?;
      *storage = (n as i8) as FFIStorage;
      Ok(None)
    }
    FFIType::Uint8 => {
      let val = read_number(arg, format!("Argument {index} must be a uint8"))?;
      let n = validate_unsigned_int(val, u8::MAX as u64, "uint8")?;
      *storage = (n as u8) as FFIStorage;
      Ok(None)
    }
    FFIType::Sint16 => {
      let val = read_number(arg, format!("Argument {index} must be an int16"))?;
      let n = validate_signed_int(val, i16::MIN as i64, i16::MAX as i64, "int16")?;
      *storage = (n as i16) as FFIStorage;
      Ok(None)
    }
    FFIType::Uint16 => {
      let val = read_number(arg, format!("Argument {index} must be a uint16"))?;
      let n = validate_unsigned_int(val, u16::MAX as u64, "uint16")?;
      *storage = (n as u16) as FFIStorage;
      Ok(None)
    }
    FFIType::Sint32 => {
      let val = read_number(arg, format!("Argument {index} must be an int32"))?;
      let n = validate_signed_int(val, i32::MIN as i64, i32::MAX as i64, "int32")?;
      *storage = (n as i32) as FFIStorage;
      Ok(None)
    }
    FFIType::Uint32 => {
      let val = read_number(arg, format!("Argument {index} must be a uint32"))?;
      let n = validate_unsigned_int(val, u32::MAX as u64, "uint32")?;
      *storage = (n as u32) as FFIStorage;
      Ok(None)
    }
    FFIType::Sint64 => {
      let val: BigInt = unsafe { arg.cast() }.map_err(|_| {
        Error::new(
          Status::InvalidArg,
          format!("Argument {index} must be an int64 (bigint)"),
        )
      })?;
      let (value, lossless) = val.get_i64();
      if !lossless {
        return Err(Error::new(
          Status::InvalidArg,
          format!("Argument {index} must be an int64"),
        ));
      }
      *storage = value as FFIStorage;
      Ok(None)
    }
    FFIType::Uint64 => {
      let val: BigInt = unsafe { arg.cast() }.map_err(|_| {
        Error::new(
          Status::InvalidArg,
          format!("Argument {index} must be a uint64 (bigint)"),
        )
      })?;
      let (_signed, value, lossless) = val.get_u64();
      if !lossless {
        return Err(Error::new(
          Status::InvalidArg,
          format!("Argument {index} must be a uint64"),
        ));
      }
      *storage = value;
      Ok(None)
    }
    FFIType::Float => {
      let n = read_number(arg, format!("Argument {index} must be a float"))? as f32;
      *storage = n.to_bits() as FFIStorage;
      Ok(None)
    }
    FFIType::Double => {
      let n = read_number(arg, format!("Argument {index} must be a double"))?;
      *storage = n.to_bits();
      Ok(None)
    }
    FFIType::Pointer => {
      if arg.get_type()? == ValueType::Null || arg.get_type()? == ValueType::Undefined {
        *storage = 0;
        return Ok(None);
      }
      if arg.get_type()? == ValueType::String {
        let s: String = arg.coerce_to_string()?.into_utf8()?.as_str()?.to_owned();
        return Ok(Some(s));
      }
      if arg.get_type()? == ValueType::BigInt {
        let val: BigInt = unsafe { arg.cast() }.map_err(|_| {
          Error::new(
            Status::InvalidArg,
            format!("Argument {index} must be a non-negative pointer bigint"),
          )
        })?;
        let (signed, value, lossless) = val.get_u64();
        if signed || !lossless || value > usize::MAX as u64 {
          return Err(Error::new(
            Status::InvalidArg,
            format!("Argument {index} must be a non-negative pointer bigint"),
          ));
        }
        *storage = value;
        return Ok(None);
      }
      if is_buffer_or_typedarray(arg) {
        let ptr = get_pointer_from_arg(arg)?;
        *storage = ptr;
        return Ok(None);
      }
      Err(Error::new(
        Status::InvalidArg,
        format!("Argument {index} must be a buffer, an ArrayBuffer, a string, or a bigint"),
      ))
    }
  }
}

/// Marshal a C return value in `storage` to a JS value.
pub fn marshal_c_to_js<'env>(
  env: &'env Env,
  storage: &FFIStorage,
  ffitype: FFIType,
) -> Result<Unknown<'env>> {
  match ffitype {
    FFIType::Void => ().into_unknown(env),
    FFIType::Sint8 => {
      let val = unsafe { *(storage as *const FFIStorage as *const libffi::low::ffi_sarg) };
      (val as i8 as i32).into_unknown(env)
    }
    FFIType::Uint8 => {
      let val = unsafe { *(storage as *const FFIStorage as *const libffi::low::ffi_arg) };
      (val as u8 as u32).into_unknown(env)
    }
    FFIType::Sint16 => {
      let val = unsafe { *(storage as *const FFIStorage as *const libffi::low::ffi_sarg) };
      (val as i16 as i32).into_unknown(env)
    }
    FFIType::Uint16 => {
      let val = unsafe { *(storage as *const FFIStorage as *const libffi::low::ffi_arg) };
      (val as u16 as u32).into_unknown(env)
    }
    FFIType::Sint32 => {
      let val = unsafe { *(storage as *const FFIStorage as *const libffi::low::ffi_sarg) };
      (val as i32).into_unknown(env)
    }
    FFIType::Uint32 => {
      let val = unsafe { *(storage as *const FFIStorage as *const libffi::low::ffi_arg) };
      (val as u32).into_unknown(env)
    }
    FFIType::Sint64 => {
      let val = unsafe { *(storage as *const FFIStorage as *const i64) };
      BigInt::from(val).into_unknown(env)
    }
    FFIType::Uint64 => BigInt::from(*storage).into_unknown(env),
    FFIType::Float => {
      let bits = *storage as u32;
      let val = f32::from_bits(bits);
      (val as f64).into_unknown(env)
    }
    FFIType::Double => {
      let val = f64::from_bits(*storage);
      val.into_unknown(env)
    }
    FFIType::Pointer => {
      let ptr = *storage;
      BigInt::from(ptr).into_unknown(env)
    }
  }
}

/// Check if argument is a buffer-like type.
pub fn is_buffer_or_typedarray(arg: &Unknown) -> bool {
  let arg_type = match arg.get_type() {
    Ok(t) => t,
    Err(_) => return false,
  };
  arg_type == ValueType::Object
}

/// Get raw pointer from buffer-like argument.
pub fn get_pointer_from_arg(arg: &Unknown) -> Result<u64> {
  if arg.is_buffer()? {
    let mut data = std::ptr::null_mut::<c_void>();
    let mut len = 0usize;
    napi::check_status!(unsafe {
      napi::sys::napi_get_buffer_info(arg.value().env, arg.raw(), &mut data, &mut len)
    })?;
    Ok(data as u64)
  } else if arg.is_arraybuffer()? {
    let mut data = std::ptr::null_mut::<c_void>();
    let mut len = 0usize;
    napi::check_status!(unsafe {
      napi::sys::napi_get_arraybuffer_info(arg.value().env, arg.raw(), &mut data, &mut len)
    })?;
    Ok(data as u64)
  } else if arg.is_typedarray()? {
    let mut typedarray_type = 0;
    let mut len = 0usize;
    let mut data = std::ptr::null_mut::<c_void>();
    let mut arraybuffer = std::ptr::null_mut();
    let mut byte_offset = 0usize;
    napi::check_status!(unsafe {
      napi::sys::napi_get_typedarray_info(
        arg.value().env,
        arg.raw(),
        &mut typedarray_type,
        &mut len,
        &mut data,
        &mut arraybuffer,
        &mut byte_offset,
      )
    })?;
    Ok(data as u64)
  } else {
    Err(Error::new(
      Status::InvalidArg,
      "Expected Buffer, ArrayBuffer, or TypedArray".to_string(),
    ))
  }
}

/// Convert a BigInt to a validated pointer address.
pub fn get_validated_pointer(value: &BigInt, label: &str) -> Result<usize> {
  let (signed, addr, lossless) = value.get_u64();
  if signed || !lossless {
    return Err(Error::new(
      Status::InvalidArg,
      format!("The {label} must be a non-negative bigint"),
    ));
  }
  if addr > usize::MAX as u64 {
    return Err(Error::new(
      Status::InvalidArg,
      format!("The {label} exceeds the platform pointer range"),
    ));
  }
  Ok(addr as usize)
}

/// Validate that a pointer span (ptr + offset, length) does not overflow.
pub fn validate_pointer_span(ptr: usize, offset: usize, length: usize) -> Result<()> {
  if offset > usize::MAX - ptr {
    return Err(Error::new(
      Status::InvalidArg,
      "The pointer and offset exceed the platform address range".to_string(),
    ));
  }
  let start = ptr + offset;
  if length > 0 && length - 1 > usize::MAX - start {
    return Err(Error::new(
      Status::InvalidArg,
      "The pointer and length exceed the platform address range".to_string(),
    ));
  }
  Ok(())
}
