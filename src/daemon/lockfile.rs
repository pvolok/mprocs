use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Contents written to the .lock file as JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct LockFileContents {
  pub pid: u32,
  /// Socket path (Unix) or TCP address (Windows).
  pub socket: String,
  /// Canonical absolute path of the working directory this daemon manages.
  pub working_dir: String,
  /// Unix timestamp (seconds) when the daemon started.
  pub started_at: u64,
  /// Version of the binary that created this daemon.
  pub version: String,
}

/// Held by the daemon process for its entire lifetime.
/// Dropping this releases the flock and removes the lock + socket files.
pub struct LockFileGuard {
  lock_path: PathBuf,
  socket_path: PathBuf,
  _file: std::fs::File,
}

impl LockFileGuard {
  pub fn socket_path(&self) -> &Path {
    &self.socket_path
  }
}

impl Drop for LockFileGuard {
  fn drop(&mut self) {
    let _ = std::fs::remove_file(&self.socket_path);
    let _ = std::fs::remove_file(&self.lock_path);
  }
}

pub struct DaemonInfo {
  pub contents: LockFileContents,
  pub is_running: bool,
}

fn fnv1a_hash(data: &[u8]) -> u64 {
  let mut hash: u64 = 0xcbf29ce484222325;
  for &byte in data {
    hash ^= byte as u64;
    hash = hash.wrapping_mul(0x100000001b3);
  }
  hash
}

fn dir_to_hash(canonical_path: &Path) -> String {
  let hash = fnv1a_hash(canonical_path.to_string_lossy().as_bytes());
  format!("{:016x}", hash)
}

pub fn get_runtime_dir() -> anyhow::Result<PathBuf> {
  #[cfg(unix)]
  {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
      let mut path = PathBuf::from(dir);
      path.push("dekit");
      return Ok(path);
    }
    let uid = rustix::process::getuid().as_raw();
    let mut path = std::env::temp_dir();
    path.push(format!("dekit-{}", uid));
    Ok(path)
  }
  #[cfg(windows)]
  {
    let local_app_data = std::env::var("LOCALAPPDATA")
      .map_err(|_| anyhow::anyhow!("LOCALAPPDATA not set"))?;
    let mut path = PathBuf::from(local_app_data);
    path.push("dekit");
    path.push("run");
    Ok(path)
  }
}

pub fn daemon_paths(working_dir: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
  let canonical = dunce::canonicalize(working_dir)?;
  let hash = dir_to_hash(&canonical);
  let runtime_dir = get_runtime_dir()?;
  let lock_path = runtime_dir.join(format!("{}.lock", hash));
  let socket_path = runtime_dir.join(format!("{}.sock", hash));
  Ok((lock_path, socket_path))
}

pub fn create_lock_file(working_dir: &Path) -> anyhow::Result<LockFileGuard> {
  let canonical = dunce::canonicalize(working_dir)?;
  let (lock_path, socket_path) = daemon_paths(working_dir)?;

  std::fs::create_dir_all(
    lock_path
      .parent()
      .ok_or_else(|| anyhow::anyhow!("No parent for lock path"))?,
  )?;

  let file = std::fs::OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)
    .open(&lock_path)?;

  acquire_flock(&file)?;

  let contents = LockFileContents {
    pid: std::process::id(),
    socket: socket_path.to_string_lossy().into_owned(),
    working_dir: canonical.to_string_lossy().into_owned(),
    started_at: std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_secs(),
    version: env!("CARGO_PKG_VERSION").to_string(),
  };

  serde_json::to_writer_pretty(&file, &contents)?;

  Ok(LockFileGuard {
    lock_path,
    socket_path,
    _file: file,
  })
}

pub fn read_lock_file(lock_path: &Path) -> Option<LockFileContents> {
  let data = std::fs::read_to_string(lock_path).ok()?;
  serde_json::from_str(&data).ok()
}

pub fn is_daemon_alive(lock_path: &Path) -> bool {
  let file = match std::fs::OpenOptions::new().read(true).open(lock_path) {
    Ok(f) => f,
    Err(_) => return false,
  };
  // Try to acquire exclusive flock. If we succeed, the daemon is dead.
  !try_acquire_flock(&file)
}

pub fn list_daemons() -> anyhow::Result<Vec<DaemonInfo>> {
  let runtime_dir = get_runtime_dir()?;
  let entries = match std::fs::read_dir(&runtime_dir) {
    Ok(e) => e,
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
      return Ok(Vec::new());
    }
    Err(err) => return Err(err.into()),
  };

  let mut daemons = Vec::new();
  for entry in entries {
    let entry = entry?;
    let path = entry.path();
    if path.extension().and_then(|e| e.to_str()) != Some("lock") {
      continue;
    }
    if let Some(contents) = read_lock_file(&path) {
      let is_running = is_daemon_alive(&path);
      daemons.push(DaemonInfo {
        contents,
        is_running,
      });
    }
  }

  Ok(daemons)
}

/// Remove stale lock and socket files for a given working directory.
pub fn cleanup_stale(working_dir: &Path) -> anyhow::Result<()> {
  let (lock_path, socket_path) = daemon_paths(working_dir)?;
  if lock_path.exists() && !is_daemon_alive(&lock_path) {
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(&lock_path);
  }
  Ok(())
}

