use anyhow::Result;

use crate::cfg::CfgObj;
use crate::console::action::Action;

#[derive(Clone)]
pub enum Hook {
  Action(Action),
}

impl Hook {
  pub fn as_action(&self) -> &Action {
    let Hook::Action(action) = self;
    action
  }
}

pub(crate) fn event_from_cfg(
  obj: &CfgObj<'_>,
  key: &str,
) -> Result<Option<Hook>> {
  match obj.get(key) {
    Some(node) => {
      let action: Action = serde_yaml::from_value(node.raw().clone())?;
      Ok(Some(Hook::Action(action)))
    }
    None => Ok(None),
  }
}
