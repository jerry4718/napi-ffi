use napi_derive::napi;

#[cfg(target_os = "windows")]
const SUFFIX: &str = "dll";
#[cfg(target_os = "macos")]
const SUFFIX: &str = "dylib";
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
const SUFFIX: &str = "so";

#[napi]
pub fn suffix() -> String {
  SUFFIX.to_owned()
}
