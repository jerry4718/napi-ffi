use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use napi_derive::napi;

use napi::bindgen_prelude::*;
use napi::{Env, JsValue, Unknown, ValueType};

use crate::ffi_callback::FFICallbackOwned;
use crate::ffi_function::FFIFunction;
use crate::signature::parse_function_signature;

/// Internal state owned by the JS DynamicLibrary instance.
struct DynamicLibraryInner {
  library: Option<libloading::Library>,
  path: String,
  symbols: HashMap<String, usize>,
  functions: HashMap<String, Rc<RefCell<FFIFunction>>>,
  callbacks: HashMap<usize, FFICallbackOwned>,
}

/// DynamicLibrary class — mirrors node:ffi's DynamicLibrary.
#[napi]
pub struct DynamicLibrary {
  inner: Rc<RefCell<DynamicLibraryInner>>,
}

#[napi]
impl DynamicLibrary {
  /// Constructor: open a dynamic library at `path`.
  #[napi(constructor)]
  pub fn new(path: Option<String>) -> Result<Self> {
    let (lib, actual_path) = if let Some(ref p) = path {
      let lib = unsafe { libloading::Library::new(p) }
        .map_err(|e| Error::new(Status::GenericFailure, format!("dlopen failed: {e}")))?;
      (Some(lib), p.clone())
    } else {
      #[cfg(not(windows))]
      {
        let lib = unsafe { libloading::Library::new("") }
          .map_err(|e| Error::new(Status::GenericFailure, format!("dlopen failed: {e}")))?;
        (Some(lib), String::new())
      }
      #[cfg(windows)]
      {
        return Err(Error::new(
          Status::InvalidArg,
          "Library path must be a string".to_string(),
        ));
      }
    };

    Ok(DynamicLibrary {
      inner: Rc::new(RefCell::new(DynamicLibraryInner {
        library: lib,
        path: actual_path,
        symbols: HashMap::new(),
        functions: HashMap::new(),
        callbacks: HashMap::new(),
      })),
    })
  }

  #[napi(getter)]
  pub fn path(&self) -> Result<String> {
    Ok(self.inner.borrow().path.clone())
  }

