#![deny(clippy::all)]

mod dynamic_library;
mod memory;
mod signature;
mod storage;
mod targets;
mod types;
mod value_helpers;

pub use dynamic_library::{dlopen, CallSpec, DynamicLibrary};
pub use memory::{
  get_float32, get_float64, get_int16, get_int32, get_int64, get_int8, get_raw_pointer, get_uint16,
  get_uint32, get_uint64, get_uint8, set_float32, set_float64, set_int16, set_int32, set_int64,
  set_int8, set_uint16, set_uint32, set_uint64, set_uint8, to_array_buffer, to_buffer, to_string,
};
pub use types::suffix;
