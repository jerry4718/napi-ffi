use libffi::middle::Cif;
use napi::bindgen_prelude::*;

use crate::targets::{
  ArrayBufferTarget, BufferTarget, F32Target, F64Target, FunctionTarget, I16Target, I32Target,
  I64Target, I8Target, PointerTarget, StringTarget, TypedTarget, U16Target, U32Target, U64Target,
  U8Target, VoidTarget,
};

pub struct CompiledFunctionSignature {
  pub args: Vec<Box<dyn TypedTarget>>,
  pub ret: Box<dyn TypedTarget>,
  pub cif: Cif,
}

pub struct CompiledCallbackSignature {
  pub args: Vec<Box<dyn TypedTarget>>,
  pub ret: Box<dyn TypedTarget>,
  pub cif: Cif,
}

impl CompiledFunctionSignature {
  pub fn argument_type_names(&self) -> Vec<String> {
    self
      .args
      .iter()
      .map(|target| target.type_name().to_owned())
      .collect()
  }

  pub fn result_type_name(&self) -> String {
    self.ret.type_name().to_owned()
  }
}

impl CompiledCallbackSignature {
  pub fn argument_type_names(&self) -> Vec<String> {
    self
      .args
      .iter()
      .map(|target| target.type_name().to_owned())
      .collect()
  }

  pub fn result_type_name(&self) -> String {
    self.ret.type_name().to_owned()
  }
}

fn parse_target(type_name: &str) -> Result<Box<dyn TypedTarget>> {
  match type_name {
    "void" => Ok(Box::new(VoidTarget)),
    "char" => {
      if (std::ffi::c_char::MIN as i32) < 0 {
        Ok(Box::new(I8Target))
      } else {
        Ok(Box::new(U8Target))
      }
    }
    "i8" | "int8" => Ok(Box::new(I8Target)),
    "bool" | "u8" | "uint8" => Ok(Box::new(U8Target)),
    "i16" | "int16" => Ok(Box::new(I16Target)),
    "u16" | "uint16" => Ok(Box::new(U16Target)),
    "i32" | "int32" => Ok(Box::new(I32Target)),
    "u32" | "uint32" => Ok(Box::new(U32Target)),
    "i64" | "int64" => Ok(Box::new(I64Target)),
    "u64" | "uint64" => Ok(Box::new(U64Target)),
    "f32" | "float" | "float32" => Ok(Box::new(F32Target)),
    "f64" | "double" | "float64" => Ok(Box::new(F64Target)),
    "pointer" | "ptr" => Ok(Box::new(PointerTarget)),
    "buffer" => Ok(Box::new(BufferTarget)),
    "arraybuffer" => Ok(Box::new(ArrayBufferTarget)),
    "function" => Ok(Box::new(FunctionTarget)),
    "string" | "str" => Ok(Box::new(StringTarget)),
    _ => Err(Error::new(
      Status::InvalidArg,
      format!("Unsupported FFI type: {type_name}"),
    )),
  }
}

pub fn compile_function_signature(definition: Object) -> Result<CompiledFunctionSignature> {
  let ret = definition
    .get::<String>("returns")?
    .or(definition.get::<String>("return")?)
    .or(definition.get::<String>("result")?)
    .unwrap_or_else(|| "void".to_owned());

  let args = definition
    .get::<Vec<String>>("parameters")?
    .or(definition.get::<Vec<String>>("arguments")?)
    .unwrap_or_default();

  let compiled_args = args
    .iter()
    .map(|name| parse_target(name))
    .collect::<Result<Vec<_>>>()?;
  let compiled_ret = parse_target(&ret)?;
  let cif = Cif::new(
    compiled_args
      .iter()
      .map(|target| target.ffi_type())
      .collect::<Vec<_>>(),
    compiled_ret.ffi_type(),
  );

  Ok(CompiledFunctionSignature {
    args: compiled_args,
    ret: compiled_ret,
    cif,
  })
}

pub fn compile_callback_signature(definition: Object) -> Result<CompiledCallbackSignature> {
  let ret = definition
    .get::<String>("returns")?
    .or(definition.get::<String>("return")?)
    .or(definition.get::<String>("result")?)
    .unwrap_or_else(|| "void".to_owned());

  let args = definition
    .get::<Vec<String>>("parameters")?
    .or(definition.get::<Vec<String>>("arguments")?)
    .unwrap_or_default();

  let compiled_args = args
    .iter()
    .map(|name| parse_target(name))
    .collect::<Result<Vec<_>>>()?;
  let compiled_ret = parse_target(&ret)?;

  let cif = Cif::new(
    compiled_args
      .iter()
      .map(|target| target.ffi_type())
      .collect::<Vec<_>>(),
    compiled_ret.ffi_type(),
  );

  Ok(CompiledCallbackSignature {
    args: compiled_args,
    ret: compiled_ret,
    cif,
  })
}
