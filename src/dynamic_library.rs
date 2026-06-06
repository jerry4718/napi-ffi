use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;

use libffi::middle::{Arg, Closure, CodePtr, Ret};
#[cfg(unix)]
use libloading::os::unix::Library as UnixLibrary;
#[cfg(windows)]
use libloading::os::windows::Library as WindowsLibrary;
use libloading::Library;
use napi::bindgen_prelude::*;
use napi::Env;
use napi_derive::napi;

use crate::signature::{compile_signature, CompiledSignature, CompiledTarget};
use crate::targets::FormalizedStorageScope;

struct RawStorage {
  pointer: NonNull<u8>,
  layout: Layout,
}

impl RawStorage {
  fn new(layout: Layout) -> Result<Self> {
    let pointer = if layout.size() == 0 {
      NonNull::dangling()
    } else {
      let raw = unsafe { alloc(layout) };
      NonNull::new(raw)
        .ok_or_else(|| Error::new(Status::GenericFailure, "Allocation failed".to_owned()))?
    };
    Ok(Self { pointer, layout })
  }

  fn as_mut_ptr(&self) -> *mut u8 {
    self.pointer.as_ptr()
  }
}

impl Drop for RawStorage {
  fn drop(&mut self) {
    if self.layout.size() != 0 {
      unsafe { dealloc(self.pointer.as_ptr(), self.layout) };
    }
  }
}

struct PreparedFunctionArg<'a> {
  target: &'a CompiledTarget,
  storage: RawStorage,
  scope: FormalizedStorageScope,
}

impl<'a> PreparedFunctionArg<'a> {
  fn new(target: &'a CompiledTarget) -> Result<Self> {
    Ok(Self {
      target,
      storage: RawStorage::new((target.ops.function_arg_layout)())?,
      scope: FormalizedStorageScope::new(),
    })
  }

  fn as_ptr(&self) -> *mut u8 {
    self.storage.as_mut_ptr()
  }

  fn scope_mut(&mut self) -> &mut FormalizedStorageScope {
    &mut self.scope
  }

  unsafe fn as_arg(&self) -> Arg<'_> {
    unsafe { (self.target.ops.function_arg_as_ffi_arg)(self.storage.as_mut_ptr()) }
  }
}

struct PreparedCallbackReturn<'a> {
  target: &'a CompiledTarget,
  storage: RawStorage,
  scope: FormalizedStorageScope,
}

impl<'a> PreparedCallbackReturn<'a> {
  fn new(target: &'a CompiledTarget) -> Result<Self> {
    Ok(Self {
      target,
      storage: RawStorage::new((target.ops.callback_return_layout)())?,
      scope: FormalizedStorageScope::new(),
    })
  }

  fn as_mut_ptr(&self) -> *mut u8 {
    self.storage.as_mut_ptr()
  }

  fn scope_mut(&mut self) -> &mut FormalizedStorageScope {
    &mut self.scope
  }

  unsafe fn copy_into_result(&self, result: &mut *mut c_void) {
    let value_ptr = unsafe { (self.target.ops.callback_return_ptr)(self.storage.as_mut_ptr()) };
    let copy_size = (self.target.ops.callback_return_copy_size)();
    if copy_size != 0 {
      let src = value_ptr.cast::<u8>();
      let dst = (result as *mut *mut c_void).cast::<u8>();
      unsafe { std::ptr::copy_nonoverlapping(src, dst, copy_size) };
    }
  }
}

struct FunctionBinding {
  pointer: usize,
  signature: CompiledSignature,
}

#[napi]
pub struct CallbackRef {
  env: sys::napi_env,
  reference: sys::napi_ref,
}

impl CallbackRef {
  fn new(env: &Env, callback: &Function<'_, (), Unknown<'_>>) -> Result<Self> {
    let mut reference = std::ptr::null_mut();
    check_status!(
      unsafe { sys::napi_create_reference(env.raw(), callback.raw(), 1, &mut reference) },
      "Create callback reference failed"
    )?;
    Ok(Self {
      env: env.raw(),
      reference,
    })
  }

  fn get_function<'env>(&self, env: &'env Env) -> Result<Unknown<'env>> {
    let mut value = std::ptr::null_mut();
    check_status!(
      unsafe { sys::napi_get_reference_value(env.raw(), self.reference, &mut value) },
      "Get callback reference value failed"
    )?;
    Ok(unsafe { Unknown::from_raw_unchecked(env.raw(), value) })
  }
}

impl Drop for CallbackRef {
  fn drop(&mut self) {
    let status = unsafe { sys::napi_delete_reference(self.env, self.reference) };
    debug_assert_eq!(
      status,
      sys::Status::napi_ok,
      "Drop callback reference failed"
    );
  }
}

