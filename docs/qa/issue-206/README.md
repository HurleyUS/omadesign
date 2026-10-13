# Drawing fixes — issue #206

Release: [0.6.3-nightly.20261013.0001](../release-0.6.3-nightly.20261013.0001/README.md). This document records the implementation checkpoint before release packaging.

Base: master `f969a0b2`. Work is isolated from the owner's feature checkout and running application.

## Changed behavior

- A selection whose objects share a rotation uses that rotated frame for its outline, handles, pointer picking, and scaling. Rectangles stay rectangles. Path conversion retains the same frame. Resize undo/redo restores the original geometry.
- Essentials includes a Square tip. The native pack generator and website download contain the same seven tips. Sampled cursors trace the tip's visible edge and turn with the selected angle and stroke direction.
- Sidebar width changes only through an explicit edge drag. Existing saved panel widths are retained; canvas hover and vertical motion cannot resize it.
- Brush spacing ranges from 1% to 10,000% and accumulates across pointer events for round and sampled brushes and erasers.
- Angle, Follow stroke direction, Scatter, angle jitter, size jitter, and opacity jitter are usable brush controls. Each stroke has a fresh random seed. Custom `.omabrush` packs retain the dynamics and accept spacing above 400%.
- Wayland binds the native tablet-v2 protocol and forwards hover, contact, release, cancellation, pressure, and stylus side buttons. X11 retains the Abs Pressure valuator and sends position, pressure, and contact together. Unrelated touches cannot release a mouse stroke. Leaving pen proximity releases held side buttons before pointer cancellation. Size and Flow pressure controls are independent.

## Software qualification

- Library suite: **1,026 passed; 0 failed; 9 ignored**. Tests run from a preserved executable, with isolated HOME/XDG directories and a short project-owned temporary directory for Unix sockets.
- New regressions cover multiple rotated rectangles and converted paths, scale/undo/redo, outline and rotated-tip pixels, pen-pressure dab size and release, high spacing through tiny pointer samples, portable dynamics, bounded scatter updates, independent stroke randomness, cancellation, and intentional sidebar resizing.
- `cargo check --all-targets` passes on this Linux ARM64 host.
- Native debug binary build and `--version` execution pass. Reviewed candidate: `.worktrees/canvas-brush-fixes/artifacts/omadesign-reviewed`. It carries the base nightly version, `0.6.3-nightly.20261011.0001`; this task does not publish a new release.
- Candidate SHA-256: `d5ac1b7e6a30acb5ef7fa17fa9c0b18434623a2dfa343b9c7f6dce412630e92c`.
- `git diff --check` passes. Build outputs, logs, local worktrees, and environment files are ignored.

## Qualification boundary

This is software and input-routing qualification. The candidate has not been installed over the running application or driven on the owner's desktop. Physical pen pressure, tablet hover/side buttons, and device hotplug are not verified. Tablet pad controls, tilt, physical tool rotation, and automatic reverse-side eraser switching are not implemented brush controls. The existing 0.5.0 human acceptance evidence and build remain preserved.

## Contributor review

[Contributor findings and the tested #202 correction](contributor-review.md). The three code PRs passed 1,014 tests together; a new fresh-process regression found an ID-migration ordering error in #202. The proposed correction passes that regression and the extreme-ID rejection test. Fred’s #202–#204 commit histories are merged into #207, including the #202 correction. Standalone PR closure and branch retirement are recorded in the consolidation receipt.

## Native media and site qualification

- Nine fresh native replays plus a direct brush-controls replay: **4,620 native WGPU frames**, with **zero unresolved input targets**. The spacing field accepts 10,000%; direction, scatter and all jitter values are checked after real UI edits.
- Light-pressure strokes remain continuous, including subpixel tip sizes. Pending spacing scales with pressure; cursors reuse their selected tip boundary.
- The template screenshot route no longer waits for an unrelated hidden welcome catalog. Its regression passes separately from the full suite. X11 pressure, touch ownership, and Wayland side-button regressions pass in the vendor crates.
- Site production build, TypeScript check and **23 tests** pass. Native clip MP4 files and both film formats decode fully. The film is 40 seconds, 1,200 frames, 1600 × 900, 30 fps, silent, with 20 context/detail cuts.
- Active README/site/root media now uses the canonical logo, current native captures and editable Fieldwork artwork. The manifest records source/output hashes. Earlier marks, mocks and films are retired; historical QA files remain unchanged.

Native captures use an isolated display and real egui input injection. They do not establish physical tablet acceptance. The Agent recording composes an unsent brief and stages no provider response.
