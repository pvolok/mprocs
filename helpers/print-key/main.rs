use crossterm::{
  event::{Event, KeyCode},
  terminal::{disable_raw_mode, enable_raw_mode},
};

fn main() {
  println!("Press \"z\" to exit.");

  enable_raw_mode().unwrap();

  loop {
    match crossterm::event::read().unwrap() {
      Event::FocusGained => (),
      Event::FocusLost => (),
      Event::Key(key_event) => {
        print!("{:?}\r\n", key_event);

        if key_event.code == KeyCode::Char('z')
          && key_event.modifiers.is_empty()
        {
          break;
        }
      }
      Event::Mouse(_) => (),
      Event::Paste(_) => (),
      Event::Resize(_, _) => (),
    }
  }

  disable_raw_mode().unwrap();
}