enum CallbackFunctionState {
  Strong(Reference<CallbackRef>),
  Weak(WeakReference<CallbackRef>),
  Collected,
}

struct CallbackContext {
  env: napi::sys::napi_env,
  thread_id: std::thread::ThreadId,
  signature: CompiledSignature,
  function_state: CallbackFunctionState,
}

fn abort_callback(message: &str) -> ! {
  eprintln!("{message}");
  std::process::abort()
}

impl CallbackContext {
  unsafe fn zero_result(&self, result: &mut *mut c_void) {
    unsafe {
      std::ptr::write_bytes(
        (result as *mut *mut c_void).cast::<u8>(),
        0,
        (self.signature.ret.ops.function_arg_layout)().size(),
      )
    };
  }

  unsafe fn invoke(&mut self, result: &mut *mut c_void, args: *const *const c_void) {
    if self.thread_id != std::thread::current().id() {
      abort_callback("Callbacks can only be invoked on the system thread they were created on")
    }
    if let Err(error) = unsafe { self.try_invoke(result, args) } {
      if error.status == Status::InvalidArg {
        abort_callback(&error.reason)
      }
      abort_callback("Callbacks cannot throw an exception")
    }
  }

  unsafe fn try_invoke(
    &mut self,
    result: &mut *mut c_void,
    args: *const *const c_void,
  ) -> Result<()> {
    let env = Env::from_raw(self.env);

    let function_value = match &mut self.function_state {
      CallbackFunctionState::Strong(holder) => holder.get_function(&env)?,
      CallbackFunctionState::Weak(holder) => match holder.upgrade(env)? {
        Some(reference) => reference.get_function(&env)?,
        None => {
          self.function_state = CallbackFunctionState::Collected;
          unsafe { self.zero_result(result) };
          return Ok(());
        }
      },
      CallbackFunctionState::Collected => {
        unsafe { self.zero_result(result) };
        return Ok(());
      }
    };

    let js_args = self
      .signature
      .args
      .iter()
      .enumerate()
      .map(|(index, target)| {
        let arg_ptr = unsafe { *args.add(index) };
        unsafe { (target.ops.formalize_callback_arg)(&env, arg_ptr, index) }
      })
      .collect::<Result<Vec<_>>>()?;

    let raw_args = js_args
      .iter()
      .map(|arg| arg.raw())
      .collect::<Vec<sys::napi_value>>();
    let mut raw_this = std::ptr::null_mut();
    check_status!(
      unsafe { sys::napi_get_undefined(env.raw(), &mut raw_this) },
      "Get undefined value failed"
    )?;
    let mut raw_return = std::ptr::null_mut();
    check_pending_exception!(
      env.raw(),
      unsafe {
        sys::napi_call_function(
          env.raw(),
          raw_this,
          function_value.raw(),
          raw_args.len(),
          raw_args.as_ptr(),
          &mut raw_return,
        )
      },
      "Callbacks cannot throw an exception"
    )?;
    let returned = unsafe { Unknown::from_raw_unchecked(env.raw(), raw_return) };
    if returned.get_type()? == ValueType::Object
      && returned.coerce_to_object()?.has_named_property("then")?
    {
      abort_callback("Callbacks cannot return promises")
    }
    let mut prepared = PreparedCallbackReturn::new(&self.signature.ret)?;
    unsafe {
      let storage = prepared.as_mut_ptr();
      let scope = prepared.scope_mut();
      (self.signature.ret.ops.formalize_callback_return)(&env, returned, storage, scope)?;
      prepared.copy_into_result(result);
    }
    Ok(())
  }
}

impl Drop for CallbackContext {
  fn drop(&mut self) {}
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
  _closure: Closure<'static>,
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
      "The first argument must be a non-negative bigint".to_owned(),
    ));
  }
  usize::try_from(raw).map_err(|_| {
    Error::new(
      Status::InvalidArg,
      "The pointer exceeds the platform pointer range".to_owned(),
    )
  })
}

fn callback_not_found() -> Error {
  Error::new(Status::InvalidArg, "Callback not found".to_owned())
}

fn validate_callback_signature(signature: &CompiledSignature) -> Result<()> {
  if signature.ret.type_name == "string" {
    return Err(Error::new(
      Status::InvalidArg,
      "Callback result type cannot be string; use pointer and manage the returned memory explicitly"
        .to_owned(),
    ));
  }
  Ok(())
}

