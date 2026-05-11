# Bug: `mprocs.log` floods with "CSI not implemented: ESC [ ?2026 h/l" warnings

**Discovered:** 2026-05-11
**Affects:** mprocs v0.9.2 (installed from nanobrew). Same code on `master` (commit `5d8c6e9`).
**Severity:** Medium — log file grows unboundedly, signal-to-noise ratio in the log makes other diagnostics hard to find, and the synchronous log writes amplify I/O contention during the unrelated hang bug.
**Reporter symptom:** discovered while investigating the hang bug. The user's `~/src/uptime/mprocs.log` was 220 KB / 3,653 lines, of which 3,612 lines (~99%) were CSI 2026 warnings.

## Symptom

`mprocs.log` accumulates thousands of `WARN [lib::vt100::screen] CSI not implemented: ESC [ ?2026 h` and `... ?2026 l` lines whenever real-world tools (shells, build systems, TUIs) run inside mprocs. Concrete capture from the user:

```
$ wc -l ~/src/uptime/mprocs.log
3653

$ grep -c "CSI not implemented" ~/src/uptime/mprocs.log
3612
```

In a few hours of use the log can grow to many megabytes, dwarfing the legitimate ERROR/WARN entries that would help diagnose real issues.

## Root cause

The terminal emulator's CSI dispatch has a catch-all (`src/term/screen.rs:1343-1350`):

```rust
fn csi_todo(params: &str, intermediate: &str, final_: u8) {
  log::warn!(
    "CSI not implemented: ESC [ {} {} {}",
    params,
    intermediate,
    final_ as char
  );
}
```

Any CSI sequence whose `(params, intermediate, final)` triple isn't matched elsewhere falls here and is logged at WARN. In practice the dominant unhandled triple is:

- `ESC[?2026h` — DECSET 2026, "Begin synchronized update"
- `ESC[?2026l` — DECRST 2026, "End synchronized update"

This is the **synchronized output mode** ("synchronized update", "SU") supported by Kitty, Alacritty, WezTerm, Ghostty, iTerm2, and others. It's a hint: "I'm about to emit several CSI sequences that should be presented atomically; please don't render a half-finished frame in the middle". Modern shells emit it routinely:

- bash 5.x prompt updates
- zsh prompt updates (Powerlevel10k uses it extensively)
- fish 4.x
- tmux 3.4+ pass-through
- many TUIs (htop, btop, top, less, vim, neovim)

Every prompt redraw produces a matched `h`/`l` pair. The user's workload includes:
- `htop` (continuously redrawing)
- many shell prompts (every cargo build warning printed via the user's shell wrapper)
- clickhouse-server's interactive header (rare but present)

Hence the 3,612 lines.

`csi_todo` does not differentiate between known-but-unsupported sequences (where WARN is the wrong level — there's nothing the user can do, nothing for the developer to fix in this code path, and the sequence has a well-defined no-op fallback) and genuinely-unknown sequences (where WARN might be useful at most once per unique triple, but is still too loud at full volume).

## Secondary effect: amplifies the hang bug

`flexi_logger` is configured with `.append()` and writes to the file path returned by `FileSpec::default().suppress_timestamp()`. Each `log::warn!` call hits the log file. While flexi_logger's default writer is reasonably efficient, at thousands of warnings per second it still consumes CPU and adds I/O syscalls that compete with the rest of the runtime — making the backpressure cascade described in the companion hang-bug doc reach the critical point a little sooner.

In other words: even if the hang bug were fixed, the log spam alone is a measurable resource leak on this workload. Fixing it is independent and worth doing on its own merits.

## Why ?2026 is safe to silently ignore

mprocs's renderer is **frame-based**: every render cycle in `App::main_loop` (`src/mprocs/app.rs:177-205`) erases the off-screen `Grid`, redraws all UI from current state, computes a screen-diff against the previous frame (`ScreenDiffer::diff` in `src/term/screen_differ.rs`), and emits the resulting bytes to the client. The vt100 emulator's role for child output is to update the proc's `Parser::screen` so the next render reflects it. There is no per-character flushing to the user's terminal in the middle of a frame.

That means the "synchronized update" hint is a no-op for mprocs: the user already only ever sees fully-formed frames. We don't need to honor `?2026h` (defer flushing) because we never partial-flushed in the first place. So accepting the sequence and doing nothing is correct.

## Proposed fix (summary)

1. In `src/term/screen.rs`, detect `?2026 h` / `?2026 l` specifically in `csi_todo` and return silently (with a one-line comment noting why).
2. Downgrade the remaining catch-all from `log::warn!` to `log::trace!` so the file no longer accumulates spam from other rarely-seen sequences either. Developers can re-enable with `RUST_LOG=trace` when investigating a specific terminal compatibility issue.

Detailed implementation steps are in `docs/superpowers/plans/2026-05-11-fix-hang-and-orphans.md`.

## Files referenced

- `src/term/screen.rs:1343-1350` — `csi_todo` function (sole defect)
- `src/term/screen.rs:1330-1338` — adjacent unhandled-OSC path, already at `debug` level (model for the trace downgrade)
- `src/mprocs/mprocs.rs:32-49` — flexi_logger setup; release builds default to `warn` level, so the fix is sufficient on its own — but a future change might also consider whether `info` is a better default release level once spam is under control

## Related upstream / spec references

- DEC private mode 2026 ("Synchronized Update Mode") — proposed by iTerm2, now de-facto standard across modern terminals.
- See <https://gist.github.com/christianparpart/d8a62cc1ab659194337d73e399004036> for the de-facto spec text.
- Ghostty supports it (`ghostty +show-config --default | grep -i sync` confirms on the user's machine).
