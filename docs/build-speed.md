# Build speed

Findings and changes from the 2026-10-23 build audit (#486).

## Done

| Change | Where | Effect |
| --- | --- | --- |
| `debug = "line-tables-only"` for dev and test | `Cargo.toml` | Clean builds 15% faster, core-file rebuilds 19% faster, target dir 44% smaller (8.8 GB to 4.9 GB). Panics and backtraces still show file:line. |
| aarch64 links use the rust-lld bundled with rustup | `.cargo/config.toml`, `scripts/aarch64-lld-cc` | `touch src/qa_env.rs && cargo test --lib --no-run` on m1pro16: GNU ld 26.0 s / 25.2 s, lld 13.7 s / 13.0 s / 13.0 s (about 2x). |
| LibRaw `*_ph.cpp` placeholder stubs are no longer compiled | `crates/omadesign-libraw/build.rs` | Removes duplicate placeholder symbols and their compile time. |
| `brush_qa` compiles again | `src/bin/brush_qa.rs` | `cargo build --bins` works. |
| Cargo workspace with the native bridges in their own crates | `Cargo.toml`, `crates/omadesign-lcms`, `crates/omadesign-libraw` | Little CMS and LibRaw (plus the C++ bridge) build in their own crates, in parallel with the app's dependencies, and their objects are cached apart from app edits. The app crate itself no longer contains `unsafe`. |

### Linker wrapper

`scripts/aarch64-lld-cc` runs `aarch64-linux-gnu-gcc -fuse-ld=lld -B<sysroot>/lib/rustlib/aarch64-unknown-linux-gnu/bin/gcc-ld`.
No system mold or lld is needed. The wrapper falls back to GNU ld when the bundled lld is missing.
Set `OMA_LINKER_GNU_LD=1` to force GNU ld, for example to compare timings. `OMA_AARCH64_CC` overrides the gcc driver.
x86_64 is unchanged because it already uses a fast linker.

## Not done yet

- **Split the application crate into domain crates.** The workspace exists now (see above), but the app is still one crate. It is 53% of build time and leaves about half the cores idle. This is the biggest win, but it is a multi-week effort and the audit schedules it for after the consolidation.

### Workspace layout

- `.` is the `omadesign` application crate (lib, `omadesign` and QA binaries).
- `crates/omadesign-lcms` compiles `vendor/lcms2` and owns the `Profile` and `Transform` handles.
- `crates/omadesign-libraw` compiles `vendor/libraw` and `src/bridge.cpp`, and exposes `decode` with a `Host` trait for cancellation, progress and memory admission.
- `vendor/affinity` is a member because the app depends on it by path, so `--workspace` also runs its tests. The patched upstream crates in `vendor/` (eframe, egui-winit, winit, resvg, usvg) are not members and build only as dependencies.

Run the suite across the workspace with `cargo test --workspace --lib`.
