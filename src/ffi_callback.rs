use std::ffi::{c_char, c_void};
use std::ptr;
use std::thread::ThreadId;

use libffi::middle::{Cif, Closure, Type};
use napi::bindgen_prelude::*;
use napi::{Env, Unknown};

use crate::signature::ParsedSignature;
use crate::types::FFIType;

struct CallbackState {
  env: napi::sys::napi_env,
  js_fn_ref: napi::sys::napi_ref,
  return_type: FFIType,
  arg_types: Vec<FFIType>,
  thread_id: ThreadId,
}

impl Drop for CallbackState {
  fn drop(&mut self) {
    unsafe {
      napi::sys::napi_delete_reference(self.env, self.js_fn_ref);
    }
  }
}

/// Owned callback data — stored in DynamicLibraryInner.callbacks.
pub struct FFICallbackOwned {
  pub ptr: usize,
  // Keep _closure before state so drop order releases the libffi closure before CallbackState.
  _closure: Closure<'static>,
  state: Box<CallbackState>,
}

impl FFICallbackOwned {
  /// Create a new callback closure.
  pub fn new(env: &Env, sig: &ParsedSignature, js_fn: &Unknown) -> Result<Self> {
    if js_fn.get_type()? != ValueType::Function {
      return Err(Error::new(
        Status::InvalidArg,
        "Callback must be a function".to_string(),
      ));
    }

    let env_raw = env.raw();
    let mut js_fn_ref = ptr::null_mut();
    check_status!(unsafe {
      napi::sys::napi_create_reference(env_raw, js_fn.raw(), 1, &mut js_fn_ref)
    })?;

    let state = Box::new(CallbackState {
      env: env_raw,
      js_fn_ref,
      return_type: sig.return_type,
      arg_types: sig.arg_types.clone(),
      thread_id: std::thread::current().id(),
    });
    let state_ref = callback_state_as_static(&state);

    let arg_types: Vec<Type> = sig.arg_types.iter().map(|t| t.middle_type()).collect();
    let cif = Cif::new(arg_types, sig.return_type.middle_type());
    let closure = match sig.return_type {
      FFIType::Void => Closure::new(cif, callback_trampoline_void, state_ref),
      FFIType::Sint8 => Closure::new(cif, callback_trampoline_i8, state_ref),
      FFIType::Uint8 => Closure::new(cif, callback_trampoline_u8, state_ref),
      FFIType::Sint16 => Closure::new(cif, callback_trampoline_i16, state_ref),
      FFIType::Uint16 => Closure::new(cif, callback_trampoline_u16, state_ref),
      FFIType::Sint32 => Closure::new(cif, callback_trampoline_i32, state_ref),
      FFIType::Uint32 => Closure::new(cif, callback_trampoline_u32, state_ref),
      FFIType::Sint64 => Closure::new(cif, callback_trampoline_i64, state_ref),
      FFIType::Uint64 | FFIType::Pointer => Closure::new(cif, callback_trampoline_u64, state_ref),
      FFIType::Float => Closure::new(cif, callback_trampoline_f32, state_ref),
      FFIType::Double => Closure::new(cif, callback_trampoline_f64, state_ref),
    };
    let ptr = *closure.code_ptr() as *const c_void as usize;

    Ok(FFICallbackOwned {
      ptr,
      _closure: closure,
      state,
    })
  }

  /// Reference the JS function (prevent GC).
  pub fn ref_js_fn(&mut self, env: &Env) -> Result<()> {
    let mut ref_count = 0;
    check_status!(unsafe {
      napi::sys::napi_reference_ref(env.raw(), self.state.js_fn_ref, &mut ref_count)
    })
  }

  /// Unreference the JS function (allow GC).
  pub fn unref_js_fn(&mut self, env: &Env) -> Result<()> {
    let mut ref_count = 0;
    check_status!(unsafe {
      napi::sys::napi_reference_unref(env.raw(), self.state.js_fn_ref, &mut ref_count)
    })
  }
}

fn callback_state_as_static(state: &CallbackState) -> &'static CallbackState {
  // SAFETY: libffi closures require callback data with a 'static lifetime. FFICallbackOwned
  // stores the Closure before the boxed state, so the closure is dropped before the state and
  // cannot invoke this reference after the state is freed. The Box keeps the state address stable.
  unsafe { &*(state as *const CallbackState) }
}