fn callback_pointer_from_closure(closure: &Closure<'_>) -> Result<BigInt> {
  let code_ptr = CodePtr::from_fun(*closure.code_ptr());
  let raw = code_ptr.as_mut_ptr() as usize;
  let raw = u64::try_from(raw).map_err(|_| {
    Error::new(
      Status::GenericFailure,
      "Callback pointer exceeds u64 range".to_owned(),
    )
  })?;
  Ok(BigInt::from(raw))
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
            .map_err(|error| {
              Error::new(Status::GenericFailure, format!("dlsym failed: {error}"))
            })?;
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
  pub fn register_callback(
    &self,
    env: &Env,
    definition: Option<Object>,
    callback: Option<Function<'_, (), Unknown<'_>>>,
  ) -> Result<BigInt> {
    self.ensure_open()?;
    let definition = definition.ok_or_else(|| {
      Error::new(
        Status::InvalidArg,
        "Callback signature must be an object".to_owned(),
      )
    })?;
    let callback = callback
      .ok_or_else(|| Error::new(Status::InvalidArg, "Callback must be a function".to_owned()))?;
    let compiled = compile_signature(definition)?;
    validate_callback_signature(&compiled)?;
    let holder = CallbackRef::new(env, &callback)?;
    let strong = holder.into_reference(Env::from_raw(env.raw()))?;
    let mut context = Box::new(CallbackContext {
      env: env.raw(),
      thread_id: std::thread::current().id(),
      signature: compiled,
      function_state: CallbackFunctionState::Strong(strong),
    });
    let closure = Closure::new_mut(context.signature.cif.clone(), callback_adapter, unsafe {
      &mut *(&mut *context as *mut CallbackContext)
    });
    let pointer = callback_pointer_from_closure(&closure)?;
    let key = pointer_from_bigint(&pointer)?;
    let context = unsafe { Box::from_raw(Box::into_raw(context)) };
    let closure = unsafe { std::mem::transmute::<Closure<'_>, Closure<'static>>(closure) };
    self.callbacks.borrow_mut().insert(
      key,
      CallbackBinding {
        context,
        _closure: closure,
      },
    );
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
    let mut callbacks = self.callbacks.borrow_mut();
    if let Some(binding) = callbacks.get_mut(&pointer) {
      let env = Env::from_raw(binding.context.env);
      binding.context.function_state = match std::mem::replace(
        &mut binding.context.function_state,
        CallbackFunctionState::Collected,
      ) {
        CallbackFunctionState::Strong(reference) => CallbackFunctionState::Strong(reference),
        CallbackFunctionState::Weak(weak) => match weak.upgrade(env)? {
          Some(reference) => CallbackFunctionState::Strong(reference),
          None => CallbackFunctionState::Collected,
        },
        CallbackFunctionState::Collected => CallbackFunctionState::Collected,
      };
      Ok(())
    } else {
      Err(callback_not_found())
    }
  }

  #[napi]
  pub fn unref_callback(&self, pointer: BigInt) -> Result<()> {
    self.ensure_open()?;
    let pointer = pointer_from_bigint(&pointer)?;
    let mut callbacks = self.callbacks.borrow_mut();
    if let Some(binding) = callbacks.get_mut(&pointer) {
      binding.context.function_state = match std::mem::replace(
        &mut binding.context.function_state,
        CallbackFunctionState::Collected,
      ) {
        CallbackFunctionState::Strong(reference) => {
          CallbackFunctionState::Weak(reference.downgrade())
        }
        CallbackFunctionState::Weak(weak) => CallbackFunctionState::Weak(weak),
        CallbackFunctionState::Collected => CallbackFunctionState::Collected,
      };
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
        let mut prepared = PreparedFunctionArg::new(target)?;
        unsafe {
          let storage = prepared.as_ptr();
          let scope = prepared.scope_mut();
          (target.ops.formalize_function_arg)(env, value, index, storage, scope)?
        };
        Ok(prepared)
      })
      .collect::<Result<Vec<_>>>()?;

    let ffi_args = prepared
      .iter()
      .map(|prepared| unsafe { prepared.as_arg() })
      .collect::<Vec<Arg<'_>>>();

    let return_layout = (binding.signature.ret.ops.function_return_layout)();
    let return_storage = RawStorage::new(return_layout)?;
    let ret = if return_layout.size() == 0 {
      Ret::void()
    } else {
      unsafe { Ret::new(&mut *return_storage.as_mut_ptr()) }
    };

    unsafe {
      binding
        .signature
        .cif
        .call_return_into(CodePtr(pointer as *mut _), &ffi_args, ret);
      (binding.signature.ret.ops.formalize_function_return)(env, return_storage.as_mut_ptr())
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
