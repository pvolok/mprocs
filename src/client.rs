use std::io::{stdout, Write};

use crossterm::event::Event;
use termwiz::{
  cell::AttributeChange,
  color::{ColorAttribute, ColorSpec},
  escape::{csi::Sgr, Action, OneBased, OperatingSystemCommand, CSI},
};

use crate::term::term_driver::TermDriver;
use crate::{
  host::{receiver::MsgReceiver, sender::MsgSender},
  protocol::{CltToSrv, CursorStyle, SrvToClt},
};

pub async fn client_main(
  sender: MsgSender<CltToSrv>,
  receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let mut term_driver = TermDriver::create()?;

  let result = client_main_loop(&mut term_driver, sender, receiver).await;

  if let Err(err) = term_driver.destroy() {
    log::error!("Term driver destroy error: {:?}", err);
  }

  result
}

async fn client_main_loop(
  term_driver: &mut TermDriver,
  mut sender: MsgSender<CltToSrv>,
  mut receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let (width, height) = crossterm::terminal::size()?;
  sender.send(CltToSrv::Init { width, height })?;

  #[derive(Debug)]
  enum LocalEvent {
    ServerMsg(Option<SrvToClt>),
    TermEvent(std::io::Result<Option<Event>>),
  }

  loop {
    let event = tokio::select! {
      msg = receiver.recv() => {
        LocalEvent::ServerMsg(msg.transpose().ok().flatten())
      }
      evt = term_driver.input() => {
        LocalEvent::TermEvent(evt)
      }
    };
    match event {
      LocalEvent::ServerMsg(msg) => match msg {
        Some(msg) => match msg {
          SrvToClt::Print(text) => {
            std::io::stdout().write_all(text.as_bytes())?;
          }
          SrvToClt::SetAttr(attr) => {
            let action = match attr {
              AttributeChange::Intensity(intensity) => {
                Action::CSI(CSI::Sgr(Sgr::Intensity(intensity)))
              }
              AttributeChange::Underline(underline) => {
                Action::CSI(CSI::Sgr(Sgr::Underline(underline)))
              }
              AttributeChange::Italic(italic) => {
                Action::CSI(CSI::Sgr(Sgr::Italic(italic)))
              }
              AttributeChange::Blink(blink) => {
                Action::CSI(CSI::Sgr(Sgr::Blink(blink)))
              }
              AttributeChange::Reverse(reverse) => {
                Action::CSI(CSI::Sgr(Sgr::Inverse(reverse)))
              }
              AttributeChange::StrikeThrough(on) => {
                Action::CSI(CSI::Sgr(Sgr::StrikeThrough(on)))
              }
              AttributeChange::Invisible(invisible) => {
                Action::CSI(CSI::Sgr(Sgr::Invisible(invisible)))
              }
              AttributeChange::Foreground(color_attribute) => {
                let color = match color_attribute {
                  ColorAttribute::TrueColorWithPaletteFallback(
                    srgba_tuple,
                    _,
                  ) => ColorSpec::TrueColor(srgba_tuple),
                  ColorAttribute::TrueColorWithDefaultFallback(srgba_tuple) => {
                    ColorSpec::TrueColor(srgba_tuple)
                  }
                  ColorAttribute::PaletteIndex(idx) => {
                    ColorSpec::PaletteIndex(idx)
                  }
                  ColorAttribute::Default => ColorSpec::Default,
                };
                Action::CSI(CSI::Sgr(Sgr::Foreground(color)))
              }
              AttributeChange::Background(color_attribute) => {
                let color = match color_attribute {
                  ColorAttribute::TrueColorWithPaletteFallback(
                    srgba_tuple,
                    _,
                  ) => ColorSpec::TrueColor(srgba_tuple),
                  ColorAttribute::TrueColorWithDefaultFallback(srgba_tuple) => {
                    ColorSpec::TrueColor(srgba_tuple)
                  }
                  ColorAttribute::PaletteIndex(idx) => {
                    ColorSpec::PaletteIndex(idx)
                  }
                  ColorAttribute::Default => ColorSpec::Default,
                };
                Action::CSI(CSI::Sgr(Sgr::Background(color)))
              }
              AttributeChange::Hyperlink(hyperlink) => {
                Action::OperatingSystemCommand(Box::new(
                  OperatingSystemCommand::SetHyperlink(
                    hyperlink.map(|l| l.as_ref().clone()),
                  ),
                ))
              }
            };
            // WezTerm on Windows doesn't support colors in form
            // `CSI 38:2::0:0:0m`, use ';' instead.
            match action {
              Action::CSI(CSI::Sgr(Sgr::Foreground(fg))) => match fg {
                ColorSpec::Default => write!(std::io::stdout(), "\x1b[39m")?,
                ColorSpec::PaletteIndex(idx @ 0..=7) => {
                  write!(std::io::stdout(), "\x1b[{}m", 30 + idx)?;
                }
                ColorSpec::PaletteIndex(idx @ 8..=15) => {
                  write!(std::io::stdout(), "\x1b[{}m", 90 - 8 + idx)?;
                }
                ColorSpec::PaletteIndex(idx) => {
                  write!(std::io::stdout(), "\x1b[38;5;{}m", idx)?;
                }
                ColorSpec::TrueColor(srgba_tuple) => {
                  let (r, g, b, _a) = srgba_tuple.as_rgba_u8();
                  write!(std::io::stdout(), "\x1b[38;2;{};{};{}m", r, g, b)?;
                }
              },
              Action::CSI(CSI::Sgr(Sgr::Background(bg))) => match bg {
                ColorSpec::Default => write!(std::io::stdout(), "\x1b[49m")?,
                ColorSpec::PaletteIndex(idx @ 0..=7) => {
                  write!(std::io::stdout(), "\x1b[{}m", 40 + idx)?;
                }
                ColorSpec::PaletteIndex(idx @ 8..=15) => {
                  write!(std::io::stdout(), "\x1b[{}m", 100 + idx)?;
                }
                ColorSpec::PaletteIndex(idx) => {
                  write!(std::io::stdout(), "\x1b[48;5;{}m", idx)?;
                }
                ColorSpec::TrueColor(srgba_tuple) => {
                  let (r, g, b, _a) = srgba_tuple.as_rgba_u8();
                  write!(std::io::stdout(), "\x1b[48;2;{};{};{}m", r, g, b)?;
                }
              },
              _ => {
                write!(std::io::stdout(), "{}", action)?;
              }
            }
          }
          SrvToClt::ResetAttrs => {
            let action = Action::CSI(CSI::Sgr(Sgr::Reset));
            write!(std::io::stdout(), "{}", action)?;
          }
          SrvToClt::SetCursor { x, y } => {
            let action = Action::CSI(CSI::Cursor(
              termwiz::escape::csi::Cursor::Position {
                line: OneBased::from_zero_based(y.into()),
                col: OneBased::from_zero_based(x.into()),
              },
            ));
            write!(stdout(), "{}", action)?;
          }
          SrvToClt::ShowCursor => {
            let action = Action::CSI(CSI::Mode(
              termwiz::escape::csi::Mode::SetDecPrivateMode(
                termwiz::escape::csi::DecPrivateMode::Code(
                  termwiz::escape::csi::DecPrivateModeCode::ShowCursor,
                ),
              ),
            ));
            write!(stdout(), "{}", action)?;
          }
          SrvToClt::HideCursor => {
            let action = Action::CSI(CSI::Mode(
              termwiz::escape::csi::Mode::ResetDecPrivateMode(
                termwiz::escape::csi::DecPrivateMode::Code(
                  termwiz::escape::csi::DecPrivateModeCode::ShowCursor,
                ),
              ),
            ));
            write!(stdout(), "{}", action)?;
          }
          SrvToClt::CursorShape(cursor_style) => {
            let cursor_style = match cursor_style {
              CursorStyle::Default => {
                termwiz::escape::csi::CursorStyle::Default
              }
              CursorStyle::BlinkingBlock => {
                termwiz::escape::csi::CursorStyle::BlinkingBlock
              }
              CursorStyle::SteadyBlock => {
                termwiz::escape::csi::CursorStyle::SteadyBlock
              }
              CursorStyle::BlinkingUnderline => {
                termwiz::escape::csi::CursorStyle::BlinkingUnderline
              }
              CursorStyle::SteadyUnderline => {
                termwiz::escape::csi::CursorStyle::SteadyUnderline
              }
              CursorStyle::BlinkingBar => {
                termwiz::escape::csi::CursorStyle::BlinkingBar
              }
              CursorStyle::SteadyBar => {
                termwiz::escape::csi::CursorStyle::SteadyBar
              }
            };
            let action = Action::CSI(CSI::Cursor(
              termwiz::escape::csi::Cursor::CursorStyle(cursor_style),
            ));
            write!(stdout(), "{}", action)?;
          }
          SrvToClt::Clear => {
            let action = Action::CSI(CSI::Edit(
              termwiz::escape::csi::Edit::EraseInDisplay(
                termwiz::escape::csi::EraseInDisplay::EraseDisplay,
              ),
            ));
            write!(stdout(), "{}", action)?;
          }
          SrvToClt::Flush => {
            stdout().flush()?;
          }
          SrvToClt::Quit => break,
        },
        _ => break,
      },
      LocalEvent::TermEvent(event) => match event? {
        Some(event) => sender.send(CltToSrv::Key(event))?,
        _ => break,
      },
    }
  }

  Ok(())
}
