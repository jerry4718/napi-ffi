use libffi::middle::{Cif, Type as FFIType};
use napi::bindgen_prelude::*;

use crate::targets::{
  TypeOps, F32_OPS, F64_OPS, I16_OPS, I32_OPS, I64_OPS, I8_OPS, POINTER_OPS, STRING_OPS, U16_OPS,
  U32_OPS, U64_OPS, U8_OPS, VOID_OPS,
};

pub struct CompiledTarget {
  pub type_name: &'static str,
  pub ffi_type: FFIType,
  pub ops: &'static TypeOps,
}

pub struct CompiledSignature {
  pub args: Vec<CompiledTarget>,
  pub ret: CompiledTarget,
  pub cif: Cif,
}

impl CompiledSignature {
  pub fn argument_type_names(&self) -> Vec<String> {
    self
      .args
      .iter()
      .map(|target| target.type_name.to_owned())
      .collect()
  }

  pub fn result_type_name(&self) -> String {
    self.ret.type_name.to_owned()
  }
}

fn compiled(type_name: &'static str, ffi_type: FFIType, ops: &'static TypeOps) -> CompiledTarget {
  CompiledTarget {
    type_name,
    ffi_type,
    ops,
  }
}

fn parse_target(type_name: &str) -> Result<CompiledTarget> {
  match type_name {
    "void" => Ok(compiled("void", FFIType::void(), &VOID_OPS)),
    "i8" | "int8" => Ok(compiled("int8", FFIType::i8(), &I8_OPS)),
    "u8" | "uint8" | "bool" => Ok(compiled("uint8", FFIType::u8(), &U8_OPS)),
    "char" => {
      if std::ffi::c_char::MIN < 0 {
        Ok(compiled("int8", FFIType::i8(), &I8_OPS))
      } else {
        Ok(compiled("uint8", FFIType::u8(), &U8_OPS))
      }
    }
    "i16" | "int16" => Ok(compiled("int16", FFIType::i16(), &I16_OPS)),
    "u16" | "uint16" => Ok(compiled("uint16", FFIType::u16(), &U16_OPS)),
    "i32" | "int32" => Ok(compiled("int32", FFIType::i32(), &I32_OPS)),
    "u32" | "uint32" => Ok(compiled("uint32", FFIType::u32(), &U32_OPS)),
    "i64" | "int64" => Ok(compiled("int64", FFIType::i64(), &I64_OPS)),
    "u64" | "uint64" => Ok(compiled("uint64", FFIType::u64(), &U64_OPS)),
    "f32" | "float" | "float32" => Ok(compiled("float32", FFIType::f32(), &F32_OPS)),
    "f64" | "double" | "float64" => Ok(compiled("float64", FFIType::f64(), &F64_OPS)),
    "pointer" | "ptr" => Ok(compiled("pointer", FFIType::pointer(), &POINTER_OPS)),
    "buffer" => Ok(compiled("buffer", FFIType::pointer(), &POINTER_OPS)),
    "arraybuffer" => Ok(compiled("arraybuffer", FFIType::pointer(), &POINTER_OPS)),
    "function" => Ok(compiled("function", FFIType::pointer(), &POINTER_OPS)),
    "string" | "str" => Ok(compiled("string", FFIType::pointer(), &STRING_OPS)),
    _ => Err(Error::new(
      Status::InvalidArg,
      format!("Unsupported FFI type: {type_name}"),
    )),
  }
}

pub fn compile_signature(definition: Object) -> Result<CompiledSignature> {
  let ret = definition
    .get::<String>("return")?
    .unwrap_or_else(|| "void".to_owned());

  let args = definition
    .get::<Vec<String>>("arguments")?
    .unwrap_or_default();

  let compiled_args = args
    .iter()
    .map(|name| parse_target(name))
    .collect::<Result<Vec<_>>>()?;

  let compiled_ret = parse_target(&ret)?;
  let cif = Cif::new(
    compiled_args
      .iter()
      .map(|target| target.ffi_type.clone())
      .collect::<Vec<_>>(),
    compiled_ret.ffi_type.clone(),
  );

  Ok(CompiledSignature {
    args: compiled_args,
    ret: compiled_ret,
    cif,
  })
}
