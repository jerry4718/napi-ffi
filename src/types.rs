use libffi::middle::Type;
use napi::bindgen_prelude::*;

use crate::args::{expect_bigint, read_number, validate_f64_integer_range};
use crate::buffer_helpers::get_buffer_like_pointer_and_len;
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

fn read_number_arg(value: &Unknown, error: impl Into<String>) -> Result<f64> {
  if value.get_type()? != ValueType::Number {
    return Err(Error::new(Status::InvalidArg, error.into()));
  }
  read_number(value)
}

fn read_bigint_arg(value: &Unknown, error: impl Into<String>) -> Result<BigInt> {
  if value.get_type()? != ValueType::BigInt {
    return Err(Error::new(Status::InvalidArg, error.into()));
  }
  unsafe { value.cast() }
}

#[derive(Clone, Copy)]
enum NumberArgRange {
  Signed { min: i64, max: i64 },
  Unsigned { max: u64 },
}

impl NumberArgRange {
  fn validate(self, value: f64, error: impl Into<String>) -> Result<FFIStorage> {
    match self {
      NumberArgRange::Signed { min, max } => {
        Ok(validate_signed_int(value, min, max, error)? as FFIStorage)
      }
      NumberArgRange::Unsigned { max } => {
        Ok(validate_unsigned_int(value, max, error)? as FFIStorage)
      }
    }
  }
}

fn marshal_number_arg(
  arg: &Unknown,
  index: usize,
  label: &str,
  range: NumberArgRange,
  storage: &mut FFIStorage,
) -> Result<Option<String>> {
  let error = format!("Argument {index} must be {label}");
  let value = read_number_arg(arg, error.clone())?;
  *storage = range.validate(value, error)?;
  Ok(None)
}

/// Validate a JS Number argument to a signed integer in range [min, max].
pub fn validate_signed_int(n: f64, min: i64, max: i64, error: impl Into<String>) -> Result<i64> {
  validate_f64_integer_range(n, min as f64, max as f64, error)?;
  Ok(n as i64)
}

