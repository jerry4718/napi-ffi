use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;

use libffi::middle::{Arg, Cif, Closure, CodePtr};
#[cfg(unix)]
use libloading::os::unix::Library as UnixLibrary;
#[cfg(windows)]
use libloading::os::windows::Library as WindowsLibrary;
use libloading::Library;
use napi::bindgen_prelude::*;
use napi::{Env, UnknownRef};
use napi_derive::napi;

use crate::signature::{
  compile_callback_signature, compile_function_signature, CompiledCallbackSignature,
  CompiledFunctionSignature,
};

struct PreparedFunctionArg {
  target: *const dyn crate::targets::TypedTarget,
  storage: NonNull<u8>,
  layout: Layout,
}

impl PreparedFunctionArg {
  fn new(target: &dyn crate::targets::TypedTarget) -> Result<Self> {
    let layout = target.function_arg_layout();
    let storage = if layout.size() == 0 {
      NonNull::dangling()
    } else {
      let raw = unsafe { alloc(layout) };
      NonNull::new(raw).ok_or_else(|| Error::new(Status::GenericFailure, "Allocation failed".to_owned()))?
    };
    Ok(Self {
      target: target as *const dyn crate::targets::TypedTarget,
      storage,
      layout,
    })
  }

  fn as_ptr(&self) -> *mut u8 {
    self.storage.as_ptr()
  }

  unsafe fn as_arg(&self) -> Arg<'_> {
    unsafe { (&*self.target).function_arg_as_ffi_arg(self.storage.as_ptr()) }
  }
}

impl Drop for PreparedFunctionArg {
  fn drop(&mut self) {
    unsafe {
      (&*self.target).drop_function_arg(self.storage.as_ptr());
      if self.layout.size() != 0 {
        dealloc(self.storage.as_ptr(), self.layout);
      }
    }
  }
}

struct PreparedCallbackReturn {
  target: *const dyn crate::targets::TypedTarget,
  storage: NonNull<u8>,
  layout: Layout,
}

impl PreparedCallbackReturn {
  fn new(target: &dyn crate::targets::TypedTarget) -> Result<Self> {
    let layout = target.callback_return_layout();
    let storage = if layout.size() == 0 {
      NonNull::dangling()
    } else {
      let raw = unsafe { alloc(layout) };
      NonNull::new(raw).ok_or_else(|| Error::new(Status::GenericFailure, "Allocation failed".to_owned()))?
    };
    Ok(Self {
      target: target as *const dyn crate::targets::TypedTarget,
      storage,
      layout,
    })
  }

  fn as_mut_ptr(&self) -> *mut u8 {
    self.storage.as_ptr()
  }

  unsafe fn copy_into_result(&self, result: &mut *mut c_void) {
    let value_ptr = unsafe { (&*self.target).callback_return_ptr(self.storage.as_ptr()) };
    let copy_size = self.layout.size();
    if copy_size != 0 {
      unsafe {
        std::ptr::copy_nonoverlapping(
          value_ptr.cast::<u8>(),
          (result as *mut *mut c_void).cast::<u8>(),
          copy_size,
        );
      }
    }
  }
}

impl Drop for PreparedCallbackReturn {
  fn drop(&mut self) {
    unsafe {
      (&*self.target).drop_callback_return(self.storage.as_ptr());
      if self.layout.size() != 0 {
        dealloc(self.storage.as_ptr(), self.layout);
      }
    }
  }
}

struct FunctionBinding {
  pointer: usize,
  signature: CompiledFunctionSignature,
}

struct CallbackContext {
  env: napi::sys::napi_env,
  signature: CompiledCallbackSignature,
  function: UnknownRef,
}

impl CallbackContext {
  unsafe fn invoke(&mut self, result: &mut *mut c_void, args: *const *const c_void) {
    let _ = unsafe { self.try_invoke(result, args) };
  }

  unsafe fn try_invoke(&mut self, result: &mut *mut c_void, args: *const *const c_void) -> Result<()> {
    let env = Env::from_raw(self.env);
    let function_value = self.function.get_value(&env)?;

    let js_args = self
      .signature
      .args
      .iter()
      .enumerate()
      .map(|(index, target)| {
        let arg_ptr = unsafe { *args.add(index) };
        unsafe { target.formalize_callback_arg(&env, arg_ptr, index) }
      })
      .collect::<Result<Vec<_>>>()?;

    let raw_args = js_args
      .iter()
      .map(|arg| unsafe { arg.raw() })
      .collect::<Vec<sys::napi_value>>();
    let mut raw_this = std::ptr::null_mut();
    check_status!(unsafe { sys::napi_get_undefined(env.raw(), &mut raw_this) }, "Get undefined value failed")?;
    let mut raw_return = std::ptr::null_mut();
    check_pending_exception!(env.raw(), unsafe {
      sys::napi_call_function(
        env.raw(),
        raw_this,
        function_value.raw(),
        raw_args.len(),
        raw_args.as_ptr(),
        &mut raw_return,
      )
    }, "Call Function failed")?;
    let returned = unsafe { Unknown::from_raw_unchecked(env.raw(), raw_return) };
    let prepared = PreparedCallbackReturn::new(self.signature.ret.as_ref())?;
    unsafe {
      self
        .signature
        .ret
        .formalize_callback_return(&env, returned, prepared.as_mut_ptr())?;
      prepared.copy_into_result(result);
    }
    Ok(())
  }
}

