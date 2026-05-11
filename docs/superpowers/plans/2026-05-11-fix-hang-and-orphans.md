# Fix mprocs Hang and Orphaned Children Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop mprocs from hanging when the terminal applies backpressure, and ensure child processes are signaled when mprocs exits (so they don't survive a closed terminal tab).

**Architecture:** Four small, independent fixes in the existing async architecture: (1) move client stdout writes off the runtime worker thread by using `tokio::io::stdout`, (2) gracefully drop disconnected clients instead of panicking, (3) add a Unix signal handler that drives the existing kernel Quit path with a force-kill escalation, (4) silence the ?2026 CSI warning that floods the log file. No new abstractions; no architectural changes.

**Tech Stack:** Rust 2024 edition, tokio (full), rustix, libc. Existing crate at `src/Cargo.toml`. Binary built with `cargo build --release -p mprocs` from `src/`.

**Repo layout reminder:** the crate root is `src/` (i.e. `src/Cargo.toml`, `src/lib.rs`, `src/bin/mprocs.rs`). Source modules live at `src/<module>/...` (e.g. `src/client.rs`, `src/mprocs/app.rs`). All paths below are repo-relative.

**Branch:** `fix/hang-and-orphans`

---

### Task 0: Create feature branch

**Files:** none

- [ ] **Step 1: Create and switch to feature branch**

```bash
cd /Users/lazureykis/src/mprocs && git checkout -b fix/hang-and-orphans
```

Expected: `Switched to a new branch 'fix/hang-and-orphans'`

- [ ] **Step 2: Verify clean state**

```bash
cd /Users/lazureykis/src/mprocs && git status
```

Expected: `nothing to commit, working tree clean`

---

### Task 1: Replace sync stdout with `tokio::io::stdout` in the client loop

**Why:** `std::io::stdout().write_all(...)` in `src/client.rs:52` blocks a tokio worker thread when the terminal stops draining (Ghostty when the window is occluded). That causes the in-process `simplex(8 * 1024)` pipe to fill and the app's render `.send().await` to stall.

**Files:**
- Modify: `src/client.rs`

- [ ] **Step 1: Read current client.rs end-to-end**

Run: `cat src/client.rs` (you already have the file; this is the working tree state confirmation).

Confirm it matches the structure we're modifying: the `LocalEvent::ServerMsg` arm uses synchronous `stdout().write_all(...)` and `stdout().flush()?`.

- [ ] **Step 2: Replace the sync stdout writes with async writes**

Replace the entire contents of `src/client.rs` with:

```rust
use tokio::io::AsyncWriteExt;

use crate::term::TermEvent;
use crate::term::key::{Key, KeyEventKind};
use crate::term_driver::TermDriver;
use crate::{
  daemon::{receiver::MsgReceiver, sender::MsgSender},
  protocol::{CltToSrv, SrvToClt},
};

pub async fn client_main(
  sender: MsgSender<CltToSrv>,
  receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let mut term_driver = TermDriver::create()?;

  client_main_loop(&mut term_driver, sender, receiver).await
}

async fn client_main_loop(
  term_driver: &mut TermDriver,
  mut sender: MsgSender<CltToSrv>,
  mut receiver: MsgReceiver<SrvToClt>,
) -> anyhow::Result<()> {
  let size = term_driver.size()?;
  sender
    .send(CltToSrv::Init {
      width: size.width,
      height: size.height,
    })
    .await?;

  #[derive(Debug)]
  enum LocalEvent {
    ServerMsg(Option<SrvToClt>),
    TermEvent(std::io::Result<Option<TermEvent>>),
  }

  let mut stdout = tokio::io::stdout();

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
            stdout.write_all(text.as_bytes()).await?;
          }
          SrvToClt::Flush => {
            stdout.flush().await?;
          }
          SrvToClt::Quit => break,
          SrvToClt::Rpc(_) => {}
        },
        _ => break,
      },
      LocalEvent::TermEvent(event) => match event? {
        Some(TermEvent::Key(Key {
          kind: KeyEventKind::Release,
          ..
        })) => (),
        Some(event) => sender.send(CltToSrv::Key(event)).await?,
        _ => break,
      },
    }
  }

  // Make sure any buffered bytes hit the terminal before TermDriver::drop
  // restores its state.
  let _ = stdout.flush().await;

  Ok(())
}
```

