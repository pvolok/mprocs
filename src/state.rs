use crate::{
  app::ClientId,
  config::GroupConfig,
  kernel::proc::ProcId,
  keymap::KeymapGroup,
  proc::{view::ProcView, CopyMode},
  widgets::list::ListState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
  Procs,
  Term,
  TermZoom,
}

/// Represents an item in the sidebar (either a group header or a process)
#[derive(Clone, Debug)]
pub enum SidebarItem {
  Group { group_index: usize },
  Process { proc_index: usize },
}

/// Runtime state for a process group
#[derive(Clone, Debug)]
pub struct GroupState {
  pub name: String,
  pub collapsed: bool,
  pub proc_indices: Vec<usize>,
}

impl Scope {
  pub fn toggle(&self) -> Self {
    match self {
      Scope::Procs => Scope::Term,
      Scope::Term => Scope::Procs,
      Scope::TermZoom => Scope::Procs,
    }
  }

  pub fn is_zoomed(&self) -> bool {
    match self {
      Scope::Procs => false,
      Scope::Term => false,
      Scope::TermZoom => true,
    }
  }
}

pub struct State {
  pub current_client_id: Option<ClientId>,

  pub scope: Scope,
  pub procs: Vec<ProcView>,
  pub procs_list: ListState,
  pub hide_keymap_window: bool,

  pub quitting: bool,

  /// Group states (runtime)
  pub groups: Vec<GroupState>,
  /// Computed view of sidebar items (groups and processes)
  pub sidebar_items: Vec<SidebarItem>,
}

impl State {
  /// Returns the selected sidebar index
  pub fn selected(&self) -> usize {
    self.procs_list.selected()
  }

  /// Returns the currently selected sidebar item
  pub fn get_selected_sidebar_item(&self) -> Option<&SidebarItem> {
    self.sidebar_items.get(self.procs_list.selected())
  }

  /// Returns the proc index if the currently selected sidebar item is a process
  pub fn selected_proc_index(&self) -> Option<usize> {
    match self.get_selected_sidebar_item() {
      Some(SidebarItem::Process { proc_index }) => Some(*proc_index),
      _ => None,
    }
  }

  pub fn get_current_proc(&self) -> Option<&ProcView> {
    self.selected_proc_index().and_then(|i| self.procs.get(i))
  }

  pub fn get_current_proc_mut(&mut self) -> Option<&mut ProcView> {
    self
      .selected_proc_index()
      .and_then(|i| self.procs.get_mut(i))
  }

  /// Select a sidebar item by its index
  pub fn select_sidebar_item(&mut self, index: usize) {
    self.procs_list.select(index);
    if let Some(SidebarItem::Process { proc_index }) =
      self.sidebar_items.get(index)
    {
      if let Some(proc_handle) = self.procs.get_mut(*proc_index) {
        proc_handle.focus();
      }
    }
  }

  /// Legacy method for selecting a proc by its index in the procs list
  pub fn select_proc(&mut self, proc_idx: usize) {
    // Find the sidebar index for this proc
    for (sidebar_idx, item) in self.sidebar_items.iter().enumerate() {
      if let SidebarItem::Process { proc_index } = item {
        if *proc_index == proc_idx {
          self.select_sidebar_item(sidebar_idx);
          return;
        }
      }
    }
    // Fallback: if proc not in sidebar (shouldn't happen), select first item
    if !self.sidebar_items.is_empty() {
      self.select_sidebar_item(0);
    }
  }

  pub fn get_proc_mut(&mut self, id: ProcId) -> Option<&mut ProcView> {
    self.procs.iter_mut().find(|p| p.id() == id)
  }

  pub fn get_keymap_group(&self) -> KeymapGroup {
    match self.scope {
      Scope::Procs => KeymapGroup::Procs,
      Scope::Term | Scope::TermZoom => match self.get_current_proc() {
        Some(proc) => match proc.copy_mode() {
          CopyMode::None(_) => KeymapGroup::Term,
          CopyMode::Active(_, _, _) => KeymapGroup::Copy,
        },
        None => KeymapGroup::Term,
      },
    }
  }

  pub fn all_procs_down(&self) -> bool {
    self.procs.iter().all(|p| !p.is_up())
  }

