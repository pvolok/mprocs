//! Config parsing utilities for manual YAML -> Rust config loading.
//!
//! Provides:
//! - [`CfgDoc`] — YAML document with write-back support and `$js`/`$select` pre-resolution
//! - [`CfgNode`] / [`CfgObj`] / [`CfgArr`] — typed accessors with path tracking for errors
//! - [`FromCfg`] / [`IntoCfg`] — conversion traits between Rust types and YAML values
//! - [`CfgCx`] — parsing context (config dir, optional JS evaluator)
//!
//! # Usage
//! ```ignore
//! let cx = CfgCx::new(config_dir);
//! let doc = CfgDoc::load(path, &cx)?;
//! let root = doc.root().as_obj()?;
//!
//! let name: String = root.required("name", &cx)?;
//! let port: usize = root.default("port", 8080, &cx)?;
//! let tags: Option<Vec<String>> = root.optional("tags", &cx)?;
//!
//! // Write-back: modify and save
//! doc.set_at(&CfgPath::root().join("port"), 9090.into_cfg());
//! doc.save()?;
//! ```

use std::ffi::OsString;
use std::fmt::{self, Display};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use indexmap::IndexMap;
use serde_yaml::Value;

#[derive(Clone, Debug, Default)]
pub struct CfgPath(Vec<String>);

impl CfgPath {
  pub fn root() -> Self {
    Self(Vec::new())
  }

  /// Append a segment, returning a new child path.
  pub fn join(&self, segment: impl ToString) -> Self {
    let mut segs = self.0.clone();
    segs.push(segment.to_string());
    Self(segs)
  }

  /// Path segments for navigating the YAML tree during write-back.
  pub fn segments(&self) -> &[String] {
    &self.0
  }
}

impl Display for CfgPath {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "<config>")?;
    for seg in &self.0 {
      write!(f, ".{}", seg)?;
    }
    Ok(())
  }
}

/// Context provided during config loading.
pub struct CfgCx {
  /// Parent directory of the config file, for resolving `<CONFIG_DIR>`.
  pub config_dir: PathBuf,
  /// Optional JS evaluator for `{"$js": "..."}` directives.
  /// Receives the source string, must return the evaluated YAML value.
  pub js_eval: Option<Box<dyn Fn(&str) -> Result<Value>>>,
}

impl CfgCx {
  pub fn new(config_dir: PathBuf) -> Self {
    Self {
      config_dir,
      js_eval: None,
    }
  }

  /// Resolve a `<CONFIG_DIR>` prefix in a path string.
  pub fn resolve_path(&self, path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("<CONFIG_DIR>") {
      let mut buf = self.config_dir.clone();
      buf.push(rest.trim_start_matches(['/', '\\']));
      buf
    } else {
      PathBuf::from(path)
    }
  }
}

/// Pre-process a YAML value tree, evaluating `$js` and `$select` directives.
///
/// - `{"$js": "cx => expr"}` — calls `cx.js_eval` and substitutes the result.
/// - `{"$select": "os", "linux": ..., "$else": ...}` — picks a branch by OS.
pub fn resolve_directives(value: &Value, cx: &CfgCx) -> Result<Value> {
  match value {
    Value::Mapping(map) => {
      if let Some(js_src) = map.get(&Value::from("$js")) {
        let src = js_src
          .as_str()
          .ok_or_else(|| anyhow::anyhow!("$js value must be a string"))?;
        if let Some(eval) = &cx.js_eval {
          let result = eval(src)?;
          return resolve_directives(&result, cx);
        } else {
          bail!("$js directive found but no JS evaluator is configured");
        }
      }

      // $select directive (first key must be "$select")
      if map
        .iter()
        .next()
        .is_some_and(|(k, _)| k.as_str() == Some("$select"))
      {
        let selected = resolve_select(map)?;
        return resolve_directives(selected, cx);
      }

      // Recurse into mapping values
      let mut result = serde_yaml::Mapping::new();
      for (k, v) in map {
        result.insert(k.clone(), resolve_directives(v, cx)?);
      }
      Ok(Value::Mapping(result))
    }
    Value::Sequence(seq) => {
      let items = seq
        .iter()
        .map(|v| resolve_directives(v, cx))
        .collect::<Result<Vec<_>>>()?;
      Ok(Value::Sequence(items))
    }
    other => Ok(other.clone()),
  }
}

