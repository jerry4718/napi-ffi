use libffi::low::CodePtr;
use libffi::middle::{arg, Cif, Ret, Type};

use napi::bindgen_prelude::*;
use napi::Env;

use crate::errors::{throw_coded_error, JsErrorKind};
use crate::signature::ParsedSignature;
use crate::types::{marshal_c_to_js, marshal_js_to_c, FFIType};

/// Represents a prepared FFI function with its CIF and metadata.
/// Wrapped in Arc so it can be shared between DynamicLibrary and JS function closures.
pub struct FFIFunction {
  pub ptr: usize,
  pub cif: Cif,
  pub return_type: FFIType,
  pub arg_types: Vec<FFIType>,
  pub return_type_name: String,
  pub arg_type_names: Vec<String>,
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
      return_type_name: sig.return_type_name.clone(),
      arg_type_names: sig.arg_type_names.clone(),
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
      return throw_coded_error(
        env,
        JsErrorKind::TypeError,
        "ERR_INVALID_ARG_VALUE",
        format!(
          "Invalid argument count: expected {expected_args}, got {}",
          args.len()
        ),
      );
    }

    // Marshal JS arguments to C-compatible storage
    let mut values: Vec<u64> = vec![0u64; expected_args];
    let mut string_keepalive: Vec<std::ffi::CString> = Vec::new();

    for (i, (arg, ffitype)) in args.iter().zip(self.arg_types.iter()).enumerate() {
      let marshaled = match marshal_js_to_c(env, arg, *ffitype, i, &mut values[i]) {
        Ok(value) => value,
        Err(error) if error.status == Status::InvalidArg => {
          return throw_coded_error(
            env,
            JsErrorKind::TypeError,
            "ERR_INVALID_ARG_VALUE",
            error.reason.clone(),
          );
        }
        Err(error) => return Err(error),
      };
      if let Some(s) = marshaled {
        // String argument: create CString and store address
        let cstr = match std::ffi::CString::new(s) {
          Ok(value) => value,
          Err(error) => {
            return throw_coded_error(
              env,
              JsErrorKind::TypeError,
              "ERR_INVALID_ARG_VALUE",
              format!("Argument {i} must not contain null bytes: {error}"),
            );
          }
        };
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
      call_ffi_void(&self.cif, code, &ffi_args);
      ().into_unknown(env)
    } else {
      call_ffi_return(&self.cif, code, &ffi_args, &mut ret_storage[0]);
      marshal_c_to_js(env, &ret_storage[0], self.return_type)
    }
  }
}

fn call_ffi_void(cif: &Cif, code: CodePtr, args: &[libffi::middle::Arg]) {
  // SAFETY: the DynamicLibrary symbol address and ParsedSignature-derived CIF define the ABI
  // contract. Callers already marshaled every JS argument into storage matching the declared type.
  unsafe {
    cif.call_return_into(code, args, Ret::void());
  }
}

fn call_ffi_return(cif: &Cif, code: CodePtr, args: &[libffi::middle::Arg], storage: &mut u64) {
  // SAFETY: storage points to the return slot reserved by invoke(), and the CIF return type was
  // derived from the parsed signature. libffi writes the declared return representation there.
  unsafe {
    let ret = Ret::new(storage);
    cif.call_return_into(code, args, ret);
  }
}
