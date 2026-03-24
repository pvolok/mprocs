use std::{cell::RefCell, io::Write, rc::Rc, sync::Arc, time::Duration};

use rquickjs::{function::Opt, Ctx, Exception, Function, Object};

use crate::{
  js::rquickjs_ext::ObjectExt,
  key::KeyEventKind,
  mouse::{MouseButton, MouseEventKind},
  protocol::CursorStyle,
  term::{term_driver::TermDriver, TermEvent},
  vt100::{
    attrs::{Attrs, Color},
    grid::{Pos, Rect},
    Grid, ScreenDiffer,
  },
};

struct TuiState {
  term_driver: TermDriver,
  differ: ScreenDiffer,
  grid: Rc<RefCell<Grid>>,
  draw_buf: String,
}

unsafe impl<'js> rquickjs::JsLifetime<'js> for TuiState {
  type Changed<'to> = TuiState;
}

struct TuiStore(Arc<tokio::sync::Mutex<Option<TuiState>>>);

unsafe impl<'js> rquickjs::JsLifetime<'js> for TuiStore {
  type Changed<'to> = TuiStore;
}

fn get_tui_store(
  ctx: &Ctx<'_>,
) -> rquickjs::Result<Arc<tokio::sync::Mutex<Option<TuiState>>>> {
  let guard = ctx.userdata::<TuiStore>().ok_or_else(|| {
    Exception::throw_message(
      ctx,
      "tui: storage is not initialized; reload dk.tui module",
    )
  })?;
  Ok(Arc::clone(&guard.0))
}

fn lock_tui_store<'a>(
  ctx: &Ctx<'_>,
  op: &str,
  store: &'a tokio::sync::Mutex<Option<TuiState>>,
) -> rquickjs::Result<tokio::sync::MutexGuard<'a, Option<TuiState>>> {
  store.try_lock().map_err(|_| {
    Exception::throw_message(ctx, &format!("tui.{op}: busy on another thread"))
  })
}

fn with_tui_state_sync<R>(
  ctx: &Ctx<'_>,
  op: &str,
  f: impl FnOnce(&mut TuiState) -> rquickjs::Result<R>,
) -> rquickjs::Result<R> {
  let store = get_tui_store(ctx)?;
  let mut guard = lock_tui_store(ctx, op, &store)?;
  let Some(state) = guard.as_mut() else {
    return Err(Exception::throw_message(
      ctx,
      "tui: not opened; call dk.tui.open()",
    ));
  };
  f(state)
}

fn map_io_error<T>(
  ctx: &Ctx<'_>,
  op: &str,
  result: std::io::Result<T>,
) -> rquickjs::Result<T> {
  result.map_err(|e| Exception::throw_message(ctx, &format!("tui.{op}: {e}")))
}

fn open_fn(ctx: Ctx<'_>) -> rquickjs::Result<()> {
  let store = get_tui_store(&ctx)?;
  let mut guard = lock_tui_store(&ctx, "open", &store)?;
  if guard.is_some() {
    return Ok(());
  }

  let term_driver = TermDriver::create()
    .map_err(|e| Exception::throw_message(&ctx, &format!("tui.open: {e}")))?;
  let size = term_driver
    .size()
    .map_err(|e| Exception::throw_message(&ctx, &format!("tui.open: {e}")))?;

  *guard = Some(TuiState {
    term_driver,
    differ: ScreenDiffer::new(),
    grid: Rc::new(RefCell::new(Grid::new(size, 0))),
    draw_buf: String::new(),
  });
  Ok(())
}

fn close_fn(ctx: Ctx<'_>) -> rquickjs::Result<()> {
  let store = get_tui_store(&ctx)?;
  let mut guard = lock_tui_store(&ctx, "close", &store)?;
  let _ = guard.take();
  Ok(())
}

