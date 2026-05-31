use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;

use libffi::middle::{Arg, CodePtr};
use libloading::Library;
use napi::bindgen_prelude::*;
use napi::Env;
use napi_derive::napi;

use crate::signature::{compile_signature, CompiledSignature};
use crate::targets::PreparedArg;

struct FunctionBinding {
  pointer: usize,
  signature: CompiledSignature,
}

#[napi(object)]
pub struct CallSpec {
  pub pointer: BigInt,
  pub arguments: Vec<String>,
  pub result: String,
  pub key: String,
}

#[napi]
pub struct DynamicLibrary {
  path: Option<String>,
  library: Option<Library>,
  functions: RefCell<HashMap<String, FunctionBinding>>,
}

fn pointer_from_bigint(pointer: &BigInt) -> Result<usize> {
  let (signed, raw, lossless) = pointer.get_u64();
  if signed || !lossless {
    return Err(Error::new(
      Status::InvalidArg,
      "The pointer must be a non-negative bigint".to_owned(),
    ));
  }
  usize::try_from(raw)
    .map_err(|_| Error::new(Status::InvalidArg, "The pointer exceeds the platform address range".to_owned()))
}

#[napi]
impl DynamicLibrary {
  #[napi(constructor)]
  pub fn new(path: Option<String>) -> Result<Self> {
    let path_value = path.ok_or_else(|| Error::new(Status::InvalidArg, "null path is not supported yet".to_owned()))?;
    let library = unsafe { Library::new(&path_value) }
      .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))?;
    Ok(Self {
      path: Some(path_value),
      library: Some(library),
      functions: RefCell::new(HashMap::new()),
    })
  }

  #[napi(getter)]
  pub fn path(&self) -> Option<String> {
    self.path.clone()
  }

  fn library(&self) -> Result<&Library> {
    self.library
      .as_ref()
      .ok_or_else(|| Error::new(Status::GenericFailure, "Library is closed".to_owned()))
  }

  #[napi]
  pub fn close(&mut self) {
    self.functions.borrow_mut().clear();
    self.library = None;
  }

  #[napi]
  pub fn get_symbol(&self, symbol: String) -> Result<BigInt> {
    let library = self.library()?;
    let pointer = unsafe {
      let raw = library
        .get::<*mut c_void>(symbol.as_bytes())
        .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))?;
      *raw as usize as u64
    };
    Ok(BigInt::from(pointer))
  }

  #[napi]
  pub fn get_function(&self, symbol: String, definition: Object) -> Result<CallSpec> {
    let compiled = compile_signature(definition)?;
    let pointer = {
      let mut cache = self.functions.borrow_mut();
      if let Some(existing) = cache.get(&symbol) {
        existing.pointer
      } else {
        let library = self.library()?;
        let raw_ptr = unsafe {
          let raw = library
            .get::<*mut c_void>(symbol.as_bytes())
            .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))?;
          *raw as usize
        };
        cache.insert(
          symbol.clone(),
          FunctionBinding {
            pointer: raw_ptr,
            signature: compiled,
          },
        );
        raw_ptr
      }
    };

    let cache = self.functions.borrow();
    let binding = cache.get(&symbol).expect("binding must exist after insertion");
    Ok(CallSpec {
      pointer: BigInt::from(pointer as u64),
      arguments: binding.signature.argument_type_names(),
      result: binding.signature.result_type_name(),
      key: symbol,
    })
  }

  #[napi]
  pub fn invoke<'env>(&self, env: &'env Env, key: String, pointer: BigInt, values: Vec<Unknown<'env>>) -> Result<Unknown<'env>> {
    let pointer = pointer_from_bigint(&pointer)?;
    let cache = self.functions.borrow();
    let binding = cache
      .get(&key)
      .ok_or_else(|| Error::new(Status::InvalidArg, format!("Function '{key}' is not defined")))?;
    if binding.pointer != pointer {
      return Err(Error::new(
        Status::InvalidArg,
        format!("Function '{key}' pointer does not match compiled definition"),
      ));
    }
    if values.len() != binding.signature.args.len() {
      return Err(Error::new(
        Status::InvalidArg,
        format!("Invalid argument count: expected {}, got {}", binding.signature.args.len(), values.len()),
      ));
    }

    let prepared = binding
      .signature
      .args
      .iter()
      .zip(values)
      .enumerate()
      .map(|(index, (target, value))| target.js_to_ffi(value, index))
      .collect::<Result<Vec<PreparedArg>>>()?;
    let ffi_args = prepared.iter().map(PreparedArg::as_arg).collect::<Vec<Arg<'_>>>();
    binding
      .signature
      .ret
      .ffi_to_js(env, &binding.signature.cif, CodePtr(pointer as *mut _), &ffi_args)
  }

  #[napi]
  pub fn get_functions<'env>(&self, env: &'env Env, definitions: Object<'env>) -> Result<Object<'env>> {
    let mut output = Object::new(env)?;
    for name in Object::keys(&definitions)? {
      let definition = definitions
        .get::<Object>(name.as_str())?
        .ok_or_else(|| Error::new(Status::InvalidArg, format!("Missing definition for symbol '{name}'")))?;
      let function = self.get_function(name.clone(), definition)?;
      output.set(name.as_str(), function)?;
    }
    Ok(output)
  }
}

#[napi]
pub fn dlopen<'env>(env: &'env Env, path: Option<String>, definitions: Option<Object<'env>>) -> Result<Object<'env>> {
  let lib = DynamicLibrary::new(path)?;
  let mut output = Object::new(env)?;
  let functions = if let Some(definitions) = definitions {
    lib.get_functions(env, definitions)?
  } else {
    Object::new(env)?
  };
  output.set("lib", lib)?;
  output.set("functions", functions)?;
  Ok(output)
}

#[napi]
pub fn dlclose(lib: &mut DynamicLibrary) {
  lib.close()
}

#[napi]
pub fn dlsym(lib: &DynamicLibrary, symbol: String) -> Result<BigInt> {
  lib.get_symbol(symbol)
}
