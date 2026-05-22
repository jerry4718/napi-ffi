use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::rc::Rc;

use napi_derive::napi;

use napi::bindgen_prelude::*;
use napi::{Env, JsValue, Property, PropertyAttributes, Unknown, ValueType};

use crate::ffi_callback::FFICallbackOwned;
use crate::ffi_function::FFIFunction;
use crate::signature::parse_function_signature;
use crate::types::get_validated_pointer_with_error;

/// Internal state owned by the JS DynamicLibrary instance.
struct DynamicLibraryInner {
  library: Option<libloading::Library>,
  path: String,
  symbols: HashMap<String, usize>,
  functions: HashMap<String, Rc<RefCell<FFIFunction>>>,
  callbacks: HashMap<usize, FFICallbackOwned>,
}

impl DynamicLibraryInner {
  fn closed_library_error() -> Error {
    Error::new(Status::GenericFailure, "Library is closed".to_string())
  }

  fn ensure_open(&self) -> Result<()> {
    self
      .library
      .as_ref()
      .ok_or_else(Self::closed_library_error)?;
    Ok(())
  }

  fn library(&self) -> Result<&libloading::Library> {
    self.library.as_ref().ok_or_else(Self::closed_library_error)
  }
}

fn ensure_cached_function_signature(
  name: &str,
  existing: &Rc<RefCell<FFIFunction>>,
  parsed: &crate::signature::ParsedSignature,
) -> Result<()> {
  let ex = existing.borrow();
  if ex.return_type != parsed.return_type || ex.arg_types != parsed.arg_types {
    return Err(Error::new(
      Status::InvalidArg,
      format!("Function {name} was already requested with a different signature"),
    ));
  }
  Ok(())
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
      if p.contains('\0') {
        return Err(Error::new(
          Status::InvalidArg,
          "Library path must not contain null bytes".to_string(),
        ));
      }
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
  pub fn symbols<'env>(&self, env: &'env Env) -> Result<Object<'env>> {
    let inner = self.inner.borrow();
    let mut obj = create_null_prototype_object(env)?;
    for (name, &ptr) in &inner.symbols {
      obj.set(name, BigInt::from(ptr as u64))?;
    }
    Ok(obj)
  }

  #[napi(getter)]
  pub fn functions<'env>(&self, env: &'env Env) -> Result<Object<'env>> {
    let inner = self.inner.borrow();
    let mut obj = create_null_prototype_object(env)?;
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
    inner.ensure_open()?;
    if let Some(&ptr) = inner.symbols.get(name) {
      return Ok(ptr);
    }
    let lib = inner.library()?;
    let sym: libloading::Symbol<*mut std::ffi::c_void> = unsafe { lib.get(name.as_bytes()) }
      .map_err(|e| Error::new(Status::GenericFailure, format!("dlsym failed: {e}")))?;
    let ptr = *sym as usize;
    inner.symbols.insert(name.to_string(), ptr);
    Ok(ptr)
  }

  #[napi]
  pub fn get_symbol(&self, name: String) -> Result<BigInt> {
    validate_symbol_name(&name)?;
    let mut inner = self.inner.borrow_mut();
    let ptr = Self::resolve_symbol(&mut inner, &name)?;
    Ok(BigInt::from(ptr as u64))
  }

  #[napi]
  pub fn get_symbols<'env>(&self, env: &'env Env) -> Result<Object<'env>> {
    let inner = self.inner.borrow();
    inner.ensure_open()?;
    let mut obj = create_null_prototype_object(env)?;
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
    validate_function_name(&name)?;
    let parsed = parse_function_signature(&name, &sig)?;

    let mut inner = self.inner.borrow_mut();
    inner.ensure_open()?;
    let (ffifn, should_cache) = Self::prepare_function(&mut inner, &name, &parsed)?;
    if should_cache {
      inner.functions.insert(name.clone(), ffifn.clone());
    }
    create_js_function_wrapper(env, &name, ffifn)
  }

  #[napi]
  pub fn get_functions<'env>(
    &self,
    env: &'env Env,
    definitions: Option<Unknown>,
  ) -> Result<Object<'env>> {
    let mut obj = create_null_prototype_object(env)?;

    if let Some(defs_value) = definitions {
      if defs_value.get_type()? != ValueType::Object || is_array(env.raw(), defs_value.raw())? {
        env.throw_type_error("Functions signatures must be an object", None)?;
        return Err(Error::new(Status::PendingException, "".to_string()));
      }
      let defs = Object::from_unknown(defs_value)?;
      let keys = defs.get_property_names()?;
      let mut pending: Vec<(String, Rc<RefCell<FFIFunction>>, bool)> = Vec::new();

      // Phase 1: prepare all functions without mutating the function cache until every definition succeeds.
      {
        let mut inner = self.inner.borrow_mut();
        inner.ensure_open()?;

        for i in 0..keys.get_array_length_unchecked()? {
          let key: Unknown = keys.get_element(i)?;
          let name: String = key.coerce_to_string()?.into_utf8()?.as_str()?.to_owned();
          validate_function_name(&name)?;
          let sig_val: Unknown = match defs.get::<Unknown>(&name)? {
            Some(value) => value,
            None => {
              env.throw_type_error(
                &format!("Signature of function {name} must be an object"),
                None,
              )?;
              return Err(Error::new(Status::PendingException, "".to_string()));
            }
          };
          if sig_val.get_type()? != ValueType::Object || is_array(env.raw(), sig_val.raw())? {
            env.throw_type_error(
              &format!("Signature of function {name} must be an object"),
              None,
            )?;
            return Err(Error::new(Status::PendingException, "".to_string()));
          }
          let sig_obj = Object::from_unknown(sig_val)?;
          let parsed = parse_function_signature(&name, &sig_obj)?;

          let (ffifn, should_cache) = Self::prepare_function(&mut inner, &name, &parsed)?;
          pending.push((name, ffifn, should_cache));
        }

        for (name, ffifn, should_cache) in &pending {
          if *should_cache {
            inner.functions.insert(name.clone(), ffifn.clone());
          }
        }
      }

      // Phase 2: create JS function wrappers (outside lock)
      for (name, ffifn, _) in pending {
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
    inner.ensure_open()?;

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
    let addr = get_validated_callback_pointer(&ptr)?;
    let mut inner = self.open_inner_mut()?;
    if inner.callbacks.remove(&addr).is_none() {
      return Err(callback_not_found_error());
    }
    Ok(())
  }

  #[napi]
  pub fn ref_callback(&self, env: &Env, ptr: BigInt) -> Result<()> {
    self.with_callback_mut(&ptr, |cb| cb.ref_js_fn(env))
  }

  #[napi]
  pub fn unref_callback(&self, env: &Env, ptr: BigInt) -> Result<()> {
    self.with_callback_mut(&ptr, |cb| cb.unref_js_fn(env))
  }
}

