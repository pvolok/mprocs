use std::collections::HashMap;
use std::hash::Hash;

use anyhow::{Result, bail};

use crate::term::key::{Key, KeySpec};

pub type KeySeq = Vec<Key>;

pub struct Keymap<A> {
  root: Group<A>,
  rev: HashMap<A, Vec<KeySeq>>,
}

pub struct Group<A> {
  pub label: Option<String>,
  children: HashMap<Key, Entry<A>>,
}

enum Entry<A> {
  Group(Group<A>),
  Binding(Binding<A>),
}

pub struct Binding<A> {
  pub action: A,
  pub desc: Option<String>,
}

pub enum Lookup<'a, A> {
  None,
  Pending(&'a Group<A>),
  Found(&'a Binding<A>),
}

pub enum ChildKind<'a, A> {
  Group(&'a Group<A>),
  Binding(&'a Binding<A>),
}

impl<A> Group<A> {
  fn new(label: Option<String>) -> Self {
    Group {
      label,
      children: HashMap::new(),
    }
  }
}

impl<A> Keymap<A> {
  pub fn new() -> Self {
    Keymap {
      root: Group::new(None),
      rev: HashMap::new(),
    }
  }

  pub fn lookup(&self, seq: &[Key]) -> Lookup<'_, A> {
    let mut group = &self.root;
    for (i, key) in seq.iter().enumerate() {
      match group.children.get(key) {
        None => return Lookup::None,
        Some(Entry::Binding(b)) => {
          return if i + 1 == seq.len() {
            Lookup::Found(b)
          } else {
            Lookup::None
          };
        }
        Some(Entry::Group(g)) => group = g,
      }
    }
    Lookup::Pending(group)
  }

  pub fn bindings(&self) -> Vec<(KeySeq, &Binding<A>)> {
    let mut out = Vec::new();
    collect(&self.root, &mut Vec::new(), &mut out);
    out
  }

  pub fn children_of(&self, prefix: &[Key]) -> Vec<(Key, ChildKind<'_, A>)> {
    let group = match self.lookup(prefix) {
      Lookup::Pending(g) => g,
      Lookup::None | Lookup::Found(_) => return Vec::new(),
    };
    group
      .children
      .iter()
      .map(|(key, entry)| {
        let kind = match entry {
          Entry::Group(g) => ChildKind::Group(g),
          Entry::Binding(b) => ChildKind::Binding(b),
        };
        (*key, kind)
      })
      .collect()
  }
}

impl<A: Clone + Eq + Hash> Keymap<A> {
  pub fn bind(
    &mut self,
    seq: &str,
    action: A,
    desc: Option<String>,
  ) -> Result<()> {
    let seq = parse_seq(seq)?;
    let binding = Binding {
      action: action.clone(),
      desc,
    };
    if let Some(old) = insert(&mut self.root, &seq, binding)? {
      self.unindex(&old, &seq);
    }
    self.rev.entry(action).or_default().push(seq);
    Ok(())
  }

  fn unindex(&mut self, action: &A, seq: &KeySeq) {
    if let Some(seqs) = self.rev.get_mut(action) {
      seqs.retain(|s| s != seq);
      if seqs.is_empty() {
        self.rev.remove(action);
      }
    }
  }
}

impl<A: Eq + Hash> Keymap<A> {
  pub fn keys_of(&self, action: &A) -> &[KeySeq] {
    self.rev.get(action).map_or(&[], Vec::as_slice)
  }

  pub fn key_of(&self, action: &A) -> Option<&KeySeq> {
    self.rev.get(action).and_then(|seqs| seqs.first())
  }

  pub fn set_group_label(
    &mut self,
    seq: &str,
    label: impl Into<String>,
  ) -> Result<()> {
    let seq = parse_seq(seq)?;
    let mut group = &mut self.root;
    for key in &seq {
      let entry = group
        .children
        .entry(*key)
        .or_insert_with(|| Entry::Group(Group::new(None)));
      match entry {
        Entry::Group(g) => group = g,
        Entry::Binding(_) => {
          bail!("Key sequence resolves to a binding, not a group")
        }
      }
    }
    group.label = Some(label.into());
    Ok(())
  }
}

/// Per-consumer chord state: each layer (modal, focused pane, global) owns one
/// and feeds keys into its own keymap.
#[derive(Default)]
pub struct Chord {
  pending: KeySeq,
}

pub enum Step<'a, A> {
  Action(&'a A),
  Pending(&'a Group<A>),
  Unmatched,
}

impl Chord {
  pub fn pending(&self) -> &[Key] {
    &self.pending
  }

  pub fn reset(&mut self) {
    self.pending.clear();
  }

  pub fn feed<'a, A>(
    &mut self,
    keymap: &'a Keymap<A>,
    key: Key,
  ) -> Step<'a, A> {
    self.pending.push(key);
    match keymap.lookup(&self.pending) {
      Lookup::Found(b) => {
        self.pending.clear();
        Step::Action(&b.action)
      }
      Lookup::Pending(g) => Step::Pending(g),
      Lookup::None => {
        self.pending.clear();
        Step::Unmatched
      }
    }
  }
}

pub fn keyseq_to_string(seq: &[Key]) -> String {
  seq
    .iter()
    .map(|key| key.spec().to_string())
    .collect::<Vec<_>>()
    .join(" ")
}