  #[napi(getter)]
  pub fn symbols(&self, env: &Env) -> Result<Object<'_>> {
    let inner = self.inner.borrow();
    let mut obj = Object::new(env)?;
    for (name, &ptr) in &inner.symbols {
      obj.set(name, BigInt::from(ptr as u64))?;
    }
    Ok(obj)
  }

  #[napi(getter)]
  pub fn functions(&self, env: &Env) -> Result<Object<'_>> {
    let inner = self.inner.borrow();
    let mut obj = Object::new(env)?;
    for (name, fn_ref) in &inner.functions {
      let js_fn = create_js_function_wrapper(env, name, fn_ref.clone())?;
      obj.set(name, js_fn)?;
    }
    Ok(obj)
  }

  #[napi]
  pub fn close(&self) -> Result<()> {
    let mut inner = self.inner.borrow_mut();
    for fn_ref in inner.functions.values() {
      fn_ref.borrow_mut().closed = true;
    }
    inner.library = None;
    inner.symbols.clear();
    inner.functions.clear();
    inner.callbacks.clear();
    Ok(())
  }

  fn resolve_symbol(inner: &mut DynamicLibraryInner, name: &str) -> Result<usize> {
    if inner.library.is_none() {
      return Err(Error::new(
        Status::GenericFailure,
        "Library is closed".to_string(),
      ));
    }
    if let Some(&ptr) = inner.symbols.get(name) {
      return Ok(ptr);
    }
    let lib = inner.library.as_ref().unwrap();
    let sym: libloading::Symbol<*mut std::ffi::c_void> = unsafe { lib.get(name.as_bytes()) }
      .map_err(|e| Error::new(Status::GenericFailure, format!("dlsym failed: {e}")))?;
    let ptr = *sym as usize;
    inner.symbols.insert(name.to_string(), ptr);
    Ok(ptr)
  }

  #[napi]
  pub fn get_symbol(&self, name: String) -> Result<BigInt> {
    let mut inner = self.inner.borrow_mut();
    let ptr = Self::resolve_symbol(&mut inner, &name)?;
    Ok(BigInt::from(ptr as u64))
  }

  #[napi]
  pub fn get_symbols(&self, env: &Env) -> Result<Object<'_>> {
    let inner = self.inner.borrow();
    if inner.library.is_none() {
      return Err(Error::new(
        Status::GenericFailure,
        "Library is closed".to_string(),
      ));
    }
    let mut obj = Object::new(env)?;
    for (name, &ptr) in &inner.symbols {
      obj.set(name, BigInt::from(ptr as u64))?;
    }
    Ok(obj)
  }

  #[napi]
  pub fn get_function<'env>(
    &self,
    env: &'env Env,
    name: String,
    sig: Object<'env>,
  ) -> Result<Function<'env, (), napi::sys::napi_value>> {
    let parsed = parse_function_signature(&name, &sig)?;

    let mut inner = self.inner.borrow_mut();
    if inner.library.is_none() {
      return Err(Error::new(
        Status::GenericFailure,
        "Library is closed".to_string(),
      ));
    }

    if let Some(existing) = inner.functions.get(&name) {
      let ex = existing.borrow();
      if ex.return_type != parsed.return_type || ex.arg_types != parsed.arg_types {
        return Err(Error::new(
          Status::InvalidArg,
          format!("Function {name} was already requested with a different signature"),
        ));
      }
      return create_js_function_wrapper(env, &name, existing.clone());
    }

    let ptr = Self::resolve_symbol(&mut inner, &name)?;
    let ffifn = Rc::new(RefCell::new(FFIFunction::new(ptr, &parsed)?));
    inner.functions.insert(name.clone(), ffifn.clone());
    create_js_function_wrapper(env, &name, ffifn)
  }

  #[napi]
  pub fn get_functions(&self, env: &Env, definitions: Option<Object>) -> Result<Object<'_>> {
    let mut obj = Object::new(env)?;

    if let Some(defs) = definitions {
      let keys = defs.get_property_names()?;
      let mut pending: Vec<(String, Rc<RefCell<FFIFunction>>)> = Vec::new();

      // Phase 1: prepare all functions (under lock)
      {
        let mut inner = self.inner.borrow_mut();
        if inner.library.is_none() {
          return Err(Error::new(
            Status::GenericFailure,
            "Library is closed".to_string(),
          ));
        }

        for i in 0..keys.get_array_length_unchecked()? {
          let key: Unknown = keys.get_element(i)?;
          let name: String = key.coerce_to_string()?.into_utf8()?.as_str()?.to_owned();
          let sig_val: Unknown = defs.get::<Unknown>(&name)?.ok_or_else(|| {
            Error::new(
              Status::InvalidArg,
              format!("Signature of function {name} must be an object"),
            )
          })?;
          if sig_val.get_type()? != ValueType::Object {
            return Err(Error::new(
              Status::InvalidArg,
              format!("Signature of function {name} must be an object"),
            ));
          }
          let sig_obj = Object::from_unknown(sig_val)?;
          let parsed = parse_function_signature(&name, &sig_obj)?;

          // Check existing cache
          if let Some(existing) = inner.functions.get(&name) {
            let ex = existing.borrow();
            if ex.return_type != parsed.return_type || ex.arg_types != parsed.arg_types {
              return Err(Error::new(
                Status::InvalidArg,
                format!("Function {name} was already requested with a different signature"),
              ));
            }
            pending.push((name, existing.clone()));
            continue;
          }

          let ptr = Self::resolve_symbol(&mut inner, &name)?;
          let ffifn = Rc::new(RefCell::new(FFIFunction::new(ptr, &parsed)?));
          inner.functions.insert(name.clone(), ffifn.clone());
          pending.push((name, ffifn));
        }
      }

      // Phase 2: create JS function wrappers (outside lock)
      for (name, ffifn) in pending {
        let js_fn = create_js_function_wrapper(env, &name, ffifn)?;
        obj.set(&name, js_fn)?;
      }
    } else {
      let inner = self.inner.borrow();
      for (name, fn_ref) in &inner.functions {
        let js_fn = create_js_function_wrapper(env, name, fn_ref.clone())?;
        obj.set(name, js_fn)?;
      }
    }

    Ok(obj)
  }

  #[napi]
  pub fn register_callback(
    &self,
    env: &Env,
    sig_or_fn: Unknown,
    fn_opt: Option<Unknown>,
  ) -> Result<BigInt> {
    let mut inner = self.inner.borrow_mut();
    if inner.library.is_none() {
      return Err(Error::new(
        Status::GenericFailure,
        "Library is closed".to_string(),
      ));
    }

    let (parsed, js_fn) = if let Some(js_fn) = fn_opt {
      let sig_obj = match sig_or_fn.get_type()? {
        ValueType::Object => Object::from_unknown(sig_or_fn)?,
        _ => {
          return Err(Error::new(
            Status::InvalidArg,
            "First argument must be a function or a signature object".to_string(),
          ));
        }
      };
      let parsed = parse_function_signature("<callback>", &sig_obj)?;
      (parsed, js_fn)
    } else {
      match sig_or_fn.get_type()? {
        ValueType::Function => {
          let parsed = crate::signature::ParsedSignature {
            return_type: crate::types::FFIType::Void,
            arg_types: Vec::new(),
            return_type_name: "void".to_string(),
            arg_type_names: Vec::new(),
          };
          (parsed, sig_or_fn)
        }
        _ => {
          return Err(Error::new(
            Status::InvalidArg,
            "First argument must be a function or a signature object".to_string(),
          ));
        }
      }
    };

    let callback = FFICallbackOwned::new(env, &parsed, &js_fn)?;
    let ptr = callback.ptr;
    inner.callbacks.insert(ptr, callback);
    Ok(BigInt::from(ptr as u64))
  }

  #[napi]
  pub fn unregister_callback(&self, _env: &Env, ptr: BigInt) -> Result<()> {
    let mut inner = self.inner.borrow_mut();
    if inner.library.is_none() {
      return Err(Error::new(
        Status::GenericFailure,
        "Library is closed".to_string(),
      ));
    }
    let addr = get_validated_pointer_compat(&ptr, "first argument")?;
    if inner.callbacks.remove(&addr).is_none() {
      return Err(Error::new(
        Status::InvalidArg,
        "Callback not found".to_string(),
      ));
    }
    Ok(())
  }

  #[napi]
  pub fn ref_callback(&self, env: &Env, ptr: BigInt) -> Result<()> {
    let mut inner = self.inner.borrow_mut();
    if inner.library.is_none() {
      return Err(Error::new(
        Status::GenericFailure,
        "Library is closed".to_string(),
      ));
    }
    let addr = get_validated_pointer_compat(&ptr, "first argument")?;
    if let Some(cb) = inner.callbacks.get_mut(&addr) {
      cb.ref_js_fn(env)?;
      Ok(())
    } else {
      Err(Error::new(
        Status::InvalidArg,
        "Callback not found".to_string(),
      ))
    }
  }

  #[napi]
  pub fn unref_callback(&self, env: &Env, ptr: BigInt) -> Result<()> {
    let mut inner = self.inner.borrow_mut();
    if inner.library.is_none() {
      return Err(Error::new(
        Status::GenericFailure,
        "Library is closed".to_string(),
      ));
    }
    let addr = get_validated_pointer_compat(&ptr, "first argument")?;
    if let Some(cb) = inner.callbacks.get_mut(&addr) {
      cb.unref_js_fn(env)?;
      Ok(())
    } else {
      Err(Error::new(
        Status::InvalidArg,
        "Callback not found".to_string(),
      ))
    }
  }
}

