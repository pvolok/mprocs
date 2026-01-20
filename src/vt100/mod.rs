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

mod attrs;
mod cell;
mod grid;
mod parser;
mod row;
mod screen;
mod size;
mod term_reply;

pub use attrs::Color;
pub use cell::Cell;
pub use parser::Parser;
pub use screen::{MouseProtocolEncoding, MouseProtocolMode, Screen, VtEvent};
pub use size::Size;
pub use term_reply::TermReplySender;
