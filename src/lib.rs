#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;

mod args;
mod buffer_helpers;
mod dynamic_library;
mod errors;
mod ffi_callback;
mod ffi_function;
mod memory;
mod signature;
mod types;

pub use buffer_helpers::*;
pub use dynamic_library::DynamicLibrary;
pub use memory::*;

#[napi]
pub fn get_suffix() -> String {
  if cfg!(target_os = "windows") {
    "dll".to_string()
  } else if cfg!(target_os = "macos") {
    "dylib".to_string()
  } else {
    "so".to_string()
  }
}

#[napi]
pub fn get_char_is_signed() -> bool {
  std::os::raw::c_char::MIN < 0
}

#[napi]
pub fn get_uintptr_max() -> u64 {
  usize::MAX as u64
}

#[napi]
pub fn get_k_sb_shared_buffer() -> Symbol {
  Symbol::new("ffi.kSbSharedBuffer")
}

#[napi]
pub fn get_k_sb_invoke_slow() -> Symbol {
  Symbol::new("ffi.kSbInvokeSlow")
}

#[napi]
pub fn get_k_sb_params() -> Symbol {
  Symbol::new("ffi.kSbParams")
}

#[napi]
pub fn get_k_sb_result() -> Symbol {
  Symbol::new("ffi.kSbResult")
}
