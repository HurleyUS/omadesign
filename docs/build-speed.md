# Build speed

Findings and changes from the 2026-10-23 build audit (#486).

## Done

| Change | Where | Effect |
| --- | --- | --- |
| `debug = "line-tables-only"` for dev and test | `Cargo.toml` | Clean builds 15% faster, core-file rebuilds 19% faster, target dir 44% smaller (8.8 GB to 4.9 GB). Panics and backtraces still show file:line. |
| aarch64 links use the rust-lld bundled with rustup | `.cargo/config.toml`, `scripts/aarch64-lld-cc` | `touch src/qa_env.rs && cargo test --lib --no-run` on m1pro16: GNU ld 26.0 s / 25.2 s, lld 13.7 s / 13.0 s / 13.0 s (about 2x). |
| LibRaw `*_ph.cpp` placeholder stubs are no longer compiled | `build.rs` | Removes duplicate placeholder symbols and their compile time. |
| `brush_qa` compiles again | `src/bin/brush_qa.rs` | `cargo build --bins` works. |

### Linker wrapper

`scripts/aarch64-lld-cc` runs `aarch64-linux-gnu-gcc -fuse-ld=lld -B<sysroot>/lib/rustlib/aarch64-unknown-linux-gnu/bin/gcc-ld`.
No system mold or lld is needed. The wrapper falls back to GNU ld when the bundled lld is missing.
Set `OMA_LINKER_GNU_LD=1` to force GNU ld, for example to compare timings. `OMA_AARCH64_CC` overrides the gcc driver.
x86_64 is unchanged because it already uses a fast linker.

## Not done yet

- **Move the LibRaw and lcms2 FFI into their own crate.** This takes about 1 day and saves 9–15 s per clean build. It would move `build.rs`, the vendored C/C++ sources, `src/formats/raw.rs` and `src/deep_color/cms.rs` across a crate boundary. That overlaps the consolidation branch, so it was left out of this fix branch to avoid conflicts.
- **Split the single crate into a workspace.** The one crate is 53% of build time and leaves about half the cores idle. This is the biggest win, but it is a multi-week effort and the audit schedules it for after the consolidation.
