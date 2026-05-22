use std::ffi::CString;

use napi::bindgen_prelude::*;

use crate::types::FFIType;

/// Parsed function signature from a JS object.
/// Supports node:ffi's flexible format with aliases:
///   result/return/returns → return type
///   parameters/arguments → argument types
pub struct ParsedSignature {
  pub return_type: FFIType,
  pub arg_types: Vec<FFIType>,
  pub return_type_name: String,
  pub arg_type_names: Vec<String>,
}

/// Parse a function signature from a JS object.
/// Mirrors node:ffi's `ParseFunctionSignature` in `src/ffi/types.cc`.
pub fn parse_function_signature(name: &str, sig: &Object) -> Result<ParsedSignature> {
  let has_returns = sig.get::<Unknown>("returns")?.is_some();
  let has_return = sig.get::<Unknown>("return")?.is_some();
  let has_result = sig.get::<Unknown>("result")?.is_some();
  let has_parameters = sig.get::<Unknown>("parameters")?.is_some();
  let has_arguments = sig.get::<Unknown>("arguments")?.is_some();

  // Validate: at most one of returns/return/result
  let return_key_count = has_returns as u8 + has_return as u8 + has_result as u8;
  if return_key_count > 1 {
    return Err(Error::new(
      Status::InvalidArg,
      format!(
        "Function signature of {name} must have either 'returns', 'return' or 'result' property"
      ),
    ));
  }

  // Validate: not both parameters and arguments
  if has_parameters && has_arguments {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Function signature of {name} must have either 'parameters' or 'arguments' property"),
    ));
  }

  // Parse return type
  let return_type_name = if has_returns || has_return || has_result {
    let key = if has_returns {
      "returns"
    } else if has_return {
      "return"
    } else {
      "result"
    };
    get_string_property(sig, key)?.ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        format!("Function signature of {name} must have a string return type"),
      )
    })?
  } else {
    "void".to_string()
  };
  let return_type = FFIType::from_str(&return_type_name)?;

  // Parse argument types

  let (arg_types, arg_type_names) = if has_arguments || has_parameters {
    let key = if has_arguments {
      "arguments"
    } else {
      "parameters"
    };
    let args_array = sig.get::<Array>(key)?.ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        format!("Arguments list of function {name} must be an array"),
      )
    })?;
    let len = args_array.len();
    let mut types = Vec::with_capacity(len as usize);
    let mut names = Vec::with_capacity(len as usize);
    for i in 0..len {
      let arg_str = get_array_string_element(&args_array, i, name)?;
      types.push(FFIType::from_str(&arg_str)?);
      names.push(arg_str);
    }
    (types, names)
  } else {
    (Vec::new(), Vec::new())
  };

  Ok(ParsedSignature {
    return_type,
    arg_types,
    return_type_name,
    arg_type_names,
  })
}

fn get_string_property(sig: &Object, key: &str) -> Result<Option<String>> {
  let Some(value) = sig.get::<Unknown>(key)? else {
    return Ok(None);
  };
  if value.get_type()? != ValueType::String {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Signature property '{key}' must be a string"),
    ));
  }
  let string = value.coerce_to_string()?.into_utf8()?.as_str()?.to_owned();
  reject_null_bytes(&string, &format!("Signature property '{key}'"))?;
  Ok(Some(string))
}

fn get_array_string_element(args_array: &Array, index: u32, name: &str) -> Result<String> {
  let value: Unknown = args_array.get_element(index)?;
  if value.get_type()? != ValueType::String {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Argument {index} of function {name} must be a string"),
    ));
  }
  let string = value.coerce_to_string()?.into_utf8()?.as_str()?.to_owned();
  reject_null_bytes(&string, &format!("Argument {index} of function {name}"))?;
  Ok(string)
}

fn reject_null_bytes(value: &str, label: &str) -> Result<()> {
  CString::new(value).map(|_| ()).map_err(|_| {
    Error::new(
      Status::InvalidArg,
      format!("{label} must not contain null bytes"),
    )
  })
}
