use std::{env::consts::OS, rc::Rc};

use anyhow::bail;
use indexmap::IndexMap;
use serde_yaml::Value;

#[derive(Clone)]
struct Trace(Option<Rc<Box<(String, Trace)>>>);

impl Trace {
  pub fn empty() -> Self {
    Trace(None)
  }

  pub fn add<T: ToString>(&self, seg: T) -> Self {
    Trace(Some(Rc::new(Box::new((seg.to_string(), self.clone())))))
  }

  pub fn to_string(&self) -> String {
    let mut str = String::new();
    fn add(buf: &mut String, trace: &Trace) {
      match &trace.0 {
        Some(part) => {
          add(buf, &part.1);
          buf.push('.');
          buf.push_str(&part.0);
        }
        None => buf.push_str("<config>"),
      }
    }
    add(&mut str, self);

    str
  }
}

pub struct Val<'a>(&'a Value, Trace);

impl<'a> Val<'a> {
  pub fn new(value: &'a Value) -> anyhow::Result<Self> {
    Self::create(value, Trace::empty())
  }

  fn create(value: &'a Value, trace: Trace) -> anyhow::Result<Self> {
    match value {
      Value::Mapping(map) => {
        if map
          .into_iter()
          .next()
          .map_or(false, |(k, _)| k.eq("$select"))
        {
          let (v, t) = Self::select(map, trace.clone())?;
          return Self::create(v, t);
        }
      }
      _ => (),
    }
    Ok(Val(value, trace))
  }

  pub fn raw(&self) -> &Value {
    self.0
  }

  fn select(
    map: &'a serde_yaml::Mapping,
    trace: Trace,
  ) -> anyhow::Result<(&'a Value, Trace)> {
    if map.get(&Value::from("$select")).unwrap() == "os" {
      if let Some(v) = map.get(&Value::from(OS)) {
        return Ok((v, trace.add(OS)));
      }

      if let Some(v) = map.get(&Value::from("$else")) {
        return Ok((v, trace.add("$else")));
      }

      anyhow::bail!(
        "No matching condition found at {}. Use \"$else\" for default value.",
        trace.to_string(),
      )
    } else {
      anyhow::bail!("Expected \"os\" at {}", trace.add("$select").to_string())
    }
  }

  pub fn error_at<T: AsRef<str>>(&self, msg: T) -> anyhow::Error {
    anyhow::format_err!("{} at {}", msg.as_ref(), self.1.to_string())
  }

  pub fn as_bool(&self) -> anyhow::Result<bool> {
    self.0.as_bool().ok_or_else(|| {
      anyhow::format_err!("Expected bool at {}", self.1.to_string())
    })
  }

  pub fn as_usize(&self) -> anyhow::Result<usize> {
    self
      .0
      .as_u64()
      .ok_or_else(|| {
        anyhow::format_err!("Expected int at {}", self.1.to_string())
      })
      .map(|x| x as usize)
  }

  pub fn as_str(&self) -> anyhow::Result<&str> {
    self.0.as_str().ok_or_else(|| {
      anyhow::format_err!("Expected string at {}", self.1.to_string())
    })
  }

  pub fn as_array(&self) -> anyhow::Result<Vec<Val>> {
    self
      .0
      .as_sequence()
      .ok_or_else(|| {
        anyhow::format_err!("Expected array at {}", self.1.to_string())
      })?
      .iter()
      .enumerate()
      .map(|(i, item)| Val::create(item, self.1.add(i)))
      .collect::<anyhow::Result<Vec<_>>>()
  }

  pub fn as_object(&self) -> anyhow::Result<IndexMap<Value, Val>> {
    self
      .0
      .as_mapping()
      .ok_or_else(|| {
        anyhow::format_err!("Expected object at {}", self.1.to_string())
      })?
      .iter()
      .map(|(k, item)| {
        #[inline]
        fn mk_val<'a>(
          k: &'a Value,
          item: &'a Value,
          trace: &'a Trace,
        ) -> anyhow::Result<Val<'a>> {
          Ok(Val::create(item, trace.add(value_to_string(k)?))?)
        }
        Ok((k.to_owned(), mk_val(k, item, &self.1)?))
      })
      .collect::<anyhow::Result<IndexMap<_, _>>>()
  }
}

pub fn value_to_string(value: &Value) -> anyhow::Result<String> {
  match value {
    Value::Null => Ok("null".to_string()),
    Value::Bool(v) => Ok(v.to_string()),
    Value::Number(v) => Ok(v.to_string()),
    Value::String(v) => Ok(v.to_string()),
    Value::Sequence(_v) => {
      bail!("`primitive_to_string` is not implemented for arrays.")
    }
    Value::Mapping(_v) => {
      bail!("`primitive_to_string` is not implemented for objects.")
    }
  }
}
