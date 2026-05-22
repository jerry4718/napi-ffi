use napi::bindgen_prelude::*;
use napi::{Env, Unknown};

use crate::signature::ParsedSignature;

/// Owned callback data — stored in DynamicLibraryInner.callbacks.
pub struct FFICallbackOwned {
  pub ptr: usize,
}

impl FFICallbackOwned {
  /// Create a new callback closure.
  pub fn new(_env: &Env, _sig: &ParsedSignature, _js_fn: &Unknown) -> Result<Self> {
    Err(Error::new(
      Status::GenericFailure,
      "FFI callbacks are not available in the napi-rs v3 implementation yet".to_string(),
    ))
  }

  /// Reference the JS function (prevent GC).
  pub fn ref_js_fn(&mut self, _env: &Env) -> Result<()> {
    Ok(())
  }

  /// Unreference the JS function (allow GC).
  pub fn unref_js_fn(&mut self, _env: &Env) -> Result<()> {
    Ok(())
  }
}