fn get_validated_pointer_compat(value: &BigInt, label: &str) -> Result<usize> {
  let (signed, addr, lossless) = value.get_u64();
  if signed || !lossless || addr > usize::MAX as u64 {
    return Err(Error::new(
      Status::InvalidArg,
      format!("The {label} must be a non-negative bigint"),
    ));
  }
  Ok(addr as usize)
}

/// Create a JS function wrapper that invokes an FFIFunction.
fn create_js_function_wrapper<'env>(
  env: &'env Env,
  name: &str,
  ffifn: Rc<RefCell<FFIFunction>>,
) -> Result<Function<'env, (), napi::sys::napi_value>> {
  let fn_ref = ffifn.borrow();
  let ptr = fn_ref.ptr;
  let return_type_name = fn_ref.return_type_name.clone();
  let arg_type_names = fn_ref.arg_type_names.clone();
  drop(fn_ref);

  let js_fn = env.create_function_from_closure::<(), napi::sys::napi_value, _>(
    name,
    move |ctx| -> Result<napi::sys::napi_value> {
      let args: Vec<Unknown> = (0..ctx.length())
        .map(|i| ctx.get::<Unknown>(i))
        .collect::<Result<Vec<_>>>()?;
      let fn_guard = ffifn.borrow();
      let result = fn_guard.invoke(ctx.env, &args)?;
      Ok(result.raw())
    },
  )?;

  let raw_env = env.raw();
  set_bigint_property(raw_env, js_fn.raw(), "pointer", ptr as u64)?;
  set_string_property(raw_env, js_fn.raw(), "__ffiReturnType", &return_type_name)?;
  set_string_array_property(env, js_fn.raw(), "__ffiArgTypes", &arg_type_names)?;

  Ok(js_fn)
}