fn size_fn(ctx: Ctx<'_>) -> rquickjs::Result<Object<'_>> {
  with_tui_state_sync(&ctx, "size", |tui| {
    let size = map_io_error(&ctx, "size", tui.term_driver.size())?;
    let out = Object::new(ctx.clone())?;
    out.set("width", size.width)?;
    out.set("height", size.height)?;
    Ok(out)
  })
}

async fn input_fn(
  ctx: Ctx<'_>,
  Opt(timeout_ms): Opt<f64>,
) -> rquickjs::Result<Option<Object<'_>>> {
  let store = get_tui_store(&ctx)?;
  let mut guard = lock_tui_store(&ctx, "input", &store)?;
  let tui = guard.as_mut().ok_or_else(|| {
    Exception::throw_message(&ctx, "tui: not opened; call dk.tui.open()")
  })?;

  let event = if let Some(timeout_ms) = timeout_ms {
    if timeout_ms.is_nan() || timeout_ms < 0.0 {
      return Err(Exception::throw_message(
        &ctx,
        "tui.input: timeout must be >= 0",
      ));
    }
    let timeout_ms = timeout_ms.floor() as u64;

    match tokio::time::timeout(
      Duration::from_millis(timeout_ms),
      tui.term_driver.input(),
    )
    .await
    {
      Ok(result) => {
        let event: Option<TermEvent> = result.map_err(|e| {
          Exception::throw_message(&ctx, &format!("tui.input: {e}"))
        })?;
        Some(event)
      }
      Err(_elapsed) => {
        let timeout = Object::new(ctx.clone())?;
        timeout.set("type", "timeout")?;
        return Ok(Some(timeout));
      }
    }
  } else {
    let event: Option<TermEvent> =
      tui.term_driver.input().await.map_err(|e| {
        Exception::throw_message(&ctx, &format!("tui.input: {e}"))
      })?;
    Some(event)
  };

  event_to_js(&ctx, event.flatten())
}

fn event_to_js<'js>(
  ctx: &Ctx<'js>,
  event: Option<TermEvent>,
) -> rquickjs::Result<Option<Object<'js>>> {
  let Some(event) = event else {
    return Ok(None);
  };

  let obj = Object::new(ctx.clone())?;
  match event {
    TermEvent::FocusGained => {
      obj.set("type", "focus")?;
      obj.set("focused", true)?;
    }
    TermEvent::FocusLost => {
      obj.set("type", "focus")?;
      obj.set("focused", false)?;
    }
    TermEvent::Resize(width, height) => {
      obj.set("type", "resize")?;
      obj.set("width", width)?;
      obj.set("height", height)?;
    }
    TermEvent::Paste(text) => {
      obj.set("type", "paste")?;
      obj.set("text", text)?;
    }
    TermEvent::Key(key) => {
      obj.set("type", "key")?;
      obj.set("key", key.to_string())?;
      let kind = match key.kind {
        KeyEventKind::Press => "press",
        KeyEventKind::Repeat => "repeat",
        KeyEventKind::Release => "release",
      };
      obj.set("kind", kind)?;
    }
    TermEvent::Mouse(mouse) => {
      obj.set("type", "mouse")?;
      obj.set("x", mouse.x)?;
      obj.set("y", mouse.y)?;
      let kind = match mouse.kind {
        MouseEventKind::Down(btn) => match btn {
          MouseButton::Left => "down-left",
          MouseButton::Right => "down-right",
          MouseButton::Middle => "down-middle",
        },
        MouseEventKind::Up(btn) => match btn {
          MouseButton::Left => "up-left",
          MouseButton::Right => "up-right",
          MouseButton::Middle => "up-middle",
        },
        MouseEventKind::Drag(btn) => match btn {
          MouseButton::Left => "drag-left",
          MouseButton::Right => "drag-right",
          MouseButton::Middle => "drag-middle",
        },
        MouseEventKind::Moved => "moved",
        MouseEventKind::ScrollDown => "scroll-down",
        MouseEventKind::ScrollUp => "scroll-up",
        MouseEventKind::ScrollLeft => "scroll-left",
        MouseEventKind::ScrollRight => "scroll-right",
      };
      obj.set("kind", kind)?;
    }
  }

  Ok(Some(obj))
}