fn resolve_select<'a>(map: &'a serde_yaml::Mapping) -> Result<&'a Value> {
  let selector = map
    .get(&Value::from("$select"))
    .and_then(|v| v.as_str())
    .ok_or_else(|| anyhow::anyhow!("$select value must be a string"))?;

  match selector {
    "os" => {
      let os = std::env::consts::OS;
      if let Some(v) = map.get(&Value::from(os)) {
        return Ok(v);
      }
      if let Some(v) = map.get(&Value::from("$else")) {
        return Ok(v);
      }
      bail!(
        "No match for OS '{}' in $select. Use \"$else\" for a default.",
        os
      )
    }
    other => bail!("Unknown $select kind: '{}'", other),
  }
}

/// A config document that supports both parsing and write-back.
///
/// Maintains two YAML trees:
/// - **source** — the original document (with `$js`/`$select` intact), used when saving.
/// - **resolved** — directives evaluated, used for parsing into Rust types.
pub struct CfgDoc {
  source: Value,
  resolved: Value,
  pub file_path: PathBuf,
}

impl CfgDoc {
  /// Create from a pre-parsed YAML value.
  pub fn from_value(
    source: Value,
    file_path: PathBuf,
    cx: &CfgCx,
  ) -> Result<Self> {
    let resolved = resolve_directives(&source, cx)?;
    Ok(Self {
      source,
      resolved,
      file_path,
    })
  }

  /// Load and resolve a YAML config file.
  pub fn load(path: &Path, cx: &CfgCx) -> Result<Self> {
    let content = std::fs::read_to_string(path)?;
    let source: Value = serde_yaml::from_str(&content)?;
    Self::from_value(source, path.to_path_buf(), cx)
  }

  /// Root node of the resolved tree, ready for parsing.
  pub fn root(&self) -> CfgNode<'_> {
    CfgNode::new(&self.resolved, CfgPath::root())
  }

  /// Write the source document back to its file.
  pub fn save(&self) -> Result<()> {
    let yaml = serde_yaml::to_string(&self.source)?;
    std::fs::write(&self.file_path, yaml)?;
    Ok(())
  }

  /// Update a value at a path in both source and resolved trees.
  /// Use this from TUI to persist a config change.
  pub fn set_at(&mut self, path: &CfgPath, value: Value) {
    set_at_path(&mut self.source, path.segments(), value.clone());
    set_at_path(&mut self.resolved, path.segments(), value);
  }

  /// Access the original source tree.
  pub fn source(&self) -> &Value {
    &self.source
  }
}

fn set_at_path(root: &mut Value, segments: &[String], value: Value) {
  if segments.is_empty() {
    *root = value;
    return;
  }
  let (key, rest) = segments.split_first().unwrap();
  match root {
    Value::Mapping(map) => {
      let yaml_key = Value::from(key.as_str());
      if rest.is_empty() {
        map.insert(yaml_key, value);
      } else {
        if map.get(&yaml_key).is_none() {
          map.insert(
            yaml_key.clone(),
            Value::Mapping(serde_yaml::Mapping::new()),
          );
        }
        if let Some(child) = map.get_mut(&yaml_key) {
          set_at_path(child, rest, value);
        }
      }
    }
    Value::Sequence(seq) => {
      if let Ok(idx) = key.parse::<usize>() {
        if let Some(elem) = seq.get_mut(idx) {
          if rest.is_empty() {
            *elem = value;
          } else {
            set_at_path(elem, rest, value);
          }
        }
      }
    }
    _ => {}
  }
}

/// Reference to a resolved config value with path tracking for error messages.
#[derive(Clone)]
pub struct CfgNode<'a> {
  value: &'a Value,
  path: CfgPath,
}

impl<'a> CfgNode<'a> {
  pub fn new(value: &'a Value, path: CfgPath) -> Self {
    Self { value, path }
  }

  pub fn path(&self) -> &CfgPath {
    &self.path
  }

