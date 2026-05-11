# Bug: mprocs hangs when the terminal applies backpressure

**Discovered:** 2026-05-11
**Affects:** mprocs v0.9.2 (installed from nanobrew). Same code present on `master` (commit `5d8c6e9`).
**Severity:** High — user-visible freeze of the whole TUI; requires `kill -9` in the worst case.
**Reporter symptom (verbatim):** "Oftentimes when I run multiple processes in ~/src/uptime when I switch back to a terminal window, I cannot control mprocs or even close it. It just hangs."

## Environment that triggers it

- Terminal: Ghostty.
- Workload: `~/src/uptime/mprocs.yaml`, ~15 simultaneous processes including `clickhouse server`, `mongod`, `nats-server`, `caddy run`, `stripe listen`, multiple `cargo r -p uptime_*` invocations and four `uptime_checker` regions. All very chatty on stdout.
- Trigger: switch focus away from the Ghostty window holding mprocs for some period, then switch back.

## Symptom

The mprocs TUI becomes unresponsive to keyboard input. The process is still running (it shows in `ps`) but it does not redraw, does not respond to keys, and `q` / Ctrl+C have no immediate effect. After a long delay it may recover; sometimes only `kill -9` ends it.

## Root cause

`src/client.rs:51-55` (master), `src/client.rs:51-56` (v0.9.2) writes server-produced bytes to stdout synchronously from inside an async loop:

```rust
SrvToClt::Print(text) => {
  std::io::stdout().write_all(text.as_bytes())?;
}
SrvToClt::Flush => {
  stdout().flush()?;
}
```

`std::io::Stdout::write_all` and `flush` are **blocking** syscalls. When Ghostty stops draining its PTY input — which it does aggressively when its window is occluded or unfocused — the tty buffer fills, and `write_all` blocks on `write(2)` in kernel-space. That blocks the tokio worker thread running the client task.

This cascades through the entire pipeline:

1. **Client task blocked on stdout.** The tokio worker handling it can't run any other tasks until the syscall returns.
2. **Server→client `simplex(8 * 1024)` fills.** `src/mprocs/mprocs.rs:228-238` connects the in-process server and client via a tiny 8 KiB buffer. With the client unable to consume, this fills within one or two render frames at this output volume.
3. **App render `.send().await.unwrap()` stalls.** `src/mprocs/app.rs:199-204`:
   ```rust
   client_handle.sender.send(SrvToClt::Print(out)).await.unwrap();
   client_handle.sender.send(SrvToClt::Flush).await.unwrap();
   ```
   Once the simplex pipe is full, this `.await` parks the App's `main_loop` task indefinitely.
4. **App's main loop stops draining its inbox.** `App::main_loop` (`src/mprocs/app.rs:144-227`) is the only consumer of `self.pr` (TaskCmd mpsc). It is suspended at the `.send().await` above, so it never reaches the `self.pr.recv_many(...)` call.
5. **The kernel and proc tasks keep producing.** Each child process read in `proc_main_loop` (`src/mprocs/proc/proc.rs:142-183`) emits `KernelCommand::TaskRendered`. The kernel (`src/kernel/kernel.rs:252-254`, `apply_effect` → `notify_listeners`) forwards a `TaskNotification` to every listener — including the App — via an **unbounded** mpsc. That queue grows without limit while the App is parked.
6. **Recovery is slow even after the syscall returns.** When the user refocuses the window and the tty drains, the client unblocks, the simplex pipe drains, and the App resumes. But the App now has to chew through every queued event (`recv_many(&mut command_buf, 512)` per iteration, then a re-render and a send). At this output volume the backlog can easily be tens of thousands of events. Each iteration also re-renders and re-sends to the client, so the queue drains slowly. From the user's seat this looks like a long hang.

The `.unwrap()` at `app.rs:201,204` is also a separate failure mode: if the client side closes for any reason (e.g. the terminal genuinely went away), the send returns an error and the App panics inside the render block. The panic hook in `setup_logger` (`src/mprocs/mprocs.rs:43-46`) logs the trace but the App task dies and the TUI freezes a second way.

## Why this user hits it so reliably

The combination of:
- a high-output workload (clickhouse, mongod, nats, cargo build, stripe listen all writing simultaneously),
- a small simplex buffer (8 KiB),
- multiple **unbounded** mpsc channels in the kernel/app pipeline (no upstream backpressure to slow the producers when the consumer stalls),
- and synchronous stdout in the consumer,

