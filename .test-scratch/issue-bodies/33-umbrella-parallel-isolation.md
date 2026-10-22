**Background:** Running `cargo test --lib` in parallel produced 40 failures in two full sweeps; when the same 42 focus filters run with `--test-threads=1`, 10 of them pass reliably -- exactly the tests that do exact before/after equality against the shared global resource pool (`crate::resources::global()`) or shared scratch. Their assertions are order- and contention-sensitive: whichever parallel test happens to be inside the pool when another one computes its before/after tuple corrupts the counters. The remaining 32 deterministic failures are tracked as individual issues.

**The 10 contention-sensitive tests** (full-suite parallel numbers from master `4cb4afd1`):

1. `channels::tests::craft_channels_invalid_scalar_payloads_names_and_transforms_preserve_source_and_pool` -- `(usage, pending)` tuple drift: left `(20492, 0)`, right `(20492, 4096)` at `src/channels/tests.rs:184`.
2. `channels::filter::family_tests::craft_channel_filter_families_all_thirty_keep_float_coverage_and_selection_owners` -- pool usage 75,586,252 vs expected 2,097,654 at `src/channels/filter_families_tests.rs:103`.
3. `channels::filter::family_tests::craft_channel_filter_families_mid_kernel_cancellation_releases_owned_working_buffers` -- pool usage 2,097,572 vs 2,097,164 at `src/channels/filter_families_tests.rs:343`.
4. `compositor::layer_merge::content_tests::content_crop_cancel_during_scan_and_allocation_releases_scratch_and_keeps_sources` -- release-scan count 12 vs 0 at `src/compositor/layer_content_bounds_tests.rs:173`.
5. `export::sizing_tests::craft_export_sizing_invalid_overflow_cancel_and_atomic_owner_release` -- failed in both parallel sweeps, passed both serial sweeps.
6. `formats::idml::tests::native_reopen_and_idml_roundtrip_preserve_editable_story_and_publishing` -- link list order flake: left `["Links/mark.png", "Links/unavailable.png"]`, right `["Links/unavailable.png", "Links/mark.png"]` at `src/formats/idml/tests.rs:93`.
7. `formats::raw::qualification::tests::raw_camera_contract_real_photo_workflow_cannot_replace_external_references` -- admitted_bytes 2,097,432 vs 0 at `src/formats/raw/qualification/tests.rs:383`.
8. `project::tests::admitted_project_output_refuses_size_drift_and_releases_its_owner` -- pool tuple `(2097514, 362)` vs `(2097152, 0)` at `src/project.rs:1210`.
9. `project::tests::native_parser_deducts_only_borrowed_root_raster_samples` -- 373 vs 362 at `src/project.rs:1132`.
10. `project::tests::native_parser_keeps_ambiguous_escaped_and_opaque_paths_fully_charged` -- failed 2 of 2 parallel sweeps, passed serial.

**Fix direction:** these tests assume exclusive ownership of the global pool for the whole assertion window. Options: serialize exactly them (they already run fine through `--test-threads=1` in the per-file qualifier), isolate the pool claim inside each assertion window, or assert deltas against a per-test scope instead of the global counters. Note the repo's release script already routes cargo through its serialized helper (`scripts/serialized-cargo.mjs` with a single global heavy lock), which unpins some of this at CI level but not for plain `cargo test` flows.