impl DynamicLibrary {
  fn prepare_function(
    inner: &mut DynamicLibraryInner,
    name: &str,
    parsed: &crate::signature::ParsedSignature,
  ) -> Result<(Rc<RefCell<FFIFunction>>, bool)> {
    if let Some(existing) = inner.functions.get(name) {
      ensure_cached_function_signature(name, existing, parsed)?;
      return Ok((existing.clone(), false));
    }

    let ptr = Self::resolve_symbol(inner, name)?;
    Ok((Rc::new(RefCell::new(FFIFunction::new(ptr, parsed)?)), true))
  }

  fn open_inner_mut(&self) -> Result<RefMut<'_, DynamicLibraryInner>> {
    let inner = self.inner.borrow_mut();
    inner.ensure_open()?;
    Ok(inner)
  }

  fn with_callback_mut<T>(
    &self,
    ptr: &BigInt,
    f: impl FnOnce(&mut FFICallbackOwned) -> Result<T>,
  ) -> Result<T> {
    let addr = get_validated_callback_pointer(ptr)?;
    let mut inner = self.open_inner_mut()?;
    let callback = inner
      .callbacks
      .get_mut(&addr)
      .ok_or_else(callback_not_found_error)?;
    f(callback)
  }
}

fn is_array(env: napi::sys::napi_env, value: napi::sys::napi_value) -> Result<bool> {
  let mut result = false;
  check_status!(unsafe { napi::sys::napi_is_array(env, value, &mut result) })?;
  Ok(result)
}