  pub fn raw(&self) -> &'a Value {
    self.value
  }

  /// Create an error anchored at this node's position.
  pub fn error(&self, msg: impl Display) -> anyhow::Error {
    anyhow::anyhow!("{} at {}", msg, self.path)
  }

  // Type checks

  pub fn is_null(&self) -> bool {
    self.value.is_null()
  }
  pub fn is_string(&self) -> bool {
    self.value.is_string()
  }
  pub fn is_mapping(&self) -> bool {
    self.value.is_mapping()
  }
  pub fn is_sequence(&self) -> bool {
    self.value.is_sequence()
  }

  // Primitive access

  pub fn as_str(&self) -> Result<&'a str> {
    self
      .value
      .as_str()
      .ok_or_else(|| self.error("expected string"))
  }

  pub fn as_bool(&self) -> Result<bool> {
    self
      .value
      .as_bool()
      .ok_or_else(|| self.error("expected bool"))
  }

  pub fn as_u64(&self) -> Result<u64> {
    self
      .value
      .as_u64()
      .ok_or_else(|| self.error("expected unsigned integer"))
  }

  pub fn as_i64(&self) -> Result<i64> {
    self
      .value
      .as_i64()
      .ok_or_else(|| self.error("expected integer"))
  }

  pub fn as_f64(&self) -> Result<f64> {
    self
      .value
      .as_f64()
      .ok_or_else(|| self.error("expected number"))
  }

  pub fn as_usize(&self) -> Result<usize> {
    self.as_u64().map(|v| v as usize)
  }

  // Composite access

  pub fn as_obj(&self) -> Result<CfgObj<'a>> {
    let map = self
      .value
      .as_mapping()
      .ok_or_else(|| self.error("expected object"))?;
    Ok(CfgObj {
      map,
      path: self.path.clone(),
    })
  }

  pub fn as_arr(&self) -> Result<CfgArr<'a>> {
    let seq = self
      .value
      .as_sequence()
      .ok_or_else(|| self.error("expected array"))?;
    Ok(CfgArr {
      seq: seq.as_slice(),
      path: self.path.clone(),
    })
  }

  /// Parse this node into a Rust type via [`FromCfg`].
  pub fn parse<T: FromCfg>(&self, cx: &CfgCx) -> Result<T> {
    T::from_cfg(self, cx)
  }
}

pub struct CfgObj<'a> {
  map: &'a serde_yaml::Mapping,
  path: CfgPath,
}

impl<'a> CfgObj<'a> {
  /// Look up a key, returning a [`CfgNode`] if present.
  pub fn get(&self, key: &str) -> Option<CfgNode<'a>> {
    self.map.get(&Value::from(key)).map(|v| CfgNode {
      value: v,
      path: self.path.join(key),
    })
  }

  /// Parse a required field. Errors if the key is missing.
  pub fn required<T: FromCfg>(&self, key: &str, cx: &CfgCx) -> Result<T> {
    match self.get(key) {
      Some(node) => T::from_cfg(&node, cx),
      None => bail!("missing required field '{}' at {}", key, self.path),
    }
  }

  /// Parse an optional field. Returns `None` if missing or null.
  pub fn optional<T: FromCfg>(
    &self,
    key: &str,
    cx: &CfgCx,
  ) -> Result<Option<T>> {
    match self.get(key) {
      Some(node) if !node.is_null() => Ok(Some(T::from_cfg(&node, cx)?)),
      _ => Ok(None),
    }
  }

  /// Parse a field with a fallback. Returns `default` if missing or null.
  pub fn default<T: FromCfg>(
    &self,
    key: &str,
    default: T,
    cx: &CfgCx,
  ) -> Result<T> {
    match self.get(key) {
      Some(node) if !node.is_null() => T::from_cfg(&node, cx),
      _ => Ok(default),
    }
  }

  /// Iterate over string-keyed entries. Non-string keys are silently skipped.
  pub fn iter(&self) -> impl Iterator<Item = (&'a str, CfgNode<'a>)> + '_ {
    self.map.iter().filter_map(move |(k, v)| {
      let key = k.as_str()?;
      Some((
        key,
        CfgNode {
          value: v,
          path: self.path.join(key),
        },
      ))
    })
  }

  pub fn path(&self) -> &CfgPath {
    &self.path
  }

  pub fn error(&self, msg: impl Display) -> anyhow::Error {
    anyhow::anyhow!("{} at {}", msg, self.path)
  }
}

pub struct CfgArr<'a> {
  seq: &'a [Value],
  path: CfgPath,
}

impl<'a> CfgArr<'a> {
  pub fn len(&self) -> usize {
    self.seq.len()
  }

  pub fn is_empty(&self) -> bool {
    self.seq.is_empty()
  }

