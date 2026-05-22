use napi::bindgen_prelude::*;
use napi::{Env, Unknown};

use crate::errors::{throw_coded_error, JsErrorKind};
use crate::types::{get_validated_pointer, validate_pointer_span};

pub fn read_number(value: &Unknown) -> Result<f64> {
  let mut out = 0.0;
  check_status!(unsafe {
    napi::sys::napi_get_value_double(value.value().env, value.raw(), &mut out)
  })?;
  Ok(out)
}

fn invalid_arg(error: impl Into<String>) -> Error {
  Error::new(Status::InvalidArg, error.into())
}

fn ensure_type(value: &Unknown, expected: ValueType, error: impl Into<String>) -> Result<()> {
  if value.get_type()? != expected {
    return Err(invalid_arg(error));
  }
  Ok(())
}

fn read_required_number(value: &Unknown, error: impl Into<String>) -> Result<f64> {
  ensure_type(value, ValueType::Number, error)?;
  read_number(value)
}

fn read_integer_in_range(
  value: &Unknown,
  min: f64,
  max: f64,
  type_error: impl Into<String>,
  range_error: impl Into<String>,
) -> Result<f64> {
  let number = read_required_number(value, type_error)?;
  validate_f64_integer_range(number, min, max, range_error)?;
  Ok(number)
}

pub fn expect_bigint(value: &Unknown, label: &str) -> Result<BigInt> {
  ensure_type(
    value,
    ValueType::BigInt,
    format!("The {label} must be a bigint"),
  )?;
  unsafe { value.cast() }
}

pub fn expect_string(env: &Env, value: &Unknown, label: &str) -> Result<String> {
  if let Err(error) = ensure_type(
    value,
    ValueType::String,
    format!("The {label} must be a string"),
  ) {
    return throw_coded_error(
      env,
      JsErrorKind::TypeError,
      "ERR_INVALID_ARG_TYPE",
      error.reason.clone(),
    );
  }
  unsafe { value.cast::<String>() }
}

pub fn validated_size(value: &Unknown, label: &str) -> Result<usize> {
  let number = read_integer_in_range(
    value,
    0.0,
    usize::MAX as f64,
    format!("The {label} must be a number"),
    format!("The {label} must be a non-negative integer"),
  )?;
  Ok(number as usize)
}

pub fn validated_size_with_code(env: &Env, value: &Unknown, label: &str) -> Result<usize> {
  let type_error = format!("The {label} must be a number");
  let range_error = format!("The {label} must be a non-negative integer");
  if let Err(error) = ensure_type(value, ValueType::Number, type_error) {
    return throw_coded_error(
      env,
      JsErrorKind::TypeError,
      "ERR_INVALID_ARG_TYPE",
      error.reason.clone(),
    );
  }

  let number = read_number(value)?;
  if validate_f64_integer_range(number, 0.0, usize::MAX as f64, &range_error).is_err() {
    return throw_coded_error(
      env,
      JsErrorKind::RangeError,
      "ERR_OUT_OF_RANGE",
      range_error,
    );
  }

  Ok(number as usize)
}

pub fn validated_pointer(value: &BigInt, label: &str) -> Result<usize> {
  get_validated_pointer(value, label)
}

pub fn validated_pointer_from_unknown(value: &Unknown, label: &str) -> Result<usize> {
  let value = expect_bigint(value, label)?;
  validated_pointer(&value, label)
}

pub fn validated_pointer_and_offset(
  ptr: &BigInt,
  offset: Option<i64>,
  access_len: usize,
  span_message: &str,
) -> Result<*mut u8> {
  let addr = validated_pointer(ptr, "pointer")?;
  if addr == 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "Cannot dereference a null pointer".to_string(),
    ));
  }

  let offset = offset.unwrap_or(0);
  if offset < 0 {
    return Err(Error::new(
      Status::InvalidArg,
      "The offset must be a number".to_string(),
    ));
  }
  let offset = usize::try_from(offset)
    .map_err(|_| Error::new(Status::InvalidArg, span_message.to_string()))?;
  validate_pointer_span_with_message(addr, offset, access_len, span_message)?;
  let target = addr
    .checked_add(offset)
    .ok_or_else(|| Error::new(Status::InvalidArg, span_message.to_string()))?;
  Ok(target as *mut u8)
}

pub fn validated_required_offset(value: &Unknown) -> Result<i64> {
  match value.get_type()? {
    ValueType::Undefined => return Err(invalid_arg("Expected an offset argument")),
    ValueType::Number => {}
    _ => return Err(invalid_arg("The offset must be a number")),
  }

  let offset = read_integer_in_range(
    value,
    0.0,
    i64::MAX as f64,
    "The offset must be a number",
    "The offset must be a number",
  )?;
  Ok(offset as i64)
}

pub fn ensure_value_present(value: &Unknown) -> Result<()> {
  if value.get_type()? == ValueType::Undefined {
    return Err(Error::new(
      Status::InvalidArg,
      "Expected a value argument".to_string(),
    ));
  }
  Ok(())
}

pub fn validated_signed_int(value: &Unknown, min: i128, max: i128, label: &str) -> Result<i128> {
  let error = format!("Value must be {label}");
  let value = read_integer_in_range(value, min as f64, max as f64, error.clone(), error)?;
  Ok(value as i128)
}

pub fn validated_float(value: &Unknown, label: &str) -> Result<f64> {
  read_required_number(value, format!("Value must be {label}"))
}

pub fn expect_bigint_value(value: &Unknown, label: &str) -> Result<BigInt> {
  ensure_type(value, ValueType::BigInt, format!("Value must be {label}"))?;
  unsafe { value.cast() }
}

pub(crate) fn validate_f64_integer_range(
  value: f64,
  min: f64,
  max: f64,
  error: impl Into<String>,
) -> Result<()> {
  if !value.is_finite() || value.fract() != 0.0 || value < min || value > max {
    return Err(Error::new(Status::InvalidArg, error.into()));
  }
  Ok(())
}

pub fn validate_pointer_span_with_message(
  ptr: usize,
  offset: usize,
  length: usize,
  message: &str,
) -> Result<()> {
  validate_pointer_span(ptr, offset, length).map_err(|error| {
    let reason = error.reason.clone();
    if reason.contains("pointer and offset") {
      Error::new(Status::InvalidArg, reason)
    } else if reason.contains("platform address range") {
      Error::new(Status::InvalidArg, message.to_string())
    } else {
      Error::new(Status::InvalidArg, reason)
    }
  })
}
