use std::path::Path;

pub fn spawn_server_daemon(working_dir: &Path) -> anyhow::Result<()> {
  let exe = std::env::current_exe()?;
  let canonical_dir = dunce::canonicalize(working_dir)?;
  let dir_str = canonical_dir.to_string_lossy().into_owned();

  #[cfg(unix)]
  return self::unix::spawn_impl(exe, &dir_str);
  #[cfg(windows)]
  return self::windows::spawn_impl(exe, &dir_str);
}

#[cfg(unix)]
mod unix {
  use std::{ffi::CString, path::PathBuf};

  use anyhow::bail;

  pub fn spawn_impl(exe: PathBuf, dir: &str) -> anyhow::Result<()> {
    let daemon =
      daemonize::Daemonize::new().working_directory(std::env::current_dir()?);

    match daemon.execute() {
      daemonize::Outcome::Parent(_) => (),
      daemonize::Outcome::Child(_) => exec(&[
        exe.to_str().ok_or_else(|| {
          anyhow::format_err!("Failed to convert exe path: {:?}", exe)
        })?,
        "server",
        "run",
        "--dir",
        dir,
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

  use windows::Win32::System::Threading::{
    CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
  };

  pub fn spawn_impl(path: PathBuf, dir: &str) -> anyhow::Result<()> {
    use std::{os::windows::process::CommandExt, process::Stdio};

    std::process::Command::new(path)
      .args(["server", "run", "--dir", dir])
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .creation_flags(CREATE_NEW_PROCESS_GROUP.0 | DETACHED_PROCESS.0)
      .spawn()?;

    Ok(())
  }
}