  pub fn get(&self, index: usize) -> Option<CfgNode<'a>> {
    self.seq.get(index).map(|v| CfgNode {
      value: v,
      path: self.path.join(index),
    })
  }

  pub fn iter(&self) -> impl Iterator<Item = CfgNode<'a>> + '_ {
    self.seq.iter().enumerate().map(move |(i, v)| CfgNode {
      value: v,
      path: self.path.join(i),
    })
  }

  /// Parse all elements as `T`.
  pub fn collect<T: FromCfg>(&self, cx: &CfgCx) -> Result<Vec<T>> {
    self.iter().map(|node| T::from_cfg(&node, cx)).collect()
  }

  pub fn path(&self) -> &CfgPath {
    &self.path
  }
}

//
// FromCfg
//

pub trait FromCfg: Sized {
  fn from_cfg(node: &CfgNode<'_>, cx: &CfgCx) -> Result<Self>;
}

// Primitive impls

impl FromCfg for String {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> Result<Self> {
    Ok(node.as_str()?.to_owned())
  }
}

impl FromCfg for bool {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> Result<Self> {
    node.as_bool()
  }
}

impl FromCfg for usize {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> Result<Self> {
    node.as_usize()
  }
}

impl FromCfg for u64 {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> Result<Self> {
    node.as_u64()
  }
}

impl FromCfg for i64 {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> Result<Self> {
    node.as_i64()
  }
}

impl FromCfg for f64 {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> Result<Self> {
    node.as_f64()
  }
}

impl FromCfg for PathBuf {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> Result<Self> {
    Ok(PathBuf::from(node.as_str()?))
  }
}

impl FromCfg for OsString {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> Result<Self> {
    Ok(OsString::from(node.as_str()?))
  }
}

impl FromCfg for Value {
  fn from_cfg(node: &CfgNode<'_>, _cx: &CfgCx) -> Result<Self> {
    Ok(node.raw().clone())
  }
}

// Composite impls

impl<T: FromCfg> FromCfg for Vec<T> {
  fn from_cfg(node: &CfgNode<'_>, cx: &CfgCx) -> Result<Self> {
    node.as_arr()?.collect(cx)
  }
}

impl<T: FromCfg> FromCfg for Option<T> {
  fn from_cfg(node: &CfgNode<'_>, cx: &CfgCx) -> Result<Self> {
    if node.is_null() {
      Ok(None)
    } else {
      Ok(Some(T::from_cfg(node, cx)?))
    }
  }
}

impl<T: FromCfg> FromCfg for IndexMap<String, T> {
  fn from_cfg(node: &CfgNode<'_>, cx: &CfgCx) -> Result<Self> {
    let obj = node.as_obj()?;
    obj
      .iter()
      .map(|(k, v)| Ok((k.to_owned(), T::from_cfg(&v, cx)?)))
      .collect()
  }
}

//
// IntoCfg
//

pub trait IntoCfg {
  fn into_cfg(&self) -> Value;
}

impl IntoCfg for String {
  fn into_cfg(&self) -> Value {
    Value::String(self.clone())
  }
}

impl IntoCfg for &str {
  fn into_cfg(&self) -> Value {
    Value::String(self.to_string())
  }
}

impl IntoCfg for bool {
  fn into_cfg(&self) -> Value {
    Value::Bool(*self)
  }
}

impl IntoCfg for usize {
  fn into_cfg(&self) -> Value {
    Value::Number((*self as u64).into())
  }
}

impl IntoCfg for u64 {
  fn into_cfg(&self) -> Value {
    Value::Number((*self).into())
  }
}

impl IntoCfg for i64 {
  fn into_cfg(&self) -> Value {
    Value::Number((*self).into())
  }
}

impl<T: IntoCfg> IntoCfg for Vec<T> {
  fn into_cfg(&self) -> Value {
    Value::Sequence(self.iter().map(|v| v.into_cfg()).collect())
  }
}

impl<T: IntoCfg> IntoCfg for Option<T> {
  fn into_cfg(&self) -> Value {
    match self {
      Some(v) => v.into_cfg(),
      None => Value::Null,
    }
  }
}

impl<T: IntoCfg> IntoCfg for IndexMap<String, T> {
  fn into_cfg(&self) -> Value {
    let mut map = serde_yaml::Mapping::new();
    for (k, v) in self {
      map.insert(Value::String(k.clone()), v.into_cfg());
    }
    Value::Mapping(map)
  }
}

impl IntoCfg for Value {
  fn into_cfg(&self) -> Value {
    self.clone()
  }
}