means any time the terminal is slow for even a few seconds, the queues grow large enough that recovery takes long enough to be perceived as a hang. In a workload with one or two procs, the same code path exists but rarely produces enough backlog to be visible.

## Supporting evidence

`/Users/lazureykis/src/uptime/mprocs.log` (captured 2026-05-09 20:27):

- 40 × `ERROR [lib::error] Error: channel closed` — these are `tokio::sync::mpsc::error::SendError`'s `Display` impl, fired by `log_ignore()` / `log_get()` paths. The 40 count corresponds to teardown noise after the App panics from one of the `.unwrap()` calls above; the various tasks all try to send onto each other's now-closed channels.
- 1 × `ERROR [lib::proc::proc] Process spawn error: Unknown error: -6` — `EAGAIN` from `fork(2)`/`forkpty(2)` under the resource pressure of spawning many cargo builds.
- 3,612 × `WARN ... CSI not implemented: ESC [ ?2026` — see the separate bug doc on log spam; not a direct cause but it amplifies any I/O contention.

## Confirmed root cause (measured)

An integration test was added at `src/tests/hang_on_backpressure.rs` that drives mprocs through a real PTY, pauses draining of the master for 8 seconds while ten varied workers produce ~10 MB/s of total output, then sends `Q` (ForceQuit) and measures how long mprocs takes to exit.

| Configuration                                       | Quit latency (release build) |
|-----------------------------------------------------|------------------------------|
| Baseline (no fix)                                   | ~3.3 s                       |
| Async stdout in client only                         | ~3.2 s (≤5% improvement)     |
| Async stdout + aggressive drain of App inbox        | **~0.26 s**                  |

Conclusion: **the dominant cost is not the synchronous stdout write — it is the App's "one render per 512 inbox events" cadence multiplied by the unbounded growth of that inbox while the App is parked on `send().await`.**

Once the simplex pipe drains:
- `recv_many(&mut buf, 512)` returns 512 of the queued events.
- The loop processes them and re-renders once.
- The render produces a diff that the client now actually has to push through stdout to the terminal.
- The next iteration drains 512 more. Each iteration emits another diff frame.
- The accumulated output is ~2 MB of diff bytes that must be drained at the terminal's reception rate (~600 KB/s on a real terminal), which is what the user perceives as a ~3–5 s freeze.

The async stdout fix is still worth doing (it removes a latent footgun where the runtime worker is parked in a kernel syscall instead of suspending the task properly), but on its own it does not eliminate the user-visible hang. The decisive fix is to drain the App inbox aggressively per iteration so the post-backpressure recovery collapses into a single render of the latest state rather than replaying every intermediate frame.

## Proposed fix (summary)

1. Replace `std::io::stdout()` in `src/client.rs` with `tokio::io::stdout()` and `AsyncWriteExt::write_all` / `flush`. This moves the blocking syscall onto tokio's blocking-IO worker pool so it cannot stall a runtime worker.
2. In `src/mprocs/app.rs`, replace the two `.unwrap()` calls in the render-send loop with error handling that swap-removes the dead client from `self.clients` and continues the App loop.
3. **In `src/mprocs/app.rs`**, raise the `recv_many` cap from 512 to 16384 and follow it with a `try_recv` drain loop. Also short-circuit the per-command processing loop the moment a `ForceQuit` is observed. This collapses a backed-up inbox into a single render of the final state and lets user-input events take effect immediately instead of waiting for the renderer to replay every intermediate frame.
4. Optional follow-up (not in this fix): cap the kernel→app and app inbox at some bound and switch to drop-oldest semantics for render notifications so a stalled consumer cannot grow the queue without bound. Out of scope here because it touches the kernel core.

Detailed implementation steps are in `docs/superpowers/plans/2026-05-11-fix-hang-and-orphans.md`.

## Files referenced

- `src/client.rs` — sync stdout in async loop (primary defect)
- `src/mprocs/app.rs:196-205` — `.unwrap()` on send to client (secondary defect)
- `src/mprocs/mprocs.rs:228-238` — small `simplex(8 * 1024)` connecting server and client
- `src/kernel/kernel.rs:252-254`, `src/kernel/kernel_message.rs:97-141` — unbounded mpsc producers
- `src/mprocs/proc/proc.rs:142-183` — per-read `TaskRendered` emission
