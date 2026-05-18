use std::collections::HashMap;

use crate::{
  console::views::pane::Pane,
  term::{Size, grid::Rect},
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaneId(u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis {
  Horizontal,
  Vertical,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Dir {
  Up,
  Down,
  Left,
  Right,
}

impl Dir {
  fn axis(self) -> Axis {
    match self {
      Dir::Left | Dir::Right => Axis::Horizontal,
      Dir::Up | Dir::Down => Axis::Vertical,
    }
  }

  fn after(self) -> bool {
    matches!(self, Dir::Right | Dir::Down)
  }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SizeSpec {
  /// Fixed extent in cells along the parent's axis.
  Fixed(u16),
  /// Take an even share of whatever's left after fixed-size siblings.
  Fill,
}

struct Node {
  parent: Option<PaneId>,
  /// Size along the parent's axis.
  size: SizeSpec,
  area: Rect,
  kind: NodeKind,
}

enum NodeKind {
  Container { axis: Axis, children: Vec<PaneId> },
  Leaf { pane: Box<dyn Pane> },
}

pub struct Layout {
  size: Size,
  nodes: HashMap<PaneId, Node>,
  next_id: u32,
  root: PaneId,
  dirty: bool,
}

impl Layout {
  pub fn new(size: Size) -> Self {
    let root = PaneId(0);
    let mut nodes = HashMap::new();
    nodes.insert(
      root,
      Node {
        parent: None,
        size: SizeSpec::Fill,
        area: Rect::new(0, 0, size.width, size.height),
        kind: NodeKind::Container {
          axis: Axis::Horizontal,
          children: Vec::new(),
        },
      },
    );
    Self {
      size,
      nodes,
      next_id: 1,
      root,
      dirty: true,
    }
  }

  pub fn resize(&mut self, size: Size) {
    if self.size != size {
      self.size = size;
      self.dirty = true;
    }
  }

  pub fn root(&self) -> PaneId {
    self.root
  }

  /// Insert a new pane next to `anchor` in `dir` direction. If `anchor` is a
  /// container, descends to its first or last child (per `dir`) and splits
  /// there; an empty container gets the new pane as its first child. Pass
  /// [`Layout::root`] to add into the root.
  pub fn insert(
    &mut self,
    anchor: PaneId,
    dir: Dir,
    pane: Box<dyn Pane>,
    size: SizeSpec,
  ) -> PaneId {
    let mut cur = anchor;
    loop {
      match &self.nodes[&cur].kind {
        NodeKind::Leaf { .. } => return self.split(cur, pane, size, dir),
        NodeKind::Container { children, .. } => {
          if children.is_empty() {
            return self.append_child(cur, pane, size);
          }
          cur = if dir.after() {
            *children.last().unwrap()
          } else {
            *children.first().unwrap()
          };
        }
      }
    }
  }

  fn append_child(
    &mut self,
    container: PaneId,
    pane: Box<dyn Pane>,
    size: SizeSpec,
  ) -> PaneId {
    let id = self.fresh_id();
    self.nodes.insert(
      id,
      Node {
        parent: Some(container),
        size,
        area: Rect::default(),
        kind: NodeKind::Leaf { pane },
      },
    );
    let parent = self.nodes.get_mut(&container).expect("container missing");
    let NodeKind::Container { children, .. } = &mut parent.kind else {
      panic!("not a container");
    };
    children.push(id);
    self.dirty = true;
    id
  }

  /// Re-layout if dirty and return the rendered geometry of every leaf in
  /// tree order. Use [`Layout::pane_mut`] to access the pane object for each
  /// id while iterating.
  pub fn render(&mut self) -> Vec<(PaneId, Rect)> {
    if self.dirty {
      self.relayout();
    }
    let mut out = Vec::new();
    self.collect_leaves(self.root, &mut out);
    out
  }

  pub fn area(&mut self, id: PaneId) -> Option<Rect> {
    if self.dirty {
      self.relayout();
    }
    self.nodes.get(&id).map(|n| n.area)
  }

  pub fn pane_mut(&mut self, id: PaneId) -> &mut dyn Pane {
    match &mut self.nodes.get_mut(&id).expect("no such pane").kind {
      NodeKind::Leaf { pane } => pane.as_mut(),
      NodeKind::Container { .. } => panic!("not a leaf"),
    }
  }

  /// Find the closest leaf neighbor of `from` in the given direction. Wraps
  /// around at the layout edges. Returns `None` only if no ancestor of `from`
  /// has the matching axis (e.g. moving vertically in a purely horizontal
  /// layout).
  pub fn neighbor(&self, from: PaneId, dir: Dir) -> Option<PaneId> {
    let target_axis = dir.axis();
    let after = dir.after();

    let mut current = from;
    let mut topmost_match: Option<PaneId> = None;
    loop {
      let Some(parent_id) = self.nodes.get(&current)?.parent else {
        break;
      };
      let parent = &self.nodes[&parent_id];
      if let NodeKind::Container { axis, children } = &parent.kind {
        if *axis == target_axis {
          topmost_match = Some(parent_id);
          let idx = children.iter().position(|&c| c == current).unwrap();
          let sibling_idx = if after {
            (idx + 1 < children.len()).then(|| idx + 1)
          } else {
            idx.checked_sub(1)
          };
          if let Some(si) = sibling_idx {
            return Some(self.descend_to_leaf(children[si], after));
          }
        }
      }
      current = parent_id;
    }

    // No sibling found anywhere up the chain. Wrap around within the
    // outermost matching-axis container we walked through.
    let top = topmost_match?;
    let NodeKind::Container { children, .. } = &self.nodes[&top].kind else {
      return None;
    };
    let wrap_idx = if after { 0 } else { children.len() - 1 };
    Some(self.descend_to_leaf(children[wrap_idx], after))
  }

  /// Descend a subtree to a leaf. `prefer_first` picks the first child of
  /// each container (use when entering a subtree from its left/top); `false`
  /// picks the last child (entering from right/bottom).
  fn descend_to_leaf(&self, node: PaneId, prefer_first: bool) -> PaneId {
    match &self.nodes[&node].kind {
      NodeKind::Leaf { .. } => node,
      NodeKind::Container { children, .. } => {
        let pick = if prefer_first {
          *children.first().expect("empty container")
        } else {
          *children.last().expect("empty container")
        };
        self.descend_to_leaf(pick, prefer_first)
      }
    }
  }

  fn split(
    &mut self,
    anchor: PaneId,
    pane: Box<dyn Pane>,
    size: SizeSpec,
    dir: Dir,
  ) -> PaneId {
    let target_axis = dir.axis();
    let after = dir.after();

    let anchor_parent = self
      .nodes
      .get(&anchor)
      .expect("no such anchor")
      .parent
      .expect("can't split root");
    let parent_axis = match &self.nodes[&anchor_parent].kind {
      NodeKind::Container { axis, .. } => *axis,
      NodeKind::Leaf { .. } => panic!("anchor's parent is not a container"),
    };

    let new_id = self.fresh_id();

    if parent_axis == target_axis {
      // Same axis as parent: just add a sibling next to anchor.
      self.nodes.insert(
        new_id,
        Node {
          parent: Some(anchor_parent),
          size,
          area: Rect::default(),
          kind: NodeKind::Leaf { pane },
        },
      );
      let parent = self.nodes.get_mut(&anchor_parent).unwrap();
      if let NodeKind::Container { children, .. } = &mut parent.kind {
        let idx = children.iter().position(|&c| c == anchor).unwrap();
        let at = if after { idx + 1 } else { idx };
        children.insert(at, new_id);
      }
    } else {
      // Cross-axis: wrap anchor in a new container that takes its place.
      let container_id = self.fresh_id();
      let anchor_size = self.nodes[&anchor].size;

      self.nodes.insert(
        new_id,
        Node {
          parent: Some(container_id),
          size,
          area: Rect::default(),
          kind: NodeKind::Leaf { pane },
        },
      );

      let container_children = if after {
        vec![anchor, new_id]
      } else {
        vec![new_id, anchor]
      };
      self.nodes.insert(
        container_id,
        Node {
          parent: Some(anchor_parent),
          size: anchor_size,
          area: Rect::default(),
          kind: NodeKind::Container {
            axis: target_axis,
            children: container_children,
          },
        },
      );

      let anchor_node = self.nodes.get_mut(&anchor).unwrap();
      anchor_node.parent = Some(container_id);
      anchor_node.size = SizeSpec::Fill;

      let parent = self.nodes.get_mut(&anchor_parent).unwrap();
      if let NodeKind::Container { children, .. } = &mut parent.kind {
        let idx = children.iter().position(|&c| c == anchor).unwrap();
        children[idx] = container_id;
      }
    }

    self.dirty = true;
    new_id
  }

  fn relayout(&mut self) {
    self.dirty = false;
    let area = Rect::new(0, 0, self.size.width, self.size.height);
    self.layout_node(self.root, area);
  }

  fn layout_node(&mut self, id: PaneId, area: Rect) {
    self.nodes.get_mut(&id).unwrap().area = area;

    let (axis, children) = match &self.nodes[&id].kind {
      NodeKind::Container { axis, children } => (*axis, children.clone()),
      NodeKind::Leaf { .. } => return,
    };

    let extent = match axis {
      Axis::Horizontal => area.width,
      Axis::Vertical => area.height,
    };

    let mut total_fixed = 0u16;
    let mut fill_count = 0u16;
    for c in &children {
      match self.nodes[c].size {
        SizeSpec::Fixed(n) => total_fixed = total_fixed.saturating_add(n),
        SizeSpec::Fill => fill_count += 1,
      }
    }
    let remaining = extent.saturating_sub(total_fixed);
    let (fill_each, fill_extra) = if fill_count > 0 {
      (remaining / fill_count, remaining % fill_count)
    } else {
      (0, 0)
    };

    let mut offset = 0u16;
    let mut fill_seen = 0u16;
    for c in &children {
      let len = match self.nodes[c].size {
        SizeSpec::Fixed(n) => n,
        SizeSpec::Fill => {
          let extra = if fill_seen < fill_extra { 1 } else { 0 };
          fill_seen += 1;
          fill_each + extra
        }
      };
      let child_area = match axis {
        Axis::Horizontal => {
          Rect::new(area.x + offset, area.y, len, area.height)
        }
        Axis::Vertical => Rect::new(area.x, area.y + offset, area.width, len),
      };
      self.layout_node(*c, child_area);
      offset = offset.saturating_add(len);
    }
  }

  fn collect_leaves(&self, id: PaneId, out: &mut Vec<(PaneId, Rect)>) {
    let node = &self.nodes[&id];
    match &node.kind {
      NodeKind::Leaf { .. } => out.push((id, node.area)),
      NodeKind::Container { children, .. } => {
        for c in children {
          self.collect_leaves(*c, out);
        }
      }
    }
  }

  fn fresh_id(&mut self) -> PaneId {
    let id = PaneId(self.next_id);
    self.next_id += 1;
    id
  }
}
