#[allow(dead_code)]
pub fn spawn_server_daemon() -> anyhow::Result<()> {
  let exe = std::env::current_exe()?;

  #[cfg(unix)]
  return self::unix::spawn_impl(exe);
  #[cfg(windows)]
  return self::windows::spawn_impl(exe);
}

#[cfg(unix)]
mod unix {
  use std::{ffi::CString, path::PathBuf};

  use anyhow::bail;

  #[allow(dead_code)]
  pub fn spawn_impl(exe: PathBuf) -> anyhow::Result<()> {
    let daemon =
      daemonize::Daemonize::new().working_directory(std::env::current_dir()?);

    match daemon.execute() {
      daemonize::Outcome::Parent(_) => (),
      daemonize::Outcome::Child(_) => exec(&[
        exe.to_str().ok_or_else(|| {
          anyhow::format_err!("Failed to convert exe path: {:?}", exe)
        })?,
        "server",
      ])?,
    }

    Ok(())
  }

  #[cfg(unix)]
  fn exec(argv: &[&str]) -> anyhow::Result<()> {
    // Add null terminations to our strings and our argument array,
    // converting them into a C-compatible format.
    let program_cstring = CString::new(
      argv
        .first()
        .ok_or_else(|| anyhow::format_err!("Empty argv"))?
        .as_bytes(),
    )?;
    let arg_cstrings = argv
      .iter()
      .map(|arg| CString::new(arg.as_bytes()))
      .collect::<Result<Vec<_>, _>>()?;
    let mut arg_charptrs: Vec<_> =
      arg_cstrings.iter().map(|arg| arg.as_ptr()).collect();
    arg_charptrs.push(std::ptr::null());

    // Use an `unsafe` block so that we can call directly into C.
    let res =
      unsafe { libc::execvp(program_cstring.as_ptr(), arg_charptrs.as_ptr()) };

    // Handle our error result.
    if res < 0 {
      bail!("Error calling execvp");
    } else {
      // Should never happen.
      panic!("execvp returned unexpectedly")
    }
  }
}

#[cfg(windows)]
mod windows {
  use std::path::PathBuf;

  pub fn spawn_impl(path: PathBuf) -> anyhow::Result<()> {
    use std::{os::windows::process::CommandExt, process::Stdio};

    use winapi::um::winbase::{CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};

    std::process::Command::new(path)
      .arg("server")
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stdout(Stdio::null())
      .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
      .spawn()?;

    Ok(())
  }
}