  pub fn toggle_keymap_window(&mut self) {
    self.hide_keymap_window = !self.hide_keymap_window;
  }

  /// Initialize groups from config
  pub fn init_groups(&mut self, group_configs: &[GroupConfig]) {
    self.groups = group_configs
      .iter()
      .map(|cfg| GroupState {
        name: cfg.name.clone(),
        collapsed: cfg.collapsed,
        proc_indices: Vec::new(),
      })
      .collect();
  }

  /// Populate group proc_indices after procs are started
  pub fn populate_group_indices(&mut self, group_configs: &[GroupConfig]) {
    // Build a map from proc name to proc index
    let proc_name_to_idx: std::collections::HashMap<_, _> = self
      .procs
      .iter()
      .enumerate()
      .map(|(idx, p)| (p.name().to_string(), idx))
      .collect();

    // Populate each group's proc_indices
    for (group_idx, group_cfg) in group_configs.iter().enumerate() {
      if let Some(group) = self.groups.get_mut(group_idx) {
        group.proc_indices = group_cfg
          .proc_names
          .iter()
          .filter_map(|name| proc_name_to_idx.get(name).copied())
          .collect();
      }
    }
  }

  /// Rebuild the sidebar_items list based on current groups and procs
  pub fn rebuild_sidebar_items(&mut self) {
    let mut items = Vec::new();
    let mut grouped_proc_indices = std::collections::HashSet::new();

    // Add groups and their processes
    for (group_idx, group) in self.groups.iter().enumerate() {
      items.push(SidebarItem::Group { group_index: group_idx });

      if !group.collapsed {
        for &proc_idx in &group.proc_indices {
          items.push(SidebarItem::Process { proc_index: proc_idx });
          grouped_proc_indices.insert(proc_idx);
        }
      } else {
        // Still track grouped procs even when collapsed
        for &proc_idx in &group.proc_indices {
          grouped_proc_indices.insert(proc_idx);
        }
      }
    }

    // Add ungrouped processes at the bottom
    for (proc_idx, _proc) in self.procs.iter().enumerate() {
      if !grouped_proc_indices.contains(&proc_idx) {
        items.push(SidebarItem::Process { proc_index: proc_idx });
      }
    }

    self.sidebar_items = items;
  }

  /// Toggle a group's collapsed state by group index
  pub fn toggle_group(&mut self, group_idx: usize) {
    if let Some(group) = self.groups.get_mut(group_idx) {
      group.collapsed = !group.collapsed;
    }
  }

  /// Toggle a group by name (for remote commands)
  pub fn toggle_group_by_name(&mut self, name: &str) -> bool {
    if let Some(group) = self.groups.iter_mut().find(|g| g.name == name) {
      group.collapsed = !group.collapsed;
      true
    } else {
      false
    }
  }

  /// Collapse a group by name
  pub fn collapse_group_by_name(&mut self, name: &str) -> bool {
    if let Some(group) = self.groups.iter_mut().find(|g| g.name == name) {
      group.collapsed = true;
      true
    } else {
      false
    }
  }

  /// Expand a group by name
  pub fn expand_group_by_name(&mut self, name: &str) -> bool {
    if let Some(group) = self.groups.iter_mut().find(|g| g.name == name) {
      group.collapsed = false;
      true
    } else {
      false
    }
  }

  /// Collapse all groups
  pub fn collapse_all_groups(&mut self) {
    for group in &mut self.groups {
      group.collapsed = true;
    }
  }

  /// Expand all groups
  pub fn expand_all_groups(&mut self) {
    for group in &mut self.groups {
      group.collapsed = false;
    }
  }

  /// Handle selection adjustment after a group collapse
  /// If the selected item is inside a collapsed group, move selection to the group header
  pub fn adjust_selection_after_collapse(&mut self) {
    let selected = self.procs_list.selected();

    // If current selection is beyond the new sidebar_items length, adjust
    if selected >= self.sidebar_items.len() && !self.sidebar_items.is_empty() {
      self
        .procs_list
        .select(self.sidebar_items.len().saturating_sub(1));
    }
  }