The changes are: drop the `use std::io::{stdout, Write}` import, add `use tokio::io::AsyncWriteExt`, build a single `tokio::io::stdout()` outside the loop, and `.await` writes and flushes. Final flush before returning so the alt-screen leave sequence in `TermDriver::Drop` isn't preceded by a partially-buffered render.

- [ ] **Step 3: Verify it compiles**

```bash
cd /Users/lazureykis/src/mprocs && cargo check -p mprocs
```

Expected: clean check (warnings allowed; no errors).

- [ ] **Step 4: Commit**

```bash
cd /Users/lazureykis/src/mprocs && git add src/client.rs && git commit -m "client: use tokio::io::stdout to avoid blocking the runtime"
```

---

### Task 2: Don't panic when the client send fails; drop the client instead

**Why:** `src/mprocs/app.rs:201,204` use `.unwrap()` on `client_handle.sender.send(...)`. When a client disconnects (or the simplex pipe closes during shutdown), the send errors and the app crashes mid-render. Combined with the prior backpressure stall this is the second half of the visible "hang then die" cycle.

**Files:**
- Modify: `src/mprocs/app.rs` (around the render-send block at lines 196-205)

- [ ] **Step 1: Replace the render-send block**

Open `src/mprocs/app.rs`. Find the block:

```rust
        for client_handle in &mut self.clients {
          let mut out = String::new();
          client_handle.differ.diff(&mut out, grid).log_ignore();
          client_handle
            .sender
            .send(SrvToClt::Print(out))
            .await
            .unwrap();
          client_handle.sender.send(SrvToClt::Flush).await.unwrap();
        }
```

Replace with:

```rust
        let mut dead_clients: Vec<usize> = Vec::new();
        for (idx, client_handle) in self.clients.iter_mut().enumerate() {
          let mut out = String::new();
          client_handle.differ.diff(&mut out, grid).log_ignore();
          if client_handle
            .sender
            .send(SrvToClt::Print(out))
            .await
            .is_err()
            || client_handle
              .sender
              .send(SrvToClt::Flush)
              .await
              .is_err()
          {
            dead_clients.push(idx);
          }
        }
        // Remove disconnected clients in reverse so indices stay valid.
        for idx in dead_clients.into_iter().rev() {
          self.clients.swap_remove(idx);
        }
```

- [ ] **Step 2: Verify it compiles**

```bash
cd /Users/lazureykis/src/mprocs && cargo check -p mprocs
```

Expected: clean check.

- [ ] **Step 3: Commit**

```bash
cd /Users/lazureykis/src/mprocs && git add src/mprocs/app.rs && git commit -m "app: drop disconnected clients instead of panicking on send"
```

---

### Task 3: Add `kill_all()` helper to `UnixProcessesWaiter`

**Why:** When the signal handler escalates to force-quit, it needs to send SIGKILL to every spawned child PID. The `UnixProcessesWaiter` already tracks every child (it holds a `HashMap<Pid, listener>` keyed on the PIDs `forkpty` returned), so this is the natural spot to expose a "send signal to everything" hook.

**Files:**
- Modify: `src/process/unix_processes_waiter.rs`

- [ ] **Step 1: Add the helper method**

Open `src/process/unix_processes_waiter.rs`. Add this method inside `impl UnixProcessesWaiter` (next to `wait_for`, before `init`):

```rust
  /// Send `sig` to every PID we are currently waiting on. Used during
  /// shutdown to force orphans to exit.
  pub fn signal_all(sig: i32) {
    let pids: Vec<Pid> = match GLOBAL.lock() {
      Ok(guard) => match guard.as_ref() {
        Some(pw) => pw.listeners.keys().copied().collect(),
        None => return,
      },
      Err(_) => return,
    };
    for pid in pids {
      let raw: i32 = pid.as_raw_nonzero().into();
      // SAFETY: kill(2) is a stable libc call; an invalid PID just returns
      // ESRCH which we deliberately ignore.
      unsafe {
        libc::kill(raw, sig);
      }
    }
  }
```

Note: keep the lock scope tight — collect PIDs under the lock, drop it, then send signals. This avoids holding the global mutex across syscalls.