fn set_bigint_property(
  env: napi::sys::napi_env,
  object: napi::sys::napi_value,
  name: &str,
  value: u64,
) -> Result<()> {
  let mut js_value = std::ptr::null_mut();
  check_status!(unsafe { napi::sys::napi_create_bigint_uint64(env, value, &mut js_value) })?;
  set_named_property(env, object, name, js_value)
}

fn set_string_property(
  env: napi::sys::napi_env,
  object: napi::sys::napi_value,
  name: &str,
  value: &str,
) -> Result<()> {
  let mut js_value = std::ptr::null_mut();
  check_status!(unsafe {
    napi::sys::napi_create_string_utf8(env, value.as_ptr().cast(), value.len() as isize, &mut js_value)
  })?;
  set_named_property(env, object, name, js_value)
}

fn set_string_array_property(
  env: &Env,
  object: napi::sys::napi_value,
  name: &str,
  values: &[String],
) -> Result<()> {
  let mut array = std::ptr::null_mut();
  check_status!(unsafe { napi::sys::napi_create_array_with_length(env.raw(), values.len(), &mut array) })?;
  for (index, value) in values.iter().enumerate() {
    let mut element = std::ptr::null_mut();
    check_status!(unsafe {
      napi::sys::napi_create_string_utf8(env.raw(), value.as_ptr().cast(), value.len() as isize, &mut element)
    })?;
    check_status!(unsafe { napi::sys::napi_set_element(env.raw(), array, index as u32, element) })?;
  }
  set_named_property(env.raw(), object, name, array)
}

fn set_named_property(
  env: napi::sys::napi_env,
  object: napi::sys::napi_value,
  name: &str,
  value: napi::sys::napi_value,
) -> Result<()> {
  let property = std::ffi::CString::new(name).expect("static string has no nul bytes");
  check_status!(unsafe { napi::sys::napi_set_named_property(env, object, property.as_ptr(), value) })?;
  Ok(())
}
