use std::ffi::CString;

use anyhow::bail;

pub fn spawn_server_daemon() -> anyhow::Result<()> {
  let exe = std::env::current_exe()?;

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
    .into_iter()
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