- [ ] **Step 2: Verify it compiles**

```bash
cd /Users/lazureykis/src/mprocs && cargo check -p mprocs
```

Expected: clean check.

- [ ] **Step 3: Commit**

```bash
cd /Users/lazureykis/src/mprocs && git add src/process/unix_processes_waiter.rs && git commit -m "unix_processes_waiter: add signal_all helper for forced shutdown"
```

---

### Task 4: Install SIGHUP/SIGINT/SIGTERM handler

**Why:** mprocs currently only handles SIGCHLD and SIGWINCH. When Ghostty closes the tab, mprocs gets SIGHUP, the default action terminates it, and its `forkpty` children — each a new session leader — survive. We need a handler that drives the existing `KernelCommand::Quit` path and, after a grace period, force-kills the rest.

**Files:**
- Modify: `src/mprocs/mprocs.rs` (around the kernel/client spawn site, lines 226-266)

- [ ] **Step 1: Locate the existing block**

In `src/mprocs/mprocs.rs`, the existing structure (around line 243-264) looks like:

```rust
      #[cfg(unix)]
      crate::process::unix_processes_waiter::UnixProcessesWaiter::init()?;
      let kernel = Kernel::new();
      let pc = kernel.context();
      let app_task_id = create_app_task(config, keymap, &pc);

      let app_sender = pc.get_task_sender(app_task_id);
      tokio::spawn(async move {
        client_loop(
          ClientId(1),
          app_sender,
          (srv_to_clt_sender, clt_to_srv_receiver),
        )
        .await
      });

      tokio::spawn(async {
        kernel.run().await;
        #[cfg(unix)]
        crate::process::unix_processes_waiter::UnixProcessesWaiter::uninit()
          .log_ignore();
      });

      let ret = client_main(clt_to_srv_sender, srv_to_clt_receiver).await;
      drop(logger);
      ret
```

- [ ] **Step 2: Insert the signal task between the kernel context creation and the `kernel.run()` spawn**

Right after `let pc = kernel.context();`, add (Unix-only):

```rust
      #[cfg(unix)]
      {
        let signal_pc = pc.clone();
        tokio::spawn(async move {
          use tokio::signal::unix::{SignalKind, signal};
          let mut hup = match signal(SignalKind::hangup()) {
            Ok(s) => s,
            Err(e) => {
              log::error!("Failed to install SIGHUP handler: {}", e);
              return;
            }
          };
          let mut int = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
              log::error!("Failed to install SIGINT handler: {}", e);
              return;
            }
          };
          let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
              log::error!("Failed to install SIGTERM handler: {}", e);
              return;
            }
          };

          tokio::select! {
            _ = hup.recv() => log::info!("Received SIGHUP, shutting down."),
            _ = int.recv() => log::info!("Received SIGINT, shutting down."),
            _ = term.recv() => log::info!("Received SIGTERM, shutting down."),
          }

          // Graceful: ask the kernel to stop every task.
          signal_pc.send(crate::kernel::kernel_message::KernelCommand::Quit);

          // Grace period for procs to exit on their configured stop signal.
          tokio::time::sleep(std::time::Duration::from_secs(5)).await;

          // Escalation 1: SIGKILL anything still alive.
          crate::process::unix_processes_waiter::UnixProcessesWaiter::signal_all(
            libc::SIGKILL,
          );

          // Escalation 2: second Quit forces the kernel loop to break.
          signal_pc.send(crate::kernel::kernel_message::KernelCommand::Quit);

          // Last-ditch: if the runtime itself is wedged, exit hard after a
          // short additional delay so we never become an unkillable orphan.
          tokio::time::sleep(std::time::Duration::from_secs(2)).await;
          log::warn!("Forcing process exit after shutdown timeout.");
          std::process::exit(0);
        });
      }
```

Place this block immediately before `let app_task_id = create_app_task(config, keymap, &pc);`.

- [ ] **Step 3: Confirm imports**

Both `KernelCommand` and `UnixProcessesWaiter` are referenced via fully-qualified paths, so no new `use` lines are required. `libc` is already a top-level dependency.

- [ ] **Step 4: Verify it compiles**

```bash
cd /Users/lazureykis/src/mprocs && cargo check -p mprocs
```