/// Validate a JS Number argument to an unsigned integer in range [0, max].
pub fn validate_unsigned_int(n: f64, max: u64, error: impl Into<String>) -> Result<u64> {
  validate_f64_integer_range(n, 0.0, max as f64, error)?;
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
    FFIType::Sint8 => marshal_number_arg(
      arg,
      index,
      "an int8",
      NumberArgRange::Signed {
        min: i8::MIN as i64,
        max: i8::MAX as i64,
      },
      storage,
    ),
    FFIType::Uint8 => marshal_number_arg(
      arg,
      index,
      "a uint8",
      NumberArgRange::Unsigned {
        max: u8::MAX as u64,
      },
      storage,
    ),
    FFIType::Sint16 => marshal_number_arg(
      arg,
      index,
      "an int16",
      NumberArgRange::Signed {
        min: i16::MIN as i64,
        max: i16::MAX as i64,
      },
      storage,
    ),
    FFIType::Uint16 => marshal_number_arg(
      arg,
      index,
      "a uint16",
      NumberArgRange::Unsigned {
        max: u16::MAX as u64,
      },
      storage,
    ),
    FFIType::Sint32 => marshal_number_arg(
      arg,
      index,
      "an int32",
      NumberArgRange::Signed {
        min: i32::MIN as i64,
        max: i32::MAX as i64,
      },
      storage,
    ),
    FFIType::Uint32 => marshal_number_arg(
      arg,
      index,
      "a uint32",
      NumberArgRange::Unsigned {
        max: u32::MAX as u64,
      },
      storage,
    ),
    FFIType::Sint64 => {
      let error = format!("Argument {index} must be an int64");
      let val = read_bigint_arg(arg, error.clone())?;
      let (value, lossless) = val.get_i64();
      if !lossless {
        return Err(Error::new(Status::InvalidArg, error));
      }
      *storage = value as FFIStorage;
      Ok(None)
    }
    FFIType::Uint64 => {
      let error = format!("Argument {index} must be a uint64");
      let val = read_bigint_arg(arg, error.clone())?;
      let (_signed, value, lossless) = val.get_u64();
      if !lossless {
        return Err(Error::new(Status::InvalidArg, error));
      }
      *storage = value;
      Ok(None)
    }
    FFIType::Float => {
      let n = read_number_arg(arg, format!("Argument {index} must be a float"))? as f32;
      *storage = n.to_bits() as FFIStorage;
      Ok(None)
    }
    FFIType::Double => {
      let n = read_number_arg(arg, format!("Argument {index} must be a double"))?;
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
        let val = expect_bigint(arg, &format!("argument {index}"))?;
        *storage = get_validated_pointer_with_error(
          &val,
          format!("Argument {index} must be a non-negative pointer bigint"),
        )? as FFIStorage;
        return Ok(None);
      }
      if let Some((ptr, _)) = get_buffer_like_pointer_and_len(arg) {
        *storage = ptr as FFIStorage;
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
    FFIType::Sint8 => (read_return_sarg(storage) as i8 as i32).into_unknown(env),
    FFIType::Uint8 => (read_return_arg(storage) as u8 as u32).into_unknown(env),
    FFIType::Sint16 => (read_return_sarg(storage) as i16 as i32).into_unknown(env),
    FFIType::Uint16 => (read_return_arg(storage) as u16 as u32).into_unknown(env),
    FFIType::Sint32 => (read_return_sarg(storage) as i32).into_unknown(env),
    FFIType::Uint32 => (read_return_arg(storage) as u32).into_unknown(env),
    FFIType::Sint64 => BigInt::from(read_return_i64(storage)).into_unknown(env),
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

fn read_return_sarg(storage: &FFIStorage) -> libffi::low::ffi_sarg {
  // SAFETY: libffi wrote the return value into storage through call_return_into. For promoted
  // small signed integer returns, libffi stores ffi_sarg-compatible bits in that slot.
  unsafe { *(storage as *const FFIStorage as *const libffi::low::ffi_sarg) }
}

fn read_return_arg(storage: &FFIStorage) -> libffi::low::ffi_arg {
  // SAFETY: libffi wrote the return value into storage through call_return_into. For promoted
  // small unsigned integer returns, libffi stores ffi_arg-compatible bits in that slot.
  unsafe { *(storage as *const FFIStorage as *const libffi::low::ffi_arg) }
}

fn read_return_i64(storage: &FFIStorage) -> i64 {
  // SAFETY: libffi wrote an i64 return value into the storage slot for Sint64 signatures.
  unsafe { *(storage as *const FFIStorage as *const i64) }
}

/// Convert a BigInt to a validated pointer address.
pub fn get_validated_pointer(value: &BigInt, label: &str) -> Result<usize> {
  get_validated_pointer_with_error(value, format!("The {label} must be a non-negative bigint"))
    .map_err(|error| {
      if error.reason.contains("platform pointer range") {
        Error::new(
          Status::InvalidArg,
          format!("The {label} exceeds the platform pointer range"),
        )
      } else {
        error
      }
    })
}

pub fn get_validated_pointer_with_error(value: &BigInt, error: impl Into<String>) -> Result<usize> {
  let error = error.into();
  let (signed, addr, lossless) = value.get_u64();
  if signed || !lossless {
    return Err(Error::new(Status::InvalidArg, error));
  }
  if addr > usize::MAX as u64 {
    return Err(Error::new(
      Status::InvalidArg,
      "The pointer exceeds the platform pointer range".to_string(),
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
  if length > 0 && length > usize::MAX - start {
    return Err(Error::new(
      Status::InvalidArg,
      "The accessed range exceeds the platform address range".to_string(),
    ));
  }
  Ok(())
}
