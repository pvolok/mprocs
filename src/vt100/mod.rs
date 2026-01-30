#![warn(clippy::cargo)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::as_conversions)]
#![warn(clippy::get_unwrap)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::similar_names)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::type_complexity)]

pub mod attrs;
pub mod cell;
pub mod grid;
pub mod parser;
pub mod row;
pub mod screen;
pub mod screen_differ;
pub mod size;

pub use attrs::Color;
pub use cell::Cell;
pub use grid::Grid;
pub use parser::Parser;
pub use screen::{MouseProtocolMode, Screen, VtEvent};
pub use screen_differ::ScreenDiffer;
pub use size::Size;
