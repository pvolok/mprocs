//! Reproducer / regression test for the "hang on terminal backpressure" bug
//! documented in docs/bugs/2026-05-11-hang-on-terminal-backpressure.md.
//!
//! Scenario, modelled on what happens when the user unfocuses the Ghostty
//! window:
//!
//!  1. Spawn `mprocs` on the slave side of a real PTY with a handful of
//!     chatty procs (`yes`).
//!  2. Drain the master end for a warmup period so mprocs reaches steady
//!     state and procs are producing output continuously.
//!  3. Stop reading from the master end for several seconds. The kernel pty
//!     buffer fills, every `write_all` from mprocs onto its stdout blocks,
//!     and the in-process simplex pipe + unbounded mpsc queues back up.
//!  4. Send `Q` (the `ForceQuit` keybinding) to mprocs' stdin via the
//!     master.
//!  5. Resume draining the master so the blocked stdout writes can complete.
//!  6. Measure how long it takes mprocs to exit after the `Q` keystroke.
//!
//! Before the fix: mprocs' client task is parked on the synchronous
//! `std::io::stdout().write_all`. While parked it cannot pull the `Q`
//! event from the term_driver and forward it to the server, so the
//! `ForceQuit` event arrives only after the entire backed-up render
//! pipeline drains. We routinely observe 10-30+ seconds in that window.
//!
//! After the fix: the stdout write goes through `tokio::io::stdout` and
//! does not block a runtime worker. The client task interleaves writes and
//! input reads, the `Q` is forwarded promptly, and mprocs exits in under
//! a second.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const MPROCS_BIN: &str = env!("CARGO_BIN_EXE_mprocs");

const WARMUP: Duration = Duration::from_secs(2);
const BACKPRESSURE_DURATION: Duration = Duration::from_secs(8);
const QUIT_LATENCY_THRESHOLD: Duration = Duration::from_secs(1);
const OVERALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Number of `seq` workers we spawn. Each produces ~1MB/s of varied output
/// (incrementing integers), so the screen-diff renderer cannot coalesce
/// them away the way it would for repeated `yes hello` lines.
const NUM_WORKERS: usize = 10;

#[test]
fn quit_latency_under_backpressure() {
  let pty_system = native_pty_system();
  let pair = pty_system
    .openpty(PtySize {
      rows: 40,
      cols: 120,
      pixel_width: 0,
      pixel_height: 0,
    })
    .expect("openpty");

  // Each worker emits a varied stream (incrementing integers) so the
  // screen-diff renderer cannot coalesce frames; every render produces a
  // genuinely new diff. Together they saturate the simplex pipe between
  // the server and client tasks and the unbounded mpsc inboxes upstream
  // of it as soon as the PTY master stops draining.
  let mut cmd = CommandBuilder::new(MPROCS_BIN);
  for i in 0..NUM_WORKERS {
    cmd.arg(format!("sh -c 'i=0; while :; do echo {i}-$i; i=$((i+1)); done'"));
  }

  let mut child = pair.slave.spawn_command(cmd).expect("spawn mprocs");
  // Parent must drop its slave handle so EOF semantics work correctly.
  drop(pair.slave);

  let master = pair.master;
  let mut writer = master.take_writer().expect("master writer");
  let reader = master.try_clone_reader().expect("master reader");

  // The reader runs in a dedicated OS thread because portable-pty's reader
  // is a blocking std::io::Read. We toggle reading on/off with a shared
  // flag; when off the thread parks the read in a 1-byte loop the test
  // controls via the flag.
  let reading = Arc::new(Mutex::new(true));
  let bytes_read = Arc::new(Mutex::new(0usize));

  let reader_handle = {
    let reading = Arc::clone(&reading);
    let bytes_read = Arc::clone(&bytes_read);
    thread::spawn(move || {
      let mut reader = reader;
      let mut buf = [0u8; 8 * 1024];
      loop {
        // Park while reading is paused. We poll cheaply rather than use a
        // condvar; this thread only matters during the brief backpressure
        // window of the test.
        if !*reading.lock().unwrap() {
          thread::sleep(Duration::from_millis(20));
          continue;
        }
        match reader.read(&mut buf) {
          Ok(0) => break,
          Ok(n) => *bytes_read.lock().unwrap() += n,
          Err(e) => {
            // EIO is expected when the slave side closes.
            if e.kind() == std::io::ErrorKind::Other
              || e.kind() == std::io::ErrorKind::UnexpectedEof
            {
              break;
            }
            eprintln!("reader error: {e}");
            break;
          }
        }
      }
    })
  };

  // Phase 1: warmup — let mprocs start, init the alt screen, spawn the
  // five `yes` workers.
  thread::sleep(WARMUP);
  let warmup_bytes = *bytes_read.lock().unwrap();
  eprintln!("warmup: {warmup_bytes} bytes drained from master");
  assert!(
    warmup_bytes > 0,
    "did not receive any output during warmup — mprocs likely failed to start"
  );

  // Phase 2: pause draining. Stdout writes inside mprocs begin to block.
  *reading.lock().unwrap() = false;
  thread::sleep(BACKPRESSURE_DURATION);

  // Phase 3: send the ForceQuit key. `Q` is bound to AppEvent::ForceQuit
  // in the default keymap (see settings.rs::add_defaults).
  let quit_sent_at = Instant::now();
  writer.write_all(b"Q").expect("write Q");
  writer.flush().ok();

  // Phase 4: resume draining so any blocked stdout writes inside mprocs
  // can complete. We are now timing how long the entire pipeline takes to
  // process the queued keystroke.
  *reading.lock().unwrap() = true;

  // Phase 5: wait for mprocs to exit.
  let exit_status = wait_with_timeout(&mut child, OVERALL_TIMEOUT);
  let quit_latency = quit_sent_at.elapsed();

  // Make sure the reader thread is done so its byte count is final.
  let _ = reader_handle.join();
  let total_bytes = *bytes_read.lock().unwrap();
  eprintln!(
    "quit latency = {:?} | total bytes drained = {total_bytes}",
    quit_latency
  );

  let status = exit_status.unwrap_or_else(|| {
    let _ = child.kill();
    panic!(
      "mprocs did not exit within {OVERALL_TIMEOUT:?} after Q keystroke \
             (latency so far: {quit_latency:?}). The hang bug is present."
    )
  });

  // Any exit is fine — the bug is about LATENCY, not exit code. The check
  // below is the actual regression assertion.
  eprintln!("mprocs exit: {status:?}");

  assert!(
    quit_latency < QUIT_LATENCY_THRESHOLD,
    "quit latency {:?} exceeded threshold {:?} — the hang bug is present",
    quit_latency,
    QUIT_LATENCY_THRESHOLD
  );
}

fn wait_with_timeout(
  child: &mut Box<dyn portable_pty::Child + Send + Sync>,
  timeout: Duration,
) -> Option<portable_pty::ExitStatus> {
  let deadline = Instant::now() + timeout;
  loop {
    match child.try_wait() {
      Ok(Some(status)) => return Some(status),
      Ok(None) => {
        if Instant::now() >= deadline {
          return None;
        }
        thread::sleep(Duration::from_millis(50));
      }
      Err(e) => {
        eprintln!("try_wait error: {e}");
        return None;
      }
    }
  }
}