fn create_null_prototype_object<'env>(env: &'env Env) -> Result<Object<'env>> {
  let obj = Object::new(env)?;
  let raw_env = env.raw();
  let mut global = std::ptr::null_mut();
  let mut object_ctor = std::ptr::null_mut();
  let mut set_prototype_of = std::ptr::null_mut();
  let mut null_value = std::ptr::null_mut();

  check_status!(unsafe { napi::sys::napi_get_global(raw_env, &mut global) })?;
  check_status!(unsafe {
    napi::sys::napi_get_named_property(raw_env, global, c"Object".as_ptr().cast(), &mut object_ctor)
  })?;
  check_status!(unsafe {
    napi::sys::napi_get_named_property(
      raw_env,
      object_ctor,
      c"setPrototypeOf".as_ptr().cast(),
      &mut set_prototype_of,
    )
  })?;
  check_status!(unsafe { napi::sys::napi_get_null(raw_env, &mut null_value) })?;

  let mut argv = [obj.raw(), null_value];
  check_status!(unsafe {
    napi::sys::napi_call_function(
      raw_env,
      object_ctor,
      set_prototype_of,
      argv.len(),
      argv.as_mut_ptr(),
      std::ptr::null_mut(),
    )
  })?;

  Ok(obj)
}

fn validate_function_name(name: &str) -> Result<()> {
  validate_no_null_bytes(name, "Function name must not contain null bytes")
}

fn validate_symbol_name(name: &str) -> Result<()> {
  validate_no_null_bytes(name, "Symbol name must not contain null bytes")
}

fn validate_no_null_bytes(value: &str, message: &str) -> Result<()> {
  if value.contains('\0') {
    return Err(Error::new(Status::InvalidArg, message.to_string()));
  }
  Ok(())
}

fn get_validated_callback_pointer(value: &BigInt) -> Result<usize> {
  get_validated_pointer_with_error(value, "The first argument must be a non-negative bigint")
    .map_err(|error| {
      if error.reason.contains("platform pointer range") {
        Error::new(
          Status::InvalidArg,
          "The first argument must be a non-negative bigint".to_string(),
        )
      } else {
        error
      }
    })
}

fn callback_not_found_error() -> Error {
  Error::new(Status::InvalidArg, "Callback not found".to_string())
}

/// Create a JS function wrapper that invokes an FFIFunction.
fn create_js_function_wrapper<'env>(
  env: &'env Env,
  name: &str,
  ffifn: Rc<RefCell<FFIFunction>>,
) -> Result<Function<'env, (), napi::sys::napi_value>> {
  let fn_ref = ffifn.borrow();
  let ptr = fn_ref.ptr;
  let arity = fn_ref.arg_types.len() as u32;
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

  let mut js_fn = js_fn;
  let metadata_attrs = PropertyAttributes::Default;
  let pointer_attrs = PropertyAttributes::Enumerable;
  let length_attrs = PropertyAttributes::Configurable;
  let properties = [
    Property::new()
      .with_utf8_name("pointer")?
      .with_napi_value(env, BigInt::from(ptr as u64))?
      .with_property_attributes(pointer_attrs),
    Property::new()
      .with_utf8_name("length")?
      .with_napi_value(env, arity)?
      .with_property_attributes(length_attrs),
    Property::new()
      .with_utf8_name("__ffiReturnType")?
      .with_napi_value(env, return_type_name)?
      .with_property_attributes(metadata_attrs),
    Property::new()
      .with_utf8_name("__ffiArgTypes")?
      .with_napi_value(env, arg_type_names)?
      .with_property_attributes(metadata_attrs),
  ];
  js_fn.define_properties(&properties)?;

  Ok(js_fn)
}