fn parse_seq(text: &str) -> Result<KeySeq> {
  let seq = text
    .split_whitespace()
    .map(KeySpec::parse)
    .map(|spec| spec.map(KeySpec::key))
    .collect::<Result<KeySeq>>()?;
  if seq.is_empty() {
    bail!("Empty key sequence");
  }
  Ok(seq)
}

/// Returns the action previously bound at this leaf, if it was overwritten.
fn insert<A>(
  group: &mut Group<A>,
  seq: &[Key],
  binding: Binding<A>,
) -> Result<Option<A>> {
  let (first, rest) = seq.split_first().unwrap();
  if rest.is_empty() {
    if let Some(Entry::Group(_)) = group.children.get(first) {
      bail!("Key sequence conflicts with an existing group");
    }
    match group.children.insert(*first, Entry::Binding(binding)) {
      Some(Entry::Binding(old)) => Ok(Some(old.action)),
      _ => Ok(None),
    }
  } else {
    let entry = group
      .children
      .entry(*first)
      .or_insert_with(|| Entry::Group(Group::new(None)));
    match entry {
      Entry::Group(g) => insert(g, rest, binding),
      Entry::Binding(_) => {
        bail!("Key sequence conflicts with an existing binding")
      }
    }
  }
}

fn collect<'a, A>(
  group: &'a Group<A>,
  prefix: &mut KeySeq,
  out: &mut Vec<(KeySeq, &'a Binding<A>)>,
) {
  for (key, entry) in &group.children {
    prefix.push(*key);
    match entry {
      Entry::Binding(b) => out.push((prefix.clone(), b)),
      Entry::Group(g) => collect(g, prefix, out),
    }
    prefix.pop();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[derive(Clone, Debug, Eq, Hash, PartialEq)]
  enum Act {
    Down,
    Up,
    GoDef,
    GoFile,
  }

  fn seq(text: &str) -> KeySeq {
    parse_seq(text).unwrap()
  }

  #[test]
  fn single_and_chord() {
    let mut km = Keymap::new();
    km.bind("<j>", Act::Down, None).unwrap();
    km.set_group_label("<g>", "go").unwrap();
    km.bind("<g> <d>", Act::GoDef, Some("go definition".into()))
      .unwrap();

    assert!(matches!(km.lookup(&seq("<j>")), Lookup::Found(_)));
    assert!(matches!(km.lookup(&seq("<g>")), Lookup::Pending(_)));
    assert!(matches!(km.lookup(&seq("<g> <d>")), Lookup::Found(_)));
    assert!(matches!(km.lookup(&seq("<g> <x>")), Lookup::None));
  }

  #[test]
  fn chord_state_machine() {
    let mut km = Keymap::new();
    km.bind("<g> <d>", Act::GoDef, None).unwrap();
    let mut chord = Chord::default();

    let g = Key::parse("<g>").unwrap();
    let d = Key::parse("<d>").unwrap();
    assert!(matches!(chord.feed(&km, g), Step::Pending(_)));
    assert!(matches!(chord.feed(&km, d), Step::Action(Act::GoDef)));
    // Buffer resets after a completed chord.
    assert!(chord.pending().is_empty());
  }

  #[test]
  fn multiple_bindings_per_action() {
    let mut km = Keymap::new();
    km.bind("<j>", Act::Down, None).unwrap();
    km.bind("<Down>", Act::Down, None).unwrap();

    assert_eq!(km.keys_of(&Act::Down).len(), 2);
    let strs: Vec<String> = km
      .keys_of(&Act::Down)
      .iter()
      .map(|s| keyseq_to_string(s))
      .collect();
    assert!(strs.contains(&"<j>".to_string()));
    assert!(strs.contains(&"<Down>".to_string()));
  }

  #[test]
  fn overwriting_a_key_drops_its_old_reverse_entry() {
    let mut km = Keymap::new();
    km.bind("<g> <d>", Act::GoDef, None).unwrap();
    km.bind("<g> <f>", Act::GoDef, None).unwrap();
    assert_eq!(km.keys_of(&Act::GoDef).len(), 2);

    // Rebind <g> <f> onto a different action.
    km.bind("<g> <f>", Act::GoFile, None).unwrap();
    assert_eq!(km.keys_of(&Act::GoDef), &[seq("<g> <d>")]);
    assert_eq!(km.keys_of(&Act::GoFile), &[seq("<g> <f>")]);
  }

  #[test]
  fn conflicts_are_reported() {
    let mut km = Keymap::<Act>::new();
    km.bind("<g> <d>", Act::GoDef, None).unwrap();
    // <g> is a group, can't also be a binding.
    assert!(km.bind("<g>", Act::Up, None).is_err());

    let mut km2 = Keymap::<Act>::new();
    km2.bind("<g>", Act::Up, None).unwrap();
    // <g> is a binding, can't descend into it.
    assert!(km2.bind("<g> <d>", Act::GoDef, None).is_err());
  }

  #[test]
  fn lists_bindings_and_group_children() {
    let mut km = Keymap::new();
    km.bind("<j>", Act::Down, None).unwrap();
    km.set_group_label("<g>", "go").unwrap();
    km.bind("<g> <d>", Act::GoDef, None).unwrap();
    km.bind("<g> <f>", Act::GoFile, None).unwrap();

    assert_eq!(km.bindings().len(), 3);
    // After <g>: two child bindings.
    assert_eq!(km.children_of(&seq("<g>")).len(), 2);
    // At root: <j> binding + <g> group.
    assert_eq!(km.children_of(&[]).len(), 2);
  }
}
