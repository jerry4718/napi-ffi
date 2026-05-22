use libffi::low::CodePtr;
use libffi::middle::{arg, Cif, Ret, Type};

use napi::bindgen_prelude::*;
use napi::Env;

use crate::signature::ParsedSignature;
use crate::types::{marshal_c_to_js, marshal_js_to_c, FFIType};

/// Represents a prepared FFI function with its CIF and metadata.
/// Wrapped in Arc so it can be shared between DynamicLibrary and JS function closures.
pub struct FFIFunction {
  pub ptr: usize,
  pub cif: Cif,
  pub return_type: FFIType,
  pub arg_types: Vec<FFIType>,
  pub closed: bool,
}

impl FFIFunction {
  /// Create a new FFIFunction from a function pointer and signature.
  pub fn new(ptr: usize, sig: &ParsedSignature) -> Result<Self> {
    let middle_arg_types: Vec<Type> = sig.arg_types.iter().map(|t| t.middle_type()).collect();
    let middle_ret_type = sig.return_type.middle_type();

    let cif = Cif::new(middle_arg_types, middle_ret_type);

    Ok(FFIFunction {
      ptr,
      cif,
      return_type: sig.return_type,
      arg_types: sig.arg_types.clone(),
      closed: false,
    })
  }

  /// Invoke the FFI function with JS arguments.
  /// Returns the result as a JS value.
  pub fn invoke<'env>(&self, env: &'env Env, args: &[Unknown<'env>]) -> Result<Unknown<'env>> {
    if self.closed {
      return Err(Error::new(
        Status::GenericFailure,
        "Library is closed".to_string(),
      ));
    }

    let expected_args = self.arg_types.len();
    if args.len() != expected_args {
      return Err(Error::new(
        Status::InvalidArg,
        format!(
          "Invalid argument count: expected {expected_args}, got {}",
          args.len()
        ),
      ));
    }

    // Marshal JS arguments to C-compatible storage
    let mut values: Vec<u64> = vec![0u64; expected_args];
    let mut string_keepalive: Vec<std::ffi::CString> = Vec::new();

    for (i, (arg, ffitype)) in args.iter().zip(self.arg_types.iter()).enumerate() {
      if let Some(s) = marshal_js_to_c(env, arg, *ffitype, i, &mut values[i])? {
        // String argument: create CString and store address
        let cstr = std::ffi::CString::new(s).map_err(|e| {
          Error::new(
            Status::InvalidArg,
            format!("Argument {i} must not contain null bytes: {e}"),
          )
        })?;
        values[i] = cstr.as_ptr() as u64;
        string_keepalive.push(cstr);
      }
    }

    // Convert values to libffi args
    let ffi_args: Vec<libffi::middle::Arg> = values.iter().map(|v| arg(v)).collect();

    // Prepare return value storage. Use 16 bytes aligned to 8 for safety (covers ffi_arg + 8-byte types)
    let mut ret_storage: [u64; 2] = [0u64; 2];

    let code = CodePtr::from_ptr(self.ptr as *const std::ffi::c_void);

    if self.return_type == FFIType::Void {
      unsafe {
        self.cif.call_return_into(code, &ffi_args, Ret::void());
      }
      ().into_unknown(env)
    } else {
      unsafe {
        let ret = Ret::new(&mut ret_storage[0]);
        self.cif.call_return_into(code, &ffi_args, ret);
      }
      marshal_c_to_js(env, &ret_storage[0], self.return_type)
    }
  }
}