Expected: clean check.

- [ ] **Step 5: Commit**

```bash
cd /Users/lazureykis/src/mprocs && git add src/mprocs/mprocs.rs && git commit -m "mprocs: handle SIGHUP/SIGINT/SIGTERM with graceful + forced shutdown"
```

---

### Task 5: Silence the `?2026` CSI warning

**Why:** Modern shells (bash 5+, fish, zsh prompts) and TUIs emit `ESC[?2026h` / `ESC[?2026l` (DECSET 2026, synchronized output) constantly. mprocs treats every unknown CSI as a `log::warn!`, which produced 3,612 entries in your `mprocs.log`. We recognise ?2026 as a no-op (we always render full frames anyway) and demote the generic "unimplemented CSI" message to `log::trace!`.

**Files:**
- Modify: `src/term/screen.rs` (the `csi_todo` function at lines 1343-1350, plus the dispatch site that calls it)

- [ ] **Step 1: Find the CSI dispatch site that calls `csi_todo`**

Run:
```bash
cd /Users/lazureykis/src/mprocs && grep -n "csi_todo" src/term/screen.rs
```

Note every call site. There should be at least the function definition and one call site (the catch-all).

- [ ] **Step 2: Add a `?2026` filter and downgrade severity**

Modify `csi_todo` in `src/term/screen.rs` to:

```rust
fn csi_todo(params: &str, intermediate: &str, final_: u8) {
  // DECSET/DECRST 2026 (synchronized output): we always render full frames,
  // so the synchronized-update hint is a no-op for us. Silently accept it
  // instead of flooding logs.
  if intermediate.is_empty() && (final_ == b'h' || final_ == b'l') && params == "?2026"
  {
    return;
  }
  log::trace!(
    "CSI not implemented: ESC [ {} {} {}",
    params,
    intermediate,
    final_ as char
  );
}
```

The check is conservative: only `?2026` followed by `h` or `l` with no intermediate is silenced. Everything else is logged at `trace` instead of `warn` so a debug build with `RUST_LOG=trace` can still see them, but the default release log file stops filling up.

- [ ] **Step 3: Add a minimal unit test**

Inline tests already live in `src/term/screen.rs` style is per-module — but `csi_todo` is private and pure logging, so a dedicated test of its filter is not meaningful. Instead, verify with cargo check (next step) and the manual verification task at the end.

- [ ] **Step 4: Verify it compiles**

```bash
cd /Users/lazureykis/src/mprocs && cargo check -p mprocs
```

Expected: clean check.

- [ ] **Step 5: Commit**

```bash
cd /Users/lazureykis/src/mprocs && git add src/term/screen.rs && git commit -m "term: silence DECSET 2026 (synchronized output) and downgrade unknown CSI to trace"
```

---

### Task 6: Full build + existing test suite

**Files:** none

- [ ] **Step 1: Run all existing tests**

```bash
cd /Users/lazureykis/src/mprocs && cargo test -p mprocs --lib
```

Expected: every existing test passes. New code added in Tasks 1-5 doesn't touch the surfaces those tests cover, so a regression here means revisit the offending task.

- [ ] **Step 2: Build the release binary**

```bash
cd /Users/lazureykis/src/mprocs && cargo build --release -p mprocs
```

Expected: `Finished release [optimized] target(s)`.

The binary lands at `target/release/mprocs`.

---

### Task 7: Manual verification against the user's real config

**Why:** The two user-reported symptoms are environmental (Ghostty backpressure, terminal close). No unit test can cover them; we verify behaviourally with the actual `~/src/uptime/mprocs.yaml`.

**Files:** none

- [ ] **Step 1: Make sure no stale mprocs / children are running before the test**

```bash
pgrep -fl mprocs || echo "no mprocs running"
```

If anything is listed, the user should `kill` it first.

- [ ] **Step 2: Test 1 — hang on backpressure**

Have the user run:

```bash
cd /Users/lazureykis/src/uptime && /Users/lazureykis/src/mprocs/target/release/mprocs
```

Then:
1. Wait for several procs (clickhouse, mongodb, nats, cargo builds) to be producing output.
2. Switch to another window for 30 seconds. (Ghostty: Cmd+Tab to a different app, then leave it.)
3. Switch back to the mprocs window.

