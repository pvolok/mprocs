use tui::style::{Color, Modifier, Style};

//
// Color
//

#[derive(ocaml::IntoValue, ocaml::FromValue)]
pub enum ColorML {
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
    Rgb(u8, u8, u8),
    Indexed(u8),
}

impl From<Color> for ColorML {
    fn from(c: Color) -> Self {
        match c {
            Color::Reset => ColorML::Reset,
            Color::Black => ColorML::Black,
            Color::Red => ColorML::Red,
            Color::Green => ColorML::Green,
            Color::Yellow => ColorML::Yellow,
            Color::Blue => ColorML::Blue,
            Color::Magenta => ColorML::Magenta,
            Color::Cyan => ColorML::Cyan,
            Color::Gray => ColorML::Gray,
            Color::DarkGray => ColorML::DarkGray,
            Color::LightRed => ColorML::LightRed,
            Color::LightGreen => ColorML::LightGreen,
            Color::LightYellow => ColorML::LightYellow,
            Color::LightBlue => ColorML::LightBlue,
            Color::LightMagenta => ColorML::LightMagenta,
            Color::LightCyan => ColorML::LightCyan,
            Color::White => ColorML::White,
            Color::Rgb(r, g, b) => ColorML::Rgb(r, g, b),
            Color::Indexed(i) => ColorML::Indexed(i),
        }
    }
}

impl From<ColorML> for Color {
    fn from(c: ColorML) -> Self {
        match c {
            ColorML::Reset => Color::Reset,
            ColorML::Black => Color::Black,
            ColorML::Red => Color::Red,
            ColorML::Green => Color::Green,
            ColorML::Yellow => Color::Yellow,
            ColorML::Blue => Color::Blue,
            ColorML::Magenta => Color::Magenta,
            ColorML::Cyan => Color::Cyan,
            ColorML::Gray => Color::Gray,
            ColorML::DarkGray => Color::DarkGray,
            ColorML::LightRed => Color::LightRed,
            ColorML::LightGreen => Color::LightGreen,
            ColorML::LightYellow => Color::LightYellow,
            ColorML::LightBlue => Color::LightBlue,
            ColorML::LightMagenta => Color::LightMagenta,
            ColorML::LightCyan => Color::LightCyan,
            ColorML::White => Color::White,
            ColorML::Rgb(r, g, b) => Color::Rgb(r, g, b),
            ColorML::Indexed(i) => Color::Indexed(i),
        }
    }
}

//
// Modifier
//

#[ocaml::func]
pub fn tui_style_bold() -> u16 {
    Modifier::BOLD.bits()
}

#[ocaml::func]
pub fn tui_style_dim() -> u16 {
    Modifier::DIM.bits()
}

#[ocaml::func]
pub fn tui_style_italic() -> u16 {
    Modifier::ITALIC.bits()
}

#[ocaml::func]
pub fn tui_style_underlined() -> u16 {
    Modifier::UNDERLINED.bits()
}

#[ocaml::func]
pub fn tui_style_slow_blink() -> u16 {
    Modifier::SLOW_BLINK.bits()
}

#[ocaml::func]
pub fn tui_style_rapid_blink() -> u16 {
    Modifier::RAPID_BLINK.bits()
}

#[ocaml::func]
pub fn tui_style_reversed() -> u16 {
    Modifier::REVERSED.bits()
}

#[ocaml::func]
pub fn tui_style_hidden() -> u16 {
    Modifier::HIDDEN.bits()
}

#[ocaml::func]
pub fn tui_style_crossed_out() -> u16 {
    Modifier::CROSSED_OUT.bits()
}

//
// Style
//

#[derive(ocaml::IntoValue, ocaml::FromValue)]
pub struct StyleML {
    pub fg: Option<ColorML>,
    pub bg: Option<ColorML>,
    pub add_modifier: u16,
    pub sub_modifier: u16,
}

impl From<Style> for StyleML {
    fn from(s: Style) -> Self {
        StyleML {
            fg: s.fg.map(|c| c.into()),
            bg: s.bg.map(|c| c.into()),
            add_modifier: s.add_modifier.bits(),
            sub_modifier: s.sub_modifier.bits(),
        }
    }
}

impl From<StyleML> for Style {
    fn from(s: StyleML) -> Self {
        Style {
            fg: s.fg.map(|c| c.into()),
            bg: s.bg.map(|c| c.into()),
            add_modifier: Modifier::from_bits_truncate(s.add_modifier),
            sub_modifier: Modifier::from_bits_truncate(s.sub_modifier),
        }
    }
}
