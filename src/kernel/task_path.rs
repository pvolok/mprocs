use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(
  Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct TaskPath(String);

#[derive(Debug)]
pub enum TaskPathError {
  Empty,
  NotAbsolute,
  EmptyComponent,
  InvalidCharacter(char),
}

impl fmt::Display for TaskPathError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      TaskPathError::Empty => write!(f, "path is empty"),
      TaskPathError::NotAbsolute => write!(f, "path must start with '/'"),
      TaskPathError::EmptyComponent => {
        write!(f, "path contains empty component")
      }
      TaskPathError::InvalidCharacter(c) => {
        write!(f, "path contains invalid character: '{}'", c)
      }
    }
  }
}

impl std::error::Error for TaskPathError {}

fn is_valid_component_char(c: char) -> bool {
  c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

impl TaskPath {
  pub fn new(s: impl Into<String>) -> Result<Self, TaskPathError> {
    let s = s.into();
    if s.is_empty() {
      return Err(TaskPathError::Empty);
    }
    if !s.starts_with('/') {
      return Err(TaskPathError::NotAbsolute);
    }
    // Root "/" is valid
    if s == "/" {
      return Ok(TaskPath(s));
    }
    for component in s[1..].split('/') {
      if component.is_empty() {
        return Err(TaskPathError::EmptyComponent);
      }
      for c in component.chars() {
        if !is_valid_component_char(c) {
          return Err(TaskPathError::InvalidCharacter(c));
        }
      }
    }
    Ok(TaskPath(s))
  }

  pub fn as_str(&self) -> &str {
    &self.0
  }

  /// Returns the parent path, or None if this is the root `/`.
  pub fn parent(&self) -> Option<TaskPath> {
    if self.0 == "/" {
      return None;
    }
    match self.0.rfind('/') {
      Some(0) => Some(TaskPath("/".to_string())),
      Some(pos) => Some(TaskPath(self.0[..pos].to_string())),
      None => None,
    }
  }

  /// Returns the last component (the "name"), or empty string for root.
  pub fn name(&self) -> &str {
    if self.0 == "/" {
      return "";
    }
    match self.0.rfind('/') {
      Some(pos) => &self.0[pos + 1..],
      None => &self.0,
    }
  }

  /// Returns an iterator over path components (excluding the leading `/`).
  /// Root `/` yields an empty iterator.
  pub fn components(&self) -> impl Iterator<Item = &str> {
    let s = if self.0 == "/" { "" } else { &self.0[1..] };
    s.split('/').filter(|c| !c.is_empty())
  }

  /// Number of components. `/a` = 1, `/a/b` = 2, `/` = 0.
  pub fn depth(&self) -> usize {
    self.components().count()
  }

  /// Test whether this path matches a glob pattern.
  /// Supports `*` (single component wildcard) and `**` (recursive wildcard).
  /// Pattern must start with `/`.
  pub fn matches_glob(&self, pattern: &str) -> bool {
    if !pattern.starts_with('/') {
      return false;
    }
    let path_parts: Vec<&str> = self.components().collect();
    let pat_parts: Vec<&str> =
      pattern[1..].split('/').filter(|c| !c.is_empty()).collect();
    glob_match(&path_parts, &pat_parts)
  }
}

fn glob_match(path: &[&str], pattern: &[&str]) -> bool {
  if pattern.is_empty() {
    return path.is_empty();
  }

  let pat = pattern[0];
  let rest_pat = &pattern[1..];

  if pat == "**" {
    // `**` matches zero or more components
    // Try matching rest of pattern at every position
    for i in 0..=path.len() {
      if glob_match(&path[i..], rest_pat) {
        return true;
      }
    }
    false
  } else if pat == "*" {
    // `*` matches exactly one component
    if path.is_empty() {
      return false;
    }
    glob_match(&path[1..], rest_pat)
  } else {
    // Literal match
    if path.is_empty() || path[0] != pat {
      return false;
    }
    glob_match(&path[1..], rest_pat)
  }
}

impl fmt::Display for TaskPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_valid_paths() {
    assert!(TaskPath::new("/").is_ok());
    assert!(TaskPath::new("/web").is_ok());
    assert!(TaskPath::new("/services/api").is_ok());
    assert!(TaskPath::new("/tools/my-watcher").is_ok());
    assert!(TaskPath::new("/a/b.c/d_e").is_ok());
  }

  #[test]
  fn test_invalid_paths() {
    assert!(TaskPath::new("").is_err());
    assert!(TaskPath::new("web").is_err());
    assert!(TaskPath::new("/web/").is_err());
    assert!(TaskPath::new("//web").is_err());
    assert!(TaskPath::new("/web//api").is_err());
    assert!(TaskPath::new("/web server").is_err());
    assert!(TaskPath::new("/web@home").is_err());
  }

  #[test]
  fn test_parent() {
    assert_eq!(TaskPath::new("/").unwrap().parent(), None);
    assert_eq!(
      TaskPath::new("/web").unwrap().parent().unwrap().as_str(),
      "/"
    );
    assert_eq!(
      TaskPath::new("/services/api")
        .unwrap()
        .parent()
        .unwrap()
        .as_str(),
      "/services"
    );
  }

  #[test]
  fn test_name() {
    assert_eq!(TaskPath::new("/").unwrap().name(), "");
    assert_eq!(TaskPath::new("/web").unwrap().name(), "web");
    assert_eq!(TaskPath::new("/services/api").unwrap().name(), "api");
  }

  #[test]
  fn test_components() {
    let p = TaskPath::new("/").unwrap();
    assert_eq!(p.components().collect::<Vec<_>>(), Vec::<&str>::new());

    let p = TaskPath::new("/web").unwrap();
    assert_eq!(p.components().collect::<Vec<_>>(), vec!["web"]);

    let p = TaskPath::new("/services/api").unwrap();
    assert_eq!(p.components().collect::<Vec<_>>(), vec!["services", "api"]);
  }

  #[test]
  fn test_depth() {
    assert_eq!(TaskPath::new("/").unwrap().depth(), 0);
    assert_eq!(TaskPath::new("/web").unwrap().depth(), 1);
    assert_eq!(TaskPath::new("/a/b/c").unwrap().depth(), 3);
  }

  #[test]
  fn test_glob_exact() {
    let p = TaskPath::new("/services/api").unwrap();
    assert!(p.matches_glob("/services/api"));
    assert!(!p.matches_glob("/services/web"));
    assert!(!p.matches_glob("/services"));
  }

  #[test]
  fn test_glob_star() {
    let p1 = TaskPath::new("/services/api").unwrap();
    let p2 = TaskPath::new("/services/web").unwrap();
    let p3 = TaskPath::new("/services/api/v2").unwrap();
    let p4 = TaskPath::new("/tools/lint").unwrap();

    assert!(p1.matches_glob("/services/*"));
    assert!(p2.matches_glob("/services/*"));
    assert!(!p3.matches_glob("/services/*")); // too deep
    assert!(!p4.matches_glob("/services/*"));
  }

  #[test]
  fn test_glob_double_star() {
    let p1 = TaskPath::new("/services/api").unwrap();
    let p2 = TaskPath::new("/services/api/v2").unwrap();
    let p3 = TaskPath::new("/services/web").unwrap();
    let p4 = TaskPath::new("/tools/lint").unwrap();

    assert!(p1.matches_glob("/services/**"));
    assert!(p2.matches_glob("/services/**"));
    assert!(p3.matches_glob("/services/**"));
    assert!(!p4.matches_glob("/services/**"));

    // /** matches everything
    assert!(p1.matches_glob("/**"));
    assert!(p4.matches_glob("/**"));
  }

  #[test]
  fn test_glob_mixed() {
    let p = TaskPath::new("/a/b/c/d").unwrap();
    assert!(p.matches_glob("/**/c/d"));
    assert!(p.matches_glob("/a/**/d"));
    assert!(p.matches_glob("/a/*/c/*"));
    assert!(!p.matches_glob("/a/*/d"));
  }
}
