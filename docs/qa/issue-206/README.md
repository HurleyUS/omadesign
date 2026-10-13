# Drawing fixes — issue #206

Base: master `f969a0b2`. Work is isolated from the owner's feature checkout and running application.

## Changed behavior

- A selection whose objects share a rotation uses that rotated frame for its outline, handles, pointer picking, and scaling. Rectangles stay rectangles. Path conversion retains the same frame. Resize undo/redo restores the original geometry.
- Essentials includes a Square tip. The native pack generator and website download contain the same seven tips. Sampled cursors trace the tip's visible edge and turn with the selected angle and stroke direction.
- Sidebar width changes only through an explicit edge drag. Existing saved panel widths are retained; canvas hover and vertical motion cannot resize it.
- Brush spacing ranges from 1% to 10,000% and accumulates across pointer events for round and sampled brushes and erasers.
- Angle, Follow stroke direction, Scatter, angle jitter, size jitter, and opacity jitter are usable brush controls. Each stroke has a fresh random seed. Custom `.omabrush` packs retain the dynamics and accept spacing above 400%.
- Wayland binds the native tablet-v2 protocol and forwards hover, contact, release, cancellation, pressure, and stylus side buttons. X11 discovers the Abs Pressure valuator and forwards normalized pressure before motion. Size and Flow pressure controls are independent.

## Software qualification

- Library suite: **1,006 passed; 0 failed; 9 ignored**. Tests run from a preserved executable, with isolated HOME/XDG directories and a short project-owned temporary directory for Unix sockets.
- New regressions cover multiple rotated rectangles and converted paths, scale/undo/redo, outline and rotated-tip pixels, pen-pressure dab size and release, high spacing through tiny pointer samples, portable dynamics, bounded scatter updates, independent stroke randomness, cancellation, and intentional sidebar resizing.
- `cargo check --all-targets` passes on this Linux ARM64 host.
- Native debug binary build and `--version` execution pass. Candidate: `.worktrees/canvas-brush-fixes/artifacts/omadesign-real-work`. It carries the base nightly version, `0.6.3-nightly.20261011.0001`; this task does not publish a new release.
- Candidate SHA-256: `64a8ee632bd87d0af576a46c412d11e91c2701a7cca01ce57d176ab6f5066c9b`.
- `git diff --check` passes. Build outputs, logs, local worktrees, and environment files are ignored.

## Qualification boundary

This is software and input-routing qualification. The candidate has not been installed over the running application or driven on the owner's desktop. Physical pen pressure, tablet hover/side buttons, and device hotplug are not verified. Tablet pad controls, tilt, and physical tool rotation are not implemented brush controls. The existing 0.5.0 human acceptance evidence and build remain preserved.

## Contributor review

[Contributor findings and the tested #202 correction](contributor-review.md). The three code PRs passed 1,014 tests together; a new fresh-process regression found an ID-migration ordering error in #202. The proposed correction passes that regression and the extreme-ID rejection test. No contributor PR was merged.