Expected: TUI is responsive within ~1 second. Pressing `Up`/`Down` selects different procs immediately. Before the fix, this is where the user observed the hang.

- [ ] **Step 3: Test 2 — closing the terminal tab kills children**

In a fresh terminal:

```bash
cd /Users/lazureykis/src/uptime && /Users/lazureykis/src/mprocs/target/release/mprocs
```

Wait for the procs to start. From another terminal:

```bash
pgrep -fl 'clickhouse server|nats-server|caddy run' | head
```

Note the PIDs. Then close the Ghostty tab containing mprocs (the actual close-tab gesture, not Ctrl-C). Wait ~10 seconds. Then:

```bash
pgrep -fl 'clickhouse server|nats-server|caddy run' | head
```

Expected: empty output (children gone). Before the fix, the children persisted indefinitely.

- [ ] **Step 4: Test 3 — Ctrl+C is graceful, second Ctrl+C escalates**

Run mprocs again. Press Ctrl+C once: procs should receive their configured stop signal and exit; mprocs should exit cleanly within ~5s. If a proc hangs, it gets SIGKILL after the grace period and mprocs still exits.

- [ ] **Step 5: Test 4 — log file no longer floods**

After running mprocs through one of the above tests, check the log:

```bash
wc -l /Users/lazureykis/src/uptime/mprocs.log
grep -c "CSI not implemented" /Users/lazureykis/src/uptime/mprocs.log
```

Expected: the CSI count is at most a small number compared to before (was 3,612 in the captured run). Most should be zero now that ?2026 is filtered.

- [ ] **Step 6: If all four tests pass, mark the plan done**

If any fail, capture: the failing test, the full `mprocs.log`, and `pgrep -fl mprocs` output. Re-enter the systematic-debugging skill before patching.

---

### Task 8: Open the PR

**Files:** none

- [ ] **Step 1: Push the branch**

```bash
cd /Users/lazureykis/src/mprocs && git push -u origin fix/hang-and-orphans
```

- [ ] **Step 2: Open a PR with the standard format**

```bash
cd /Users/lazureykis/src/mprocs && gh pr create --title "Fix hang on terminal backpressure and orphaned children on shutdown" --body "$(cat <<'EOF'
## Summary
- Move client stdout writes to `tokio::io::stdout` so a slow/occluded terminal can't block the runtime worker thread, which was the underlying cause of mprocs becoming unresponsive when switching back to the Ghostty window.
- Install SIGHUP/SIGINT/SIGTERM handlers that drive the existing kernel `Quit` path, with a 5s grace period followed by `SIGKILL` to any survivors via a new `UnixProcessesWaiter::signal_all` helper. Closing the terminal tab now reliably kills child processes instead of leaving them reparented to launchd.
- Stop panicking on disconnected clients in `App::main_loop`; just drop them.
- Silence `?2026` (synchronized output) and downgrade other unknown CSI sequences to trace, so the log file no longer fills up with thousands of warnings.

## Test plan
- [ ] `cargo build --release -p mprocs` succeeds
- [ ] `cargo test -p mprocs --lib` passes
- [ ] Unfocus the Ghostty window for 30s with mprocs running ~15 chatty procs; TUI is responsive on return
- [ ] Close the terminal tab while mprocs is running; child procs (`clickhouse server`, `nats-server`, etc.) exit
- [ ] Single Ctrl+C exits cleanly within 5s; stuck procs get SIGKILL'd and mprocs still exits
- [ ] `mprocs.log` no longer accumulates thousands of `CSI not implemented: ESC [ ?2026` warnings
EOF
)"
```

Expected: the command prints the new PR URL.

---

## Notes for the executor

- **Order matters only for Task 0.** Tasks 1-5 touch disjoint files and can be reviewed independently; do them in numeric order so commit history reads cleanly.
- **Do not skip `cargo check` between tasks.** Each task is small; running check catches mismatches immediately.
- **Granular commits per task.** This matches the user's stated preference for one commit per todo-list item.
- **Avoid the temptation to refactor while you're in these files.** Bug fix only. The user's CLAUDE.md is explicit about this.
- **No `--no-verify` on commits.** If a pre-commit hook fires and fails, fix the underlying issue.