unsafe extern "C" fn callback_trampoline_void(
  _cif: &libffi::low::ffi_cif,
  _result: &mut (),
  args: *const *const c_void,
  state: &CallbackState,
) {
  let _ = call_js_callback(state, args);
}

macro_rules! define_callback_trampoline {
  ($name:ident, $result_ty:ty, $reader:ident) => {
    unsafe extern "C" fn $name(
      _cif: &libffi::low::ffi_cif,
      result: &mut $result_ty,
      args: *const *const c_void,
      state: &CallbackState,
    ) {
      *result = call_js_callback(state, args)
        .and_then(|ret| $reader(state, ret))
        .unwrap_or_else(fatal_invalid_return) as $result_ty;
    }
  };
}

define_callback_trampoline!(callback_trampoline_i8, i8, read_i32_return);
define_callback_trampoline!(callback_trampoline_u8, u8, read_u32_return);
define_callback_trampoline!(callback_trampoline_i16, i16, read_i32_return);
define_callback_trampoline!(callback_trampoline_u16, u16, read_u32_return);
define_callback_trampoline!(callback_trampoline_i32, i32, read_i32_return);
define_callback_trampoline!(callback_trampoline_u32, u32, read_u32_return);
define_callback_trampoline!(callback_trampoline_i64, i64, read_i64_return);
define_callback_trampoline!(callback_trampoline_u64, u64, read_u64_return);
define_callback_trampoline!(callback_trampoline_f32, f32, read_f64_return);
define_callback_trampoline!(callback_trampoline_f64, f64, read_f64_return);

unsafe fn call_js_callback(
  state: &CallbackState,
  args: *const *const c_void,
) -> Result<napi::sys::napi_value> {
  if std::thread::current().id() != state.thread_id {
    fatal_abort::<napi::sys::napi_value>(
      "Callbacks can only be invoked on the system thread they were created on",
    );
  }

  let js_args = callback_args_to_js(state, args)?;
  let mut js_fn = ptr::null_mut();
  check_status!(napi::sys::napi_get_reference_value(
    state.env,
    state.js_fn_ref,
    &mut js_fn
  ))?;
  if js_fn.is_null() {
    return Err(Error::new(
      Status::GenericFailure,
      "Callback reference is empty".to_string(),
    ));
  }

  let mut undefined = ptr::null_mut();
  check_status!(napi::sys::napi_get_undefined(state.env, &mut undefined))?;

  let mut ret = ptr::null_mut();
  let call_status = napi::sys::napi_call_function(
    state.env,
    undefined,
    js_fn,
    js_args.len(),
    js_args.as_ptr(),
    &mut ret,
  );
  if call_status != napi::sys::Status::napi_ok {
    fatal_abort::<napi::sys::napi_value>("Callbacks cannot throw an exception");
  }

  let mut is_promise = false;
  if napi::sys::napi_is_promise(state.env, ret, &mut is_promise) == napi::sys::Status::napi_ok
    && is_promise
  {
    fatal_abort::<napi::sys::napi_value>("Callbacks cannot return promises");
  }

  Ok(ret)
}

unsafe fn callback_args_to_js(
  state: &CallbackState,
  args: *const *const c_void,
) -> Result<Vec<napi::sys::napi_value>> {
  let mut js_args = Vec::with_capacity(state.arg_types.len());
  for (index, ffitype) in state.arg_types.iter().enumerate() {
    let arg_ptr = callback_arg_at(args, index);
    let mut value = ptr::null_mut();
    match ffitype {
      FFIType::Void => napi::sys::napi_get_undefined(state.env, &mut value),
      FFIType::Sint8 => napi::sys::napi_create_int32(
        state.env,
        read_callback_arg::<i8>(arg_ptr) as i32,
        &mut value,
      ),
      FFIType::Uint8 => napi::sys::napi_create_uint32(
        state.env,
        read_callback_arg::<u8>(arg_ptr) as u32,
        &mut value,
      ),
      FFIType::Sint16 => napi::sys::napi_create_int32(
        state.env,
        read_callback_arg::<i16>(arg_ptr) as i32,
        &mut value,
      ),
      FFIType::Uint16 => napi::sys::napi_create_uint32(
        state.env,
        read_callback_arg::<u16>(arg_ptr) as u32,
        &mut value,
      ),
      FFIType::Sint32 => {
        napi::sys::napi_create_int32(state.env, read_callback_arg::<i32>(arg_ptr), &mut value)
      }
      FFIType::Uint32 => {
        napi::sys::napi_create_uint32(state.env, read_callback_arg::<u32>(arg_ptr), &mut value)
      }
      FFIType::Sint64 => napi::sys::napi_create_bigint_int64(
        state.env,
        read_callback_arg::<i64>(arg_ptr),
        &mut value,
      ),
      FFIType::Uint64 => napi::sys::napi_create_bigint_uint64(
        state.env,
        read_callback_arg::<u64>(arg_ptr),
        &mut value,
      ),
      FFIType::Float => napi::sys::napi_create_double(
        state.env,
        read_callback_arg::<f32>(arg_ptr) as f64,
        &mut value,
      ),
      FFIType::Double => {
        napi::sys::napi_create_double(state.env, read_callback_arg::<f64>(arg_ptr), &mut value)
      }
      FFIType::Pointer => napi::sys::napi_create_bigint_uint64(
        state.env,
        read_callback_arg::<usize>(arg_ptr) as u64,
        &mut value,
      ),
    };
    if value.is_null() {
      return Err(Error::new(
        Status::GenericFailure,
        "Failed to marshal callback argument".to_string(),
      ));
    }
    js_args.push(value);
  }
  Ok(js_args)
}