impl Drop for CallbackContext {
  fn drop(&mut self) {
    unsafe {
      let function = std::ptr::read(&self.function);
      let env = Env::from_raw(self.env);
      let _ = function.unref(&env);
    }
  }
}

unsafe extern "C" fn callback_adapter(
  _cif: &libffi::low::ffi_cif,
  result: &mut *mut c_void,
  args: *const *const c_void,
  context: &mut CallbackContext,
) {
  unsafe { context.invoke(result, args) }
}

struct CallbackBinding {
  context: Box<CallbackContext>,
  closure: Closure<'static>,
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
  callbacks: RefCell<HashMap<usize, CallbackBinding>>,
}

fn pointer_from_bigint(pointer: &BigInt) -> Result<usize> {
  let (signed, raw, lossless) = pointer.get_u64();
  if signed || !lossless {
    return Err(Error::new(
      Status::InvalidArg,
      "The pointer must be a non-negative bigint".to_owned(),
    ));
  }
  usize::try_from(raw).map_err(|_| {
    Error::new(
      Status::InvalidArg,
      "The pointer exceeds the platform address range".to_owned(),
    )
  })
}

fn callback_not_found() -> Error {
  Error::new(Status::InvalidArg, "Callback not found".to_owned())
}

fn callback_pointer_from_closure(closure: &Closure<'_>) -> Result<BigInt> {
  let code_ptr = CodePtr::from_fun(*closure.code_ptr());
  let raw = code_ptr.as_mut_ptr() as usize;
  let raw = u64::try_from(raw)
    .map_err(|_| Error::new(Status::GenericFailure, "Callback pointer exceeds u64 range".to_owned()))?;
  Ok(BigInt::from(raw))
}

fn rebuild_callback_cif(signature: &CompiledCallbackSignature) -> Cif {
  Cif::new(
    signature
      .args
      .iter()
      .map(|target| target.ffi_type())
      .collect::<Vec<_>>(),
    signature.ret.ffi_type(),
  )
}

#[napi]
impl DynamicLibrary {
  #[napi(constructor)]
  pub fn new(path: Option<String>) -> Result<Self> {
    let (path_value, library) = match path {
      Some(path_value) => {
        let library = unsafe { Library::new(&path_value) }
          .map_err(|error| Error::new(Status::GenericFailure, format!("dlopen failed: {error}")))?;
        (Some(path_value), library)
      }
      None => {
        #[cfg(unix)]
        let library: Library = UnixLibrary::this().into();
        #[cfg(windows)]
        let library: Library = WindowsLibrary::this()
          .map(Into::into)
          .map_err(|error| Error::new(Status::GenericFailure, format!("dlopen failed: {error}")))?;
        (None, library)
      }
    };
    Ok(Self {
      path: path_value,
      library: Some(library),
      functions: RefCell::new(HashMap::new()),
      callbacks: RefCell::new(HashMap::new()),
    })
  }

  #[napi(getter)]
  pub fn path(&self) -> Option<String> {
    self.path.clone()
  }

  fn library(&self) -> Result<&Library> {
    self
      .library
      .as_ref()
      .ok_or_else(|| Error::new(Status::GenericFailure, "Library is closed".to_owned()))
  }

  fn ensure_open(&self) -> Result<()> {
    self.library().map(|_| ())
  }

  #[napi]
  pub fn close(&mut self) {
    self.functions.borrow_mut().clear();
    self.callbacks.borrow_mut().clear();
    self.library = None;
  }

  #[napi]
  pub fn get_symbol(&self, symbol: String) -> Result<BigInt> {
    let library = self.library()?;
    let pointer = unsafe {
      let raw = library
        .get::<*mut c_void>(symbol.as_bytes())
        .map_err(|error| Error::new(Status::GenericFailure, format!("dlsym failed: {error}")))?;
      *raw as usize as u64
    };
    Ok(BigInt::from(pointer))
  }

