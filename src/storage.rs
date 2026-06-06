use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;

use libffi::middle::Arg;
use napi::bindgen_prelude::*;

use crate::signature::CompiledTarget;
use crate::targets::FormalizedStorageScope;

pub(crate) struct RawStorage {
  pointer: NonNull<u8>,
  layout: Layout,
}

impl RawStorage {
  pub(crate) fn new(layout: Layout) -> Result<Self> {
    let pointer = if layout.size() == 0 {
      NonNull::dangling()
    } else {
      let raw = unsafe { alloc(layout) };
      NonNull::new(raw)
        .ok_or_else(|| Error::new(Status::GenericFailure, "Allocation failed".to_owned()))?
    };
    Ok(Self { pointer, layout })
  }

  pub(crate) fn as_mut_ptr(&self) -> *mut u8 {
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

pub(crate) struct PreparedFunctionArg<'a> {
  target: &'a CompiledTarget,
  storage: RawStorage,
  scope: FormalizedStorageScope,
}

impl<'a> PreparedFunctionArg<'a> {
  pub(crate) fn new(target: &'a CompiledTarget) -> Result<Self> {
    Ok(Self {
      target,
      storage: RawStorage::new(target.ops.function_arg_layout)?,
      scope: FormalizedStorageScope::new(),
    })
  }

  pub(crate) fn as_ptr(&self) -> *mut u8 {
    self.storage.as_mut_ptr()
  }

  pub(crate) fn scope_mut(&mut self) -> &mut FormalizedStorageScope {
    &mut self.scope
  }

  pub(crate) unsafe fn as_arg(&self) -> Arg<'_> {
    unsafe { (self.target.ops.function_arg_as_ffi_arg)(self.storage.as_mut_ptr()) }
  }
}

pub(crate) struct PreparedCallbackReturn<'a> {
  target: &'a CompiledTarget,
  storage: RawStorage,
  scope: FormalizedStorageScope,
}

impl<'a> PreparedCallbackReturn<'a> {
  pub(crate) fn new(target: &'a CompiledTarget) -> Result<Self> {
    Ok(Self {
      target,
      storage: RawStorage::new(target.ops.callback_return_layout)?,
      scope: FormalizedStorageScope::new(),
    })
  }

  pub(crate) fn as_mut_ptr(&self) -> *mut u8 {
    self.storage.as_mut_ptr()
  }

  pub(crate) fn scope_mut(&mut self) -> &mut FormalizedStorageScope {
    &mut self.scope
  }

  pub(crate) unsafe fn copy_into_result(&self, result: &mut *mut std::ffi::c_void) {
    let value_ptr = unsafe { (self.target.ops.callback_return_ptr)(self.storage.as_mut_ptr()) };
    let copy_size = self.target.ops.callback_return_copy_size;
    if copy_size != 0 {
      let src = value_ptr.cast::<u8>();
      let dst = (result as *mut *mut std::ffi::c_void).cast::<u8>();
      unsafe { std::ptr::copy_nonoverlapping(src, dst, copy_size) };
    }
  }
}
