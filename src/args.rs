use napi::bindgen_prelude::*;
use napi::{Env, Unknown};

use crate::errors::{throw_coded_error, JsErrorKind};
use crate::types::{get_validated_pointer, validate_pointer_span};

pub fn read_number(value: &Unknown) -> Result<f64> {
  let mut out = 0.0;
  check_status!(unsafe { napi::sys::napi_get_value_double(value.value().env, value.raw(), &mut out) })?;
  Ok(out)
}

pub fn expect_bigint(value: &Unknown, label: &str) -> Result<BigInt> {
  if value.get_type()? != ValueType::BigInt {
    return Err(Error::new(
      Status::InvalidArg,
      format!("The {label} must be a bigint"),
    ));
  }
  unsafe { value.cast() }
}

pub fn expect_string(env: &Env, value: &Unknown, label: &str) -> Result<String> {
  if value.get_type()? != ValueType::String {
    return throw_coded_error(
      env,
      JsErrorKind::TypeError,
      "ERR_INVALID_ARG_TYPE",
      format!("The {label} must be a string"),
    );
  }
  unsafe { value.cast::<String>() }
}

pub fn validated_size(value: &Unknown, label: &str) -> Result<usize> {
  if value.get_type()? != ValueType::Number {
    return Err(Error::new(
      Status::InvalidArg,
      format!("The {label} must be a number"),
    ));
  }

  let number = read_number(value)?;
  if !number.is_finite() || number.fract() != 0.0 || number < 0.0 || number > usize::MAX as f64 {
    return Err(Error::new(
      Status::InvalidArg,
      format!("The {label} must be a non-negative integer"),
    ));
  }

  Ok(number as usize)
}

pub fn validated_size_with_code(env: &Env, value: &Unknown, label: &str) -> Result<usize> {
  if value.get_type()? != ValueType::Number {
    return throw_coded_error(
      env,
      JsErrorKind::TypeError,
      "ERR_INVALID_ARG_TYPE",
      format!("The {label} must be a number"),
    );
  }

  let number = read_number(value)?;
  if !number.is_finite() || number.fract() != 0.0 || number < 0.0 || number > usize::MAX as f64 {
    return throw_coded_error(
      env,
      JsErrorKind::RangeError,
      "ERR_OUT_OF_RANGE",
      format!("The {label} must be a non-negative integer"),
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

  let offset = offset.unwrap_or(0) as usize;
  validate_pointer_span_with_message(addr, offset, access_len, span_message)?;
  Ok((addr + offset) as *mut u8)
}

pub fn validated_required_offset(value: &Unknown) -> Result<i64> {
  match value.get_type()? {
    ValueType::Undefined => {
      return Err(Error::new(
        Status::InvalidArg,
        "Expected an offset argument".to_string(),
      ));
    }
    ValueType::Number => {}
    _ => {
      return Err(Error::new(
        Status::InvalidArg,
        "The offset must be a number".to_string(),
      ));
    }
  }

  let offset = read_number(value)?;
  if !offset.is_finite() || offset.fract() != 0.0 || offset < 0.0 || offset > i64::MAX as f64 {
    return Err(Error::new(
      Status::InvalidArg,
      "The offset must be a number".to_string(),
    ));
  }
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
  if value.get_type()? != ValueType::Number {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Value must be {label}"),
    ));
  }

  let value = read_number(value)?;
  if !value.is_finite() || value.fract() != 0.0 || value < min as f64 || value > max as f64 {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Value must be {label}"),
    ));
  }
  Ok(value as i128)
}

pub fn validated_float(value: &Unknown, label: &str) -> Result<f64> {
  if value.get_type()? != ValueType::Number {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Value must be {label}"),
    ));
  }
  Ok(read_number(value)?)
}

pub fn expect_bigint_value(value: &Unknown, label: &str) -> Result<BigInt> {
  if value.get_type()? != ValueType::BigInt {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Value must be {label}"),
    ));
  }
  unsafe { value.cast() }
}

pub fn validate_pointer_span_with_message(ptr: usize, offset: usize, length: usize, message: &str) -> Result<()> {
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
