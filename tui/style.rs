use tui::style::Modifier;

#[no_mangle]
pub static tui_mod_bold: u16 = Modifier::BOLD.bits();

#[no_mangle]
pub static tui_mod_dim: u16 = Modifier::DIM.bits();

#[no_mangle]
pub static tui_mod_italic: u16 = Modifier::ITALIC.bits();

#[no_mangle]
pub static tui_mod_underlined: u16 = Modifier::UNDERLINED.bits();

#[no_mangle]
pub static tui_mod_slow_blink: u16 = Modifier::SLOW_BLINK.bits();

#[no_mangle]
pub static tui_mod_rapid_blink: u16 = Modifier::RAPID_BLINK.bits();

#[no_mangle]
pub static tui_mod_reversed: u16 = Modifier::REVERSED.bits();

#[no_mangle]
pub static tui_mod_hidden: u16 = Modifier::HIDDEN.bits();

#[no_mangle]
pub static tui_mod_crossed_out: u16 = Modifier::CROSSED_OUT.bits();

#[no_mangle]
pub static tui_mod_empty: u16 = Modifier::empty().bits();
