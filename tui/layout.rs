use ocaml::{Error, Int};
use tui::layout::{Constraint, Direction, Layout, Rect};

#[derive(ocaml::IntoValue, ocaml::FromValue, Debug)]
pub struct RectML {
  pub x: u16,
  pub y: u16,
  pub w: u16,
  pub h: u16,
}

impl RectML {
  pub fn of_tui(rect: Rect) -> Self {
    RectML {
      x: rect.x,
      y: rect.y,
      w: rect.width,
      h: rect.height,
    }
  }

  pub fn tui(&self) -> Rect {
    Rect::new(self.x, self.y, self.w, self.h)
  }
}

#[derive(ocaml::IntoValue, ocaml::FromValue, Debug)]
pub enum DirML {
  Horizontal,
  Vertical,
}

#[derive(ocaml::IntoValue, ocaml::FromValue)]
pub enum OConstraint {
  Percentage(u16),
  Ratio(Int, Int),
  Length(u16),
  Max(u16),
  Min(u16),
}

impl OConstraint {
  pub fn of_ml(&self) -> Result<Constraint, Error> {
    let constr = match self {
      OConstraint::Percentage(x) => (Constraint::Percentage(*x)),
      OConstraint::Ratio(a, b) => {
        Constraint::Ratio((*a).try_into()?, (*b).try_into()?)
      }
      OConstraint::Length(x) => Constraint::Length(*x),
      OConstraint::Max(x) => Constraint::Max(*x),
      OConstraint::Min(x) => Constraint::Min(*x),
    };
    Ok(constr)
  }

  pub fn to_ml(constr: &Constraint) -> Result<OConstraint, Error> {
    let constr = match constr {
      Constraint::Percentage(x) => OConstraint::Percentage(*x),
      Constraint::Ratio(a, b) => {
        OConstraint::Ratio((*a).try_into()?, (*b).try_into()?)
      }
      Constraint::Length(x) => OConstraint::Length(*x),
      Constraint::Max(x) => OConstraint::Max(*x),
      Constraint::Min(x) => OConstraint::Min(*x),
    };
    Ok(constr)
  }
}

#[ocaml::func]
pub fn tui_layout(
  spec: Vec<OConstraint>,
  dir: DirML,
  area: RectML,
) -> Vec<RectML> {
  let parts = Layout::default()
    .direction(match dir {
      DirML::Horizontal => Direction::Horizontal,
      DirML::Vertical => Direction::Vertical,
    })
    .constraints(
      spec
        .iter()
        .map(|c| c.of_ml().expect("<c.of_ml()>"))
        .collect::<Vec<Constraint>>(),
    )
    .split(area.tui());
  parts.iter().map(|p| RectML::of_tui(*p)).collect()
}