fn draw_fn<'js>(ctx: Ctx<'js>, cb: Function<'js>) -> rquickjs::Result<()> {
  with_tui_state_sync(&ctx, "draw", |tui| {
    let size = map_io_error(&ctx, "draw", tui.term_driver.size())?;
    let grid = tui.grid.clone();
    {
      let mut g = grid.borrow_mut();
      if g.size() != size {
        g.set_size(size);
      }
      g.clear();
      g.cursor_pos = None;
      g.cursor_style = CursorStyle::Default;
    }

    let frame = frame_object(&ctx, grid.clone())?;
    cb.call::<_, ()>((frame,))?;

    tui.draw_buf.clear();
    {
      let grid = grid.borrow();
      tui.differ.diff(&mut tui.draw_buf, &*grid).map_err(|e| {
        Exception::throw_message(&ctx, &format!("tui.draw: {e}"))
      })?;
    }

    let mut stdout = std::io::stdout();
    map_io_error(&ctx, "draw", stdout.write_all(tui.draw_buf.as_bytes()))?;
    map_io_error(&ctx, "draw", stdout.flush())?;
    Ok(())
  })
}

fn frame_object<'js>(
  ctx: &Ctx<'js>,
  grid: Rc<RefCell<Grid>>,
) -> rquickjs::Result<Object<'js>> {
  let frame = Object::new(ctx.clone())?;
  {
    let size = grid.borrow().size();
    frame.set("width", size.width)?;
    frame.set("height", size.height)?;
  }

  frame.def_fn("text", {
    let grid = grid.clone();
    move |ctx: Ctx<'js>,
          x: i32,
          y: i32,
          text: String,
          Opt(style): Opt<Object<'js>>|
          -> rquickjs::Result<()> {
      if x < 0 || y < 0 {
        return Ok(());
      }
      let x = x as u16;
      let y = y as u16;

      let mut grid = grid.borrow_mut();
      let size = grid.size();
      if y >= size.height || x >= size.width {
        return Ok(());
      }

      let attrs = parse_attrs(&ctx, style.as_ref())?;
      let area = Rect {
        x,
        y,
        width: size.width - x,
        height: 1,
      };
      grid.draw_text(area, &text, attrs);
      Ok(())
    }
  })?;

  frame.def_fn("clear", {
    let grid = grid.clone();
    move |ctx: Ctx<'js>,
          Opt(ch): Opt<String>,
          Opt(style): Opt<Object<'js>>|
          -> rquickjs::Result<()> {
      let fill = ch.and_then(|s| s.chars().next()).unwrap_or(' ');
      let attrs = parse_attrs(&ctx, style.as_ref())?;
      let mut grid = grid.borrow_mut();
      let area = grid.area();
      grid.fill_area(area, fill, attrs);
      Ok(())
    }
  })?;

  frame.def_fn("hideCursor", {
    let grid = grid.clone();
    move || {
      grid.borrow_mut().cursor_pos = None;
    }
  })?;

  frame.def_fn("setCursor", {
    let grid = grid.clone();
    move |x: i32, y: i32| {
      if x < 0 || y < 0 {
        return;
      }
      let x = x as u16;
      let y = y as u16;
      let mut grid = grid.borrow_mut();
      let size = grid.size();
      if x < size.width && y < size.height {
        grid.cursor_pos = Some(Pos { col: x, row: y });
      }
    }
  })?;

  frame.def_fn("setCursorStyle", {
    let grid = grid.clone();
    move |style: String| {
      let style = match style.as_str() {
        "default" => CursorStyle::Default,
        "blinkingBlock" => CursorStyle::BlinkingBlock,
        "steadyBlock" => CursorStyle::SteadyBlock,
        "blinkingUnderline" => CursorStyle::BlinkingUnderline,
        "steadyUnderline" => CursorStyle::SteadyUnderline,
        "blinkingBar" => CursorStyle::BlinkingBar,
        "steadyBar" => CursorStyle::SteadyBar,
        _ => CursorStyle::Default,
      };
      grid.borrow_mut().cursor_style = style;
    }
  })?;

  Ok(frame)
}

