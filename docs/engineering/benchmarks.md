# Benchmarks: Rocket vs. Bruno and Postman

The full record behind the short benchmark section in the README. Measured on one Linux machine on 2026-10-21 with the installed builds: Rocket 2.1.1, Bruno 4.2.0 and Postman 11.71.7 (snap revision 360).

## Results

| | Rocket 2.1.1 | Bruno 4.2.0 | Postman 11.71.7 |
|---|---|---|---|
| **Runtime** | Rust + OS WebView | Electron | Electron |
| **Install size (unpacked on disk)** | **146 MB** | 527 MB | 653 MB |
| **Download size** | **48.4 MB** (.deb) | not measured | 174 MB (snap) |
| **Idle memory (PSS)** | **~262 MB** (3 processes) | ~566 MB (7 processes) | ~516 MB (7 processes) |
| **Idle private memory** | **151 MB** | 480 MB | 311 MB |
| **Idle CPU** | ~1% | **~0%** | ~7% |
| **Startup → window visible** | 0.43 s (blank) | ~3.5 s | **~1.0 s** |
| **Startup → UI drawn** | ~10.4 s | ~4.2 s | **~2.2 s** |

## Method

**Methodology:** idle memory is PSS (proportional set size, via `/proc/<pid>/smaps_rollup`) summed across each app's full process tree, with no requests sent. PSS avoids double-counting pages shared between an app's own forked processes, which a plain RSS sum would inflate (RSS read 484 MB for Rocket, 867–1052 MB for Bruno and 856–865 MB for Postman). Install size is the space used after installation, so it compares like with like: `dpkg -s rocket` (`Installed-Size`) for Rocket, and `du` of `/opt/Bruno` and `/snap/postman/360` for the two Electron apps. Download size is the installer file as shipped: Rocket's `rocket_2.1.1_amd64.deb` and Postman's snap package; Bruno's installer was not measured. A small script took a 10 s window at ~20, 60, 120 and 250 s after launch; Rocket stayed at 262 MB PSS the whole time, Bruno held 549–631 MB and Postman 515–525 MB. The apps ran one after the other on the same machine, without `systemd-run` isolation. Postman was already signed in with a workspace open, so it was not an empty install.

Startup is wall-clock time from process launch, polled every ~0.1 s over X11 (XWayland, `GDK_BACKEND=x11`, `--ozone-platform=x11` for Bruno). "Window visible" is the main window reaching `IsViewable` at a real size, ignoring small placeholder windows. "UI drawn" is the first moment a capture of that window (`xwd`) has more than 20 distinct colours and no single colour covering 80% or more of it. A window that is up but blank, or showing a flat loading screen, does not count. Five runs for Rocket (10.30–10.62 s), three each for Bruno (3.31–4.30 s) and Postman (2.15–2.26 s). Postman's very first launch after a cold start was slower (8.1 s).

## Findings and history

**Takeaways:**

- **Rocket uses about half the memory of either Electron app:** ~262 MB idle, 2.0x less than Postman (~516 MB) and 2.2x less than Bruno (~566 MB), with 3 processes against 7. Private memory is 151 MB against 311 MB and 480 MB. It also takes about 3.6–4.5x less disk space once installed (146 MB against 527 MB and 653 MB).
- **Rocket is the slowest of the three to start.** The window opens in 0.43 s but is blank until a loading screen at ~1.7 s, and the UI is not drawn until ~10.4 s, against ~4.2 s for Bruno and ~2.2 s for Postman. The web process is the bottleneck: in an earlier 2.1.0 run it sat at about 120% CPU throughout the load. Startup is the one area where the Electron apps are clearly ahead.
- **Idle CPU:** Rocket (~1%) and Bruno (~0%) are quiet at rest. Postman stayed near 7% of one core, most likely background sync because it was signed in, so a fresh install may look lighter.
- **Bruno's old 169.6 MB figure did not reproduce.** Its renderer and zygote processes alone hold about 430 MB RSS. Postman's re-measured ~516 MB agrees with its earlier 507.7 MB, so only Bruno's old figure was off.
- **Earlier Rocket numbers:** 2.1.0 read ~317 MB PSS (measured on the session's default display backend; the 2.1.1 run used X11, which may account for part of the drop), and a debug build read 352 MB. The earlier startup figures (Rocket 6.38 s, Bruno 7.86 s, Postman 0.99 s) used a window-only check and are not comparable with the table above.
- **A previous "171 MB" figure for Rocket was a measurement artifact, not a real number.** All development on this machine runs inside a shared cgroup (`memory.high` capped at 2 GB) alongside the tooling doing the measuring, concurrent builds, and other processes. Under that pressure the kernel reclaims and swaps out pages, and PSS only counts resident memory — so a reading taken mid-pressure can come in dramatically lower than the app's real footprint. We proved this directly: the *same running Rocket instance*, with zero user activity, read 292 MB and then 159 MB twenty seconds later purely from swap, with no code change involved. Measured outside that cgroup in an isolated `systemd-run --user --scope` (`memory.high=max`, confirmed 0 bytes swapped), Rocket 0.9.2 was flat at **~332 MB** from 20 seconds to 180 seconds of idle time — this is the real number. Rocket 2.1.1 read ~262 MB in the 2026-10-21 re-measurement. The original 171 MB figure was a single sample taken immediately after a release build, inside the same pressured cgroup; an empty, freshly-installed Rocket with no data at all already needs more memory than that once actually measured cleanly.
- **The `WebKitCacheModel::DocumentViewer` and `WEBKIT_DISABLE_COMPOSITING_MODE=1` fixes are still real and still necessary** — with compositing left on, WebKitWebProcess's real footprint (measured the same clean way) balloons to **3.5 GB**, not something visible under cgroup pressure. Keep both.
- **One small, real optimization did come out of this investigation:** the sidebar kept its History panel permanently mounted (rendering up to 200 entries, ~2,000 DOM nodes — 77% of the app's entire DOM) even when the user was looking at the Collections tab. It's now deferred until the History tab is actually opened.
- **Startup needs a render check, not just a window check.** Rocket's window is viewable at 0.43 s but blank, so a window-only check would report a startup time 25x too short. The 2026-10-21 numbers wait for drawn pixels, and use the same rule for all three apps.
- **Postman requires an account.** An earlier measurement pass had to sign in through an external OAuth browser flow, which spawned a separate Brave process that was excluded from its footprint. The 2026-10-21 run used an already signed-in Postman, which explains its idle CPU. Bruno and Rocket both work fully offline with no account.

## Re-running

- Memory and CPU: sum PSS (`/proc/<pid>/smaps_rollup`), `RssAnon` and CPU ticks over the app's whole process tree, in a 10 s window at ~20, 60, 120 and 250 s after launch.
- Startup: launch the app, poll every ~0.1 s for the main window with `xdotool` and `xwininfo`, capture it with `xwd`, and count distinct colours. "UI drawn" needs more than 20 distinct colours and no colour covering 80% or more.
- Launch flags: `GDK_BACKEND=x11` for Rocket, `--ozone-platform=x11` for Bruno, plain `/snap/bin/postman` for Postman. A background session needs the live `XAUTHORITY` (`/run/user/<uid>/.mutter-Xwaylandauth.*`, from the `Xwayland -auth` argument).
- Keep Rocket's install up to date first (`dpkg -s rocket`), and close any running Rocket so the new binary is the one measured.
