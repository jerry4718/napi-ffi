use napi::bindgen_prelude::*;
use napi::Env;

pub enum JsErrorKind {
  RangeError,
  TypeError,
}

pub(crate) fn create_string_utf8(
  raw_env: napi::sys::napi_env,
  value: &str,
) -> Result<napi::sys::napi_value> {
  let mut out = std::ptr::null_mut();
  check_status!(unsafe {
    napi::sys::napi_create_string_utf8(
      raw_env,
      value.as_ptr().cast(),
      value.len() as isize,
      &mut out,
    )
  })?;
  Ok(out)
}

pub fn throw_coded_error<T>(
  env: &Env,
  kind: JsErrorKind,
  code: &str,
  message: impl AsRef<str>,
) -> Result<T> {
  let raw_env = env.raw();
  let message = message.as_ref();
  let code_value = create_string_utf8(raw_env, code)?;
  let message_value = create_string_utf8(raw_env, message)?;
  let mut error_value = std::ptr::null_mut();

  match kind {
    JsErrorKind::RangeError => check_status!(unsafe {
      napi::sys::napi_create_range_error(raw_env, code_value, message_value, &mut error_value)
    })?,
    JsErrorKind::TypeError => check_status!(unsafe {
      napi::sys::napi_create_type_error(raw_env, code_value, message_value, &mut error_value)
    })?,
  }

  check_status!(unsafe {
    napi::sys::napi_set_named_property(raw_env, error_value, c"code".as_ptr().cast(), code_value)
  })?;
  check_status!(unsafe { napi::sys::napi_throw(raw_env, error_value) })?;
  Err(Error::new(Status::PendingException, String::new()))
}