fn parse_attrs(
  ctx: &Ctx<'_>,
  style: Option<&Object<'_>>,
) -> rquickjs::Result<Attrs> {
  let mut attrs = Attrs::default();
  let Some(style) = style else {
    return Ok(attrs);
  };

  if let Some(color) = parse_color(ctx, style, "fg")? {
    attrs.fgcolor = color;
  }
  if let Some(color) = parse_color(ctx, style, "bg")? {
    attrs.bgcolor = color;
  }
  if let Ok(Some(v)) = style.get::<_, Option<bool>>("bold") {
    attrs.set_bold(v);
  }
  if let Ok(Some(v)) = style.get::<_, Option<bool>>("italic") {
    attrs.set_italic(v);
  }
  if let Ok(Some(v)) = style.get::<_, Option<bool>>("underline") {
    attrs.set_underline(v);
  }
  if let Ok(Some(v)) = style.get::<_, Option<bool>>("inverse") {
    attrs.set_inverse(v);
  }
  Ok(attrs)
}

fn parse_color(
  ctx: &Ctx<'_>,
  style: &Object<'_>,
  key: &str,
) -> rquickjs::Result<Option<Color>> {
  if let Ok(Some(idx)) = style.get::<_, Option<i32>>(key) {
    if (0..=255).contains(&idx) {
      return Ok(Some(Color::Idx(idx as u8)));
    }
    return Err(Exception::throw_message(
      ctx,
      &format!("tui.draw: {key} color index must be in 0..=255"),
    ));
  }

  if let Ok(Some(name)) = style.get::<_, Option<String>>(key) {
    let color = match name.as_str() {
      "default" => Color::Default,
      "black" => Color::BLACK,
      "red" => Color::RED,
      "green" => Color::GREEN,
      "yellow" => Color::YELLOW,
      "blue" => Color::BLUE,
      "magenta" => Color::MAGENTA,
      "cyan" => Color::CYAN,
      "white" => Color::WHITE,
      "brightBlack" => Color::BRIGHT_BLACK,
      "brightRed" => Color::BRIGHT_RED,
      "brightGreen" => Color::BRIGHT_GREEN,
      "brightYellow" => Color::BRIGHT_YELLOW,
      "brightBlue" => Color::BRIGHT_BLUE,
      "brightMagenta" => Color::BRIGHT_MAGENTA,
      "brightCyan" => Color::BRIGHT_CYAN,
      "brightWhite" => Color::BRIGHT_WHITE,
      _ => {
        return Err(Exception::throw_message(
          ctx,
          &format!("tui.draw: unknown color name: {name}"),
        ));
      }
    };
    return Ok(Some(color));
  }

  if let Ok(Some(rgb)) = style.get::<_, Option<Object<'_>>>(key) {
    if let (Ok(r), Ok(g), Ok(b)) = (
      rgb.get::<_, i32>("r"),
      rgb.get::<_, i32>("g"),
      rgb.get::<_, i32>("b"),
    ) {
      if (0..=255).contains(&r)
        && (0..=255).contains(&g)
        && (0..=255).contains(&b)
      {
        return Ok(Some(Color::Rgb(r as u8, g as u8, b as u8)));
      }
      return Err(Exception::throw_message(
        ctx,
        &format!("tui.draw: {key} rgb components must be in 0..=255"),
      ));
    }
  }

  Ok(None)
}

pub fn init(ctx: Ctx<'_>) -> rquickjs::Result<Object<'_>> {
  if ctx.userdata::<TuiStore>().is_none() {
    let store = TuiStore(Arc::new(tokio::sync::Mutex::new(None)));
    ctx.store_userdata(store).map_err(|_| {
      Exception::throw_message(
        &ctx,
        "tui: failed to initialize runtime storage",
      )
    })?;
  }

  let obj = Object::new(ctx.clone())?;

  obj.def_fn("open", open_fn)?;
  obj.def_fn("close", close_fn)?;
  obj.def_fn("size", size_fn)?;
  obj.def_fn_async("input", input_fn)?;
  obj.def_fn("draw", draw_fn)?;

  Ok(obj)
}