  #[napi]
  pub fn get_function(&self, symbol: String, definition: Object) -> Result<CallSpec> {
    let compiled = compile_function_signature(definition)?;
    let pointer = {
      let mut cache = self.functions.borrow_mut();
      if let Some(existing) = cache.get(&symbol) {
        existing.pointer
      } else {
        let library = self.library()?;
        let raw_ptr = unsafe {
          let raw = library
            .get::<*mut c_void>(symbol.as_bytes())
            .map_err(|error| Error::new(Status::GenericFailure, format!("dlsym failed: {error}")))?;
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
    let binding = cache
      .get(&symbol)
      .expect("binding must exist after insertion");
    Ok(CallSpec {
      pointer: BigInt::from(pointer as u64),
      arguments: binding.signature.argument_type_names(),
      result: binding.signature.result_type_name(),
      key: symbol,
    })
  }

  #[napi]
  pub fn register_callback(&self, env: &Env, definition: Option<Object>, callback: Option<Unknown>) -> Result<BigInt> {
    self.ensure_open()?;
    let definition = definition.ok_or_else(|| {
      Error::new(Status::InvalidArg, "Callback signature must be an object".to_owned())
    })?;
    let callback = callback.ok_or_else(|| {
      Error::new(Status::InvalidArg, "Callback must be a function".to_owned())
    })?;
    let compiled = compile_callback_signature(definition)?;
    let function = callback.create_ref()?;
    let mut context = Box::new(CallbackContext {
      env: env.raw(),
      signature: compiled,
      function,
    });
    let closure = Closure::new_mut(rebuild_callback_cif(&context.signature), callback_adapter, unsafe {
      &mut *(&mut *context as *mut CallbackContext)
    });
    let pointer = callback_pointer_from_closure(&closure)?;
    let key = pointer_from_bigint(&pointer)?;
    let context = unsafe { Box::from_raw(Box::into_raw(context)) };
    let closure = unsafe { std::mem::transmute::<Closure<'_>, Closure<'static>>(closure) };
    self.callbacks.borrow_mut().insert(key, CallbackBinding { context, closure });
    Ok(pointer)
  }

  #[napi]
  pub fn unregister_callback(&self, pointer: BigInt) -> Result<()> {
    self.ensure_open()?;
    let pointer = pointer_from_bigint(&pointer)?;
    if self.callbacks.borrow_mut().remove(&pointer).is_some() {
      Ok(())
    } else {
      Err(callback_not_found())
    }
  }

  #[napi]
  pub fn ref_callback(&self, pointer: BigInt) -> Result<()> {
    self.ensure_open()?;
    let pointer = pointer_from_bigint(&pointer)?;
    if self.callbacks.borrow().contains_key(&pointer) {
      Ok(())
    } else {
      Err(callback_not_found())
    }
  }

  #[napi]
  pub fn unref_callback(&self, pointer: BigInt) -> Result<()> {
    self.ensure_open()?;
    let pointer = pointer_from_bigint(&pointer)?;
    if self.callbacks.borrow().contains_key(&pointer) {
      Ok(())
    } else {
      Err(callback_not_found())
    }
  }

  #[napi]
  pub fn invoke<'env>(
    &self,
    env: &'env Env,
    key: String,
    pointer: BigInt,
    values: Vec<Unknown<'env>>,
  ) -> Result<Unknown<'env>> {
    let pointer = pointer_from_bigint(&pointer)?;
    let cache = self.functions.borrow();
    let binding = cache.get(&key).ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        format!("Function '{key}' is not defined"),
      )
    })?;
    if binding.pointer != pointer {
      return Err(Error::new(
        Status::InvalidArg,
        format!("Function '{key}' pointer does not match compiled definition"),
      ));
    }
    if values.len() != binding.signature.args.len() {
      return Err(Error::new(
        Status::InvalidArg,
        format!(
          "Invalid argument count: expected {}, got {}",
          binding.signature.args.len(),
          values.len()
        ),
      ));
    }

    let prepared = binding
      .signature
      .args
      .iter()
      .zip(values)
      .enumerate()
      .map(|(index, (target, value))| {
        let prepared = PreparedFunctionArg::new(target.as_ref())?;
        unsafe { target.formalize_function_arg(env, value, index, prepared.as_ptr())? };
        Ok(prepared)
      })
      .collect::<Result<Vec<_>>>()?;

    let ffi_args = prepared
      .iter()
      .map(|prepared| unsafe { prepared.as_arg() })
      .collect::<Vec<Arg<'_>>>();

    unsafe {
      binding.signature.ret.formalize_function_return(
        env,
        &binding.signature.cif,
        CodePtr(pointer as *mut _),
        &ffi_args,
      )
    }
  }

  #[napi]
  pub fn get_functions<'env>(
    &self,
    env: &'env Env,
    definitions: Object<'env>,
  ) -> Result<Object<'env>> {
    let mut output = Object::new(env)?;
    for name in Object::keys(&definitions)? {
      let definition = definitions.get::<Object>(name.as_str())?.ok_or_else(|| {
        Error::new(
          Status::InvalidArg,
          format!("Missing definition for symbol '{name}'"),
        )
      })?;
      let function = self.get_function(name.clone(), definition)?;
      output.set(name.as_str(), function)?;
    }
    Ok(output)
  }
}

#[napi]
pub fn dlopen<'env>(
  env: &'env Env,
  path: Option<String>,
  definitions: Option<Object<'env>>,
) -> Result<Object<'env>> {
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
