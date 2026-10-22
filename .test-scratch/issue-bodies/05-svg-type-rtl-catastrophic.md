**Test:** `text::svg_type::tests::svg_type_rotated_reflected_mixed_positioned_rtl_vertical_and_path_roundtrip`

**Panic:** `مرحبا بالعالم: 0.75889283` at `src/text/svg_type/tests.rs:66` -- the compositor render does not match the reference SVG raster; the per-run pixel error came back 0.759 against a 0.1 threshold.

Reproduce: `cargo test --lib -- svg_type_rotated_reflected_mixed_positioned_rtl_vertical_and_path_roundtrip`

The test accepts rotated, reflected, mixed positioned, RTL, vertical and path type in one document and checks each run's render against an SVG reference via `crate::compositor::render_export`. The Arabic RTL positioned run (`مرحبا بالعالم`) fails first: the text is accepted (the SVG reader does not refuse it) but the painted output lands ~0.76 mean-error away from the reference, which means the run's geography (position/order/shaping) is painted wrong, not just antialiased differently.

Run on master `4cb4afd1` (Release 0.6.8: Craft parity), `cargo test --lib -- --test-threads=1`.