/// Remove all stale lock and socket files from the runtime directory.
pub fn cleanup_all_stale() -> anyhow::Result<u32> {
  let runtime_dir = get_runtime_dir()?;
  let entries = match std::fs::read_dir(&runtime_dir) {
    Ok(e) => e,
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
      return Ok(0);
    }
    Err(err) => return Err(err.into()),
  };

  let mut count = 0u32;
  for entry in entries {
    let entry = entry?;
    let path = entry.path();
    if path.extension().and_then(|e| e.to_str()) != Some("lock") {
      continue;
    }
    if !is_daemon_alive(&path) {
      // Derive the socket path from the lock path stem.
      let socket_path = path.with_extension("sock");
      let _ = std::fs::remove_file(&socket_path);
      let _ = std::fs::remove_file(&path);
      count += 1;
    }
  }
  Ok(count)
}

pub fn get_daemon_status(
  working_dir: &Path,
) -> anyhow::Result<Option<DaemonInfo>> {
  let (lock_path, _) = daemon_paths(working_dir)?;
  if let Some(contents) = read_lock_file(&lock_path) {
    let is_running = is_daemon_alive(&lock_path);
    Ok(Some(DaemonInfo {
      contents,
      is_running,
    }))
  } else {
    Ok(None)
  }
}

/// Sends SIGTERM (Unix) or terminates the process (Windows).
/// TODO: Connect and send quit command.
pub fn stop_daemon(working_dir: &Path) -> anyhow::Result<()> {
  let (lock_path, _) = daemon_paths(working_dir)?;
  let contents = read_lock_file(&lock_path)
    .ok_or_else(|| anyhow::anyhow!("No daemon found for this directory"))?;

  if !is_daemon_alive(&lock_path) {
    cleanup_stale(working_dir)?;
    anyhow::bail!("Daemon is not running (stale lock file cleaned up)");
  }

  kill_process(contents.pid)?;
  Ok(())
}

#[cfg(unix)]
fn kill_process(pid: u32) -> anyhow::Result<()> {
  use rustix::process::{kill_process as rk, Pid, Signal};
  let pid = Pid::from_raw(pid as i32)
    .ok_or_else(|| anyhow::anyhow!("Invalid PID: {}", pid))?;
  rk(pid, Signal::TERM)
    .map_err(|e| anyhow::anyhow!("Failed to send SIGTERM to daemon: {}", e))
}

#[cfg(windows)]
fn kill_process(pid: u32) -> anyhow::Result<()> {
  use windows::Win32::Foundation::CloseHandle;
  use windows::Win32::System::Threading::{
    OpenProcess, TerminateProcess, PROCESS_TERMINATE,
  };

  unsafe {
    let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
      .map_err(|e| anyhow::anyhow!("Failed to open process {}: {}", pid, e))?;
    let result = TerminateProcess(handle, 1);
    let _ = CloseHandle(handle);
    result.map_err(|e| {
      anyhow::anyhow!("Failed to terminate process {}: {}", pid, e)
    })?;
  }
  Ok(())
}

#[cfg(unix)]
fn acquire_flock(file: &std::fs::File) -> anyhow::Result<()> {
  use std::os::fd::AsFd;
  rustix::fs::flock(
    file.as_fd(),
    rustix::fs::FlockOperation::NonBlockingLockExclusive,
  )
  .map_err(|e| {
    if e == rustix::io::Errno::WOULDBLOCK {
      anyhow::anyhow!("Another daemon is already running for this directory")
    } else {
      anyhow::anyhow!("Failed to acquire lock: {}", e)
    }
  })
}

#[cfg(unix)]
fn try_acquire_flock(file: &std::fs::File) -> bool {
  use std::os::fd::AsFd;
  rustix::fs::flock(
    file.as_fd(),
    rustix::fs::FlockOperation::NonBlockingLockExclusive,
  )
  .is_ok()
}

#[cfg(windows)]
fn acquire_flock(file: &std::fs::File) -> anyhow::Result<()> {
  use std::os::windows::io::AsRawHandle;
  use windows::Win32::Foundation::HANDLE;
  use windows::Win32::Storage::FileSystem::{
    LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
  };

  let handle = HANDLE(file.as_raw_handle() as _);
  let mut overlapped = unsafe { std::mem::zeroed() };
  let result = unsafe {
    LockFileEx(
      handle,
      LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
      0,
      1,
      0,
      &mut overlapped,
    )
  };
  if result.is_err() {
    anyhow::bail!("Another daemon is already running for this directory");
  }
  Ok(())
}

#[cfg(windows)]
fn try_acquire_flock(file: &std::fs::File) -> bool {
  use std::os::windows::io::AsRawHandle;
  use windows::Win32::Foundation::HANDLE;
  use windows::Win32::Storage::FileSystem::{
    LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
  };

  let handle = HANDLE(file.as_raw_handle() as _);
  let mut overlapped = unsafe { std::mem::zeroed() };
  let result = unsafe {
    LockFileEx(
      handle,
      LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
      0,
      1,
      0,
      &mut overlapped,
    )
  };
  result.is_ok()
}