unsafe fn callback_arg_at(args: *const *const c_void, index: usize) -> *const c_void {
  // SAFETY: libffi passes an args array with one valid entry for each declared callback argument.
  *args.add(index)
}

unsafe fn read_callback_arg<T: Copy>(arg_ptr: *const c_void) -> T {
  // SAFETY: arg_ptr is provided by libffi for the currently matched FFIType and points to a value
  // with the corresponding C ABI representation for T.
  *(arg_ptr as *const T)
}

unsafe fn read_i32_return(state: &CallbackState, ret: napi::sys::napi_value) -> Result<i32> {
  let value = read_f64_return(state, ret)?;
  validate_callback_integer_return(value, i32::MIN as f64, i32::MAX as f64)?;
  Ok(value as i32)
}

unsafe fn read_u32_return(state: &CallbackState, ret: napi::sys::napi_value) -> Result<u32> {
  let value = read_f64_return(state, ret)?;
  validate_callback_integer_return(value, 0.0, u32::MAX as f64)?;
  Ok(value as u32)
}

fn validate_callback_integer_return(value: f64, min: f64, max: f64) -> Result<()> {
  if !value.is_finite() || value.fract() != 0.0 || value < min || value > max {
    return Err(Error::new(
      Status::InvalidArg,
      "Invalid callback return".to_string(),
    ));
  }
  Ok(())
}

unsafe fn read_i64_return(state: &CallbackState, ret: napi::sys::napi_value) -> Result<i64> {
  let mut value = 0;
  let mut lossless = false;
  check_status!(napi::sys::napi_get_value_bigint_int64(
    state.env,
    ret,
    &mut value,
    &mut lossless
  ))?;
  Ok(value)
}

unsafe fn read_u64_return(state: &CallbackState, ret: napi::sys::napi_value) -> Result<u64> {
  if state.return_type == FFIType::Pointer {
    let mut value_type = napi::sys::ValueType::napi_undefined;
    check_status!(napi::sys::napi_typeof(state.env, ret, &mut value_type))?;
    if matches!(
      value_type,
      napi::sys::ValueType::napi_null | napi::sys::ValueType::napi_undefined
    ) {
      return Ok(0);
    }
  }
  let mut value = 0;
  let mut lossless = false;
  check_status!(napi::sys::napi_get_value_bigint_uint64(
    state.env,
    ret,
    &mut value,
    &mut lossless
  ))?;
  Ok(value)
}

unsafe fn read_f64_return(state: &CallbackState, ret: napi::sys::napi_value) -> Result<f64> {
  let mut value = 0.0;
  check_status!(napi::sys::napi_get_value_double(state.env, ret, &mut value))?;
  Ok(value)
}

fn fatal_invalid_return<T: Default>(err: Error) -> T {
  if err.reason == "Callback reference is empty" {
    return T::default();
  }
  fatal_abort("Callback returned invalid value for declared FFI type")
}

fn fatal_abort<T>(message: &str) -> T {
  unsafe {
    napi::sys::napi_fatal_error(
      ptr::null(),
      0,
      message.as_ptr() as *const c_char,
      message.len() as isize,
    );
  }
  unreachable!()
}
