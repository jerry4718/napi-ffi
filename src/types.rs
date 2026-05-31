use napi_derive::napi;

#[napi(object)]
pub struct FfiTypes {
  pub void: String,
  pub pointer: String,
  pub buffer: String,
  pub array_buffer: String,
  pub function: String,
  pub bool: String,
  pub char: String,
  pub string: String,
  pub float: String,
  pub double: String,
  pub int_8: String,
  pub uint_8: String,
  pub int_16: String,
  pub uint_16: String,
  pub int_32: String,
  pub uint_32: String,
  pub int_64: String,
  pub uint_64: String,
  pub float_32: String,
  pub float_64: String,
}

fn suffix_value() -> &'static str {
  if cfg!(target_os = "windows") {
    "dll"
  } else if cfg!(target_os = "macos") {
    "dylib"
  } else {
    "so"
  }
}

#[napi]
pub fn types() -> FfiTypes {
  FfiTypes {
    void: "void".to_owned(),
    pointer: "pointer".to_owned(),
    buffer: "buffer".to_owned(),
    array_buffer: "arraybuffer".to_owned(),
    function: "function".to_owned(),
    bool: "bool".to_owned(),
    char: "char".to_owned(),
    string: "string".to_owned(),
    float: "float".to_owned(),
    double: "double".to_owned(),
    int_8: "int8".to_owned(),
    uint_8: "uint8".to_owned(),
    int_16: "int16".to_owned(),
    uint_16: "uint16".to_owned(),
    int_32: "int32".to_owned(),
    uint_32: "uint32".to_owned(),
    int_64: "int64".to_owned(),
    uint_64: "uint64".to_owned(),
    float_32: "float32".to_owned(),
    float_64: "float64".to_owned(),
  }
}

#[napi]
pub fn suffix() -> String {
  suffix_value().to_owned()
}
