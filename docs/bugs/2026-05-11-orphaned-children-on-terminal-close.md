# Bug: Closing the terminal tab leaves mprocs's child processes running

**Discovered:** 2026-05-11
**Affects:** mprocs v0.9.2 (installed from nanobrew). Same code on `master` (commit `5d8c6e9`).
**Severity:** High — silent leakage of long-running daemons (databases, message brokers, build watchers) into the user's session. The user has to track them down and kill them by hand.
**Reporter symptom (verbatim):** "Even if I close the terminal tab, processes keep running."

## Environment that triggers it

- Terminal: Ghostty.
- Workload: `~/src/uptime/mprocs.yaml` — clickhouse server, mongod, nats-server, redis-server, caddy, stripe listen, multiple `cargo run` invocations, four uptime_checker regions, throttlecrab-server. Every one of these is a long-running daemon that doesn't exit unless signaled.
- Trigger: close the Ghostty tab (the tab-close gesture, not Ctrl+C, not `q` inside mprocs) while procs are running. Often performed after the hang documented in the companion bug doc has already made the TUI unresponsive.

## Symptom

After closing the terminal tab, the mprocs process is gone, but `pgrep -fl 'clickhouse server|nats-server|caddy run|stripe listen|cargo'` still lists the children. They keep holding ports, writing log files, and (in the case of cargo) consuming CPU. The user has to `pkill` them manually.

## Root cause

Two compounding facts:

**(1) mprocs installs no SIGHUP / SIGINT / SIGTERM handler.**

A grep of `src/` confirms only two signal kinds are handled anywhere in the codebase:

- `SIGCHLD` — `src/process/unix_processes_waiter.rs:44` (used to reap children).
- `SIGWINCH` — `src/term_driver/mod.rs:96-99` (used to forward window resize events).

There is no `tokio::signal::unix::signal(SignalKind::hangup())` anywhere. When Ghostty closes the tab, the kernel sends SIGHUP to the controlling process of the pty (mprocs). The default disposition of SIGHUP is **terminate**, so mprocs dies immediately. No code runs that could clean up children, because there is no handler to run it.

**(2) The children are in their own session, so they don't get SIGHUP from the OS.**

mprocs spawns each child with `libc::forkpty()` (`src/process/unix_process.rs:71-76`). `forkpty(3)` calls `setsid()` in the child before `execvp`, which makes the child the session leader of a *new* session — disconnected from the original controlling terminal. From the OS's perspective, the children are not children of any tty Ghostty closed; they are independent session leaders that happen to have an open pty master fd back to (now-dead) mprocs.

When mprocs dies, the OS:
- closes mprocs's open file descriptors (including the master ends of every child's pty);
- reparents the children to launchd (init);
- does **not** signal them.

The children, none the wiser, keep running. Their stdout/stdin would EIO on the next write/read because the pty master is gone, but most of them just stop writing to stdout and continue serving network requests. Clickhouse and nats explicitly survive a closed-stdin. `cargo r -p ...` survives too — it has spawned its own child (the binary being built), which it doesn't kill.

A grep for `setpgid`, `killpg`, `kill(0,`, or "process_group" in `src/` finds zero results — confirming mprocs has no mechanism to broadcast a signal to its descendants.

## Why this isn't fixed by mprocs exiting "normally"

Even if mprocs's quit hotkey (`q` / Ctrl+C — there's a quit modal: `src/mprocs/modal/quit.rs`) runs, it goes through `KernelCommand::Quit` (`src/kernel/kernel.rs:124-143`), which iterates tasks and calls `TaskCmd::Stop` on each. `Stop` ends up at `proc.stop()` (`src/mprocs/proc/proc.rs:319-322`) which sends the configured `stop_signal` (SIGINT or SIGTERM) to the child. That works **for the cooperative quit path** when the user explicitly quits mprocs.

But that path is never triggered by a terminal-close, because mprocs is dead before it has a chance to run it. The signal-handler gap is the root cause.

## Why the hang bug makes this worse

When the TUI hangs (see the companion bug doc) the user's most natural recovery is "close the tab and start over". That:
1. Sends SIGHUP, which mprocs ignores in the userspace sense (no handler) so the OS terminates it.
2. Leaves the children orphaned with launchd as their new parent.
3. Looks to the user like mprocs was "the thing keeping everything running" — but actually mprocs never was. forkpty deliberately gave each child its own session, and mprocs never tracked them for cleanup beyond the lifetime of its own process.

## Supporting evidence

- `src/process/unix_process.rs:71-76` — `forkpty` call.
- `src/process/unix_process.rs:82-93` — child resets `SIGCHLD/SIGHUP/SIGINT/SIGQUIT/SIGTERM/SIGALRM` to `SIG_DFL` and unblocks them. This is only the child side; the parent never installs handlers for the listed signals.
- `src/process/unix_processes_waiter.rs` — only handles SIGCHLD via tokio.
- `src/term_driver/mod.rs:96-99` — only handles SIGWINCH via tokio.
- Negative evidence: `grep -rEn "SIGHUP|SIGINT|SIGTERM|SignalKind|tokio::signal" src/` returns no main-process handler installation for HUP/INT/TERM.
- Negative evidence: `grep -rEn "setpgid|setsid|killpg|kill\(0" src/` returns zero hits.

## Proposed fix (summary)

1. Install a tokio Unix signal handler for SIGHUP, SIGINT, and SIGTERM at app startup (in `src/mprocs/mprocs.rs`, right after the kernel context is created).
2. On any of those signals: send `KernelCommand::Quit`. The kernel's existing Quit handler already iterates tasks and sends Stop, which already sends the configured stop signal to each child — so cooperative procs exit normally.
3. After a 5-second grace period, escalate: SIGKILL every PID we are still waiting on. Implementation: add a `signal_all(sig: i32)` helper to `UnixProcessesWaiter` that walks its `listeners` map (which is keyed by PID) and calls `libc::kill(pid, sig)`. Then send a second `KernelCommand::Quit` so the kernel's loop breaks out of `is_ready_to_quit`'s wait.
4. As a final safety net, `std::process::exit(0)` after another 2 seconds. This guards against the case where the runtime itself is wedged so badly that even the second Quit can't be processed.

Detailed implementation steps are in `docs/superpowers/plans/2026-05-11-fix-hang-and-orphans.md`.

## Files referenced

- `src/process/unix_process.rs` — `forkpty` spawn site
- `src/process/unix_processes_waiter.rs` — SIGCHLD reaper; would also host the new `signal_all` helper
- `src/term_driver/mod.rs:96-99` — example of how mprocs already uses `tokio::signal::unix::signal`
- `src/kernel/kernel.rs:124-143` — existing `KernelCommand::Quit` handling
- `src/mprocs/proc/proc.rs:319-322` — `proc.stop()` → configured stop signal
- `src/mprocs/mprocs.rs:226-266` — where the signal handler task should be spawned

## Platform notes

- This bug is Unix-only as documented. On Windows, mprocs's lifetime is tied to the console window in a different way and pty children survive console close via a different mechanism. The fix above is `#[cfg(unix)]` and a Windows equivalent (CTRL_CLOSE_EVENT handler via SetConsoleCtrlHandler) is out of scope for this bug; the user's environment is macOS + Ghostty.
- macOS has no `prctl(PR_SET_PDEATHSIG)`; the "die when parent dies" approach used on Linux is not portable here. The grace-period + SIGKILL approach above is portable to both Linux and macOS.