  /// Returns the number of sidebar items
  pub fn sidebar_len(&self) -> usize {
    self.sidebar_items.len()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn make_test_state() -> State {
    State {
      current_client_id: None,
      scope: Scope::Procs,
      procs: Vec::new(),
      procs_list: ListState::default(),
      hide_keymap_window: false,
      quitting: false,
      groups: Vec::new(),
      sidebar_items: Vec::new(),
    }
  }

  fn make_group_configs() -> Vec<GroupConfig> {
    vec![
      GroupConfig {
        name: "backend".to_string(),
        collapsed: false,
        proc_names: vec!["server".to_string(), "worker".to_string()],
      },
      GroupConfig {
        name: "frontend".to_string(),
        collapsed: true,
        proc_names: vec!["client".to_string()],
      },
    ]
  }

  #[test]
  fn test_init_groups_empty() {
    let mut state = make_test_state();
    state.init_groups(&[]);
    assert!(state.groups.is_empty());
  }

  #[test]
  fn test_init_groups_single() {
    let mut state = make_test_state();
    let configs = vec![GroupConfig {
      name: "test".to_string(),
      collapsed: false,
      proc_names: vec!["proc1".to_string()],
    }];
    state.init_groups(&configs);

    assert_eq!(state.groups.len(), 1);
    assert_eq!(state.groups[0].name, "test");
    assert!(!state.groups[0].collapsed);
    // proc_indices are populated by populate_group_indices, not init_groups
    assert!(state.groups[0].proc_indices.is_empty());
  }

  #[test]
  fn test_init_groups_preserves_collapsed_state() {
    let mut state = make_test_state();
    let configs = make_group_configs();
    state.init_groups(&configs);

    assert_eq!(state.groups.len(), 2);
    assert!(!state.groups[0].collapsed); // backend
    assert!(state.groups[1].collapsed); // frontend
  }

  #[test]
  fn test_toggle_group_by_name_existing() {
    let mut state = make_test_state();
    state.groups = vec![GroupState {
      name: "test".to_string(),
      collapsed: false,
      proc_indices: Vec::new(),
    }];

    let result = state.toggle_group_by_name("test");
    assert!(result);
    assert!(state.groups[0].collapsed);

    let result = state.toggle_group_by_name("test");
    assert!(result);
    assert!(!state.groups[0].collapsed);
  }

  #[test]
  fn test_toggle_group_by_name_nonexistent() {
    let mut state = make_test_state();
    state.groups = vec![GroupState {
      name: "test".to_string(),
      collapsed: false,
      proc_indices: Vec::new(),
    }];

    let result = state.toggle_group_by_name("nonexistent");
    assert!(!result);
    // Original group should be unchanged
    assert!(!state.groups[0].collapsed);
  }

  #[test]
  fn test_collapse_group_by_name() {
    let mut state = make_test_state();
    state.groups = vec![GroupState {
      name: "test".to_string(),
      collapsed: false,
      proc_indices: Vec::new(),
    }];

    let result = state.collapse_group_by_name("test");
    assert!(result);
    assert!(state.groups[0].collapsed);

    // Calling again should still return true and stay collapsed
    let result = state.collapse_group_by_name("test");
    assert!(result);
    assert!(state.groups[0].collapsed);
  }

  #[test]
  fn test_expand_group_by_name() {
    let mut state = make_test_state();
    state.groups = vec![GroupState {
      name: "test".to_string(),
      collapsed: true,
      proc_indices: Vec::new(),
    }];

    let result = state.expand_group_by_name("test");
    assert!(result);
    assert!(!state.groups[0].collapsed);

    // Calling again should still return true and stay expanded
    let result = state.expand_group_by_name("test");
    assert!(result);
    assert!(!state.groups[0].collapsed);
  }

  #[test]
  fn test_collapse_all_groups() {
    let mut state = make_test_state();
    state.groups = vec![
      GroupState {
        name: "group1".to_string(),
        collapsed: false,
        proc_indices: Vec::new(),
      },
      GroupState {
        name: "group2".to_string(),
        collapsed: false,
        proc_indices: Vec::new(),
      },
    ];

    state.collapse_all_groups();

    assert!(state.groups[0].collapsed);
    assert!(state.groups[1].collapsed);
  }

  #[test]
  fn test_expand_all_groups() {
    let mut state = make_test_state();
    state.groups = vec![
      GroupState {
        name: "group1".to_string(),
        collapsed: true,
        proc_indices: Vec::new(),
      },
      GroupState {
        name: "group2".to_string(),
        collapsed: true,
        proc_indices: Vec::new(),
      },
    ];

    state.expand_all_groups();

    assert!(!state.groups[0].collapsed);
    assert!(!state.groups[1].collapsed);
  }

  #[test]
  fn test_toggle_group_by_index() {
    let mut state = make_test_state();
    state.groups = vec![
      GroupState {
        name: "group1".to_string(),
        collapsed: false,
        proc_indices: Vec::new(),
      },
      GroupState {
        name: "group2".to_string(),
        collapsed: true,
        proc_indices: Vec::new(),
      },
    ];

    state.toggle_group(0);
    assert!(state.groups[0].collapsed);

    state.toggle_group(1);
    assert!(!state.groups[1].collapsed);
  }

  #[test]
  fn test_toggle_group_by_index_out_of_bounds() {
    let mut state = make_test_state();
    state.groups = vec![GroupState {
      name: "group1".to_string(),
      collapsed: false,
      proc_indices: Vec::new(),
    }];

    // Should not panic
    state.toggle_group(10);
    // Original group should be unchanged
    assert!(!state.groups[0].collapsed);
  }

  #[test]
  fn test_sidebar_len() {
    let mut state = make_test_state();
    state.sidebar_items = vec![
      SidebarItem::Group { group_index: 0 },
      SidebarItem::Process { proc_index: 0 },
      SidebarItem::Process { proc_index: 1 },
    ];

    assert_eq!(state.sidebar_len(), 3);
  }

  #[test]
  fn test_selected_proc_index_when_group_selected() {
    let mut state = make_test_state();
    state.sidebar_items = vec![
      SidebarItem::Group { group_index: 0 },
      SidebarItem::Process { proc_index: 0 },
    ];
    // Configure the ListState to know about item count
    state
      .procs_list
      .fit(crate::vt100::grid::Rect::new(0, 0, 10, 10), 2);
    state.procs_list.select(0); // Select the group

    assert_eq!(state.selected_proc_index(), None);
  }

  #[test]
  fn test_selected_proc_index_when_proc_selected() {
    let mut state = make_test_state();
    state.sidebar_items = vec![
      SidebarItem::Group { group_index: 0 },
      SidebarItem::Process { proc_index: 0 },
    ];
    // Configure the ListState to know about item count
    state
      .procs_list
      .fit(crate::vt100::grid::Rect::new(0, 0, 10, 10), 2);
    state.procs_list.select(1); // Select the process

    assert_eq!(state.selected_proc_index(), Some(0));
  }

  #[test]
  fn test_get_selected_sidebar_item() {
    let mut state = make_test_state();
    state.sidebar_items = vec![
      SidebarItem::Group { group_index: 0 },
      SidebarItem::Process { proc_index: 0 },
    ];
    // Configure the ListState to know about item count
    state
      .procs_list
      .fit(crate::vt100::grid::Rect::new(0, 0, 10, 10), 2);

    state.procs_list.select(0);
    match state.get_selected_sidebar_item() {
      Some(SidebarItem::Group { group_index }) => assert_eq!(*group_index, 0),
      _ => panic!("Expected Group"),
    }

    state.procs_list.select(1);
    match state.get_selected_sidebar_item() {
      Some(SidebarItem::Process { proc_index }) => assert_eq!(*proc_index, 0),
      _ => panic!("Expected Process"),
    }
  }

  #[test]
  fn test_adjust_selection_after_collapse() {
    let mut state = make_test_state();
    state.sidebar_items = vec![
      SidebarItem::Group { group_index: 0 },
      SidebarItem::Process { proc_index: 0 },
    ];
    // Configure the ListState with a count larger than sidebar_items
    // to simulate an out-of-bounds selection
    state
      .procs_list
      .fit(crate::vt100::grid::Rect::new(0, 0, 10, 10), 10);
    state.procs_list.select(5); // Selection beyond sidebar length

    state.adjust_selection_after_collapse();

    // Should adjust to last valid index
    assert_eq!(state.procs_list.selected(), 1);
  }
}
