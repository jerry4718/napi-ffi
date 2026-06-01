use std::ffi::c_void;

use napi::bindgen_prelude::*;

pub(crate) fn raw_bytes_pointer(value: Unknown<'_>) -> Option<*mut c_void> {
  if let Ok(buffer) = unsafe { value.cast::<Buffer>() } {
    return Some(buffer.as_ref().as_ptr() as *mut c_void);
  }
  if let Ok(arraybuffer) = unsafe { value.cast::<ArrayBuffer>() } {
    return Some(arraybuffer.as_ref().as_ptr() as *mut c_void);
  }
  if let Ok(typed) = unsafe { value.cast::<TypedArray>() } {
    return Some(
      typed
        .arraybuffer
        .as_ref()
        .as_ptr()
        .wrapping_add(typed.byte_offset) as *mut c_void,
    );
  }
  None
}
