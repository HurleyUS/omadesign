# 0.6.3-nightly.20261013.0001 qualification

This selected nightly is published as the latest release. It includes the canvas, brush and pen changes in #207, Fred Nix's #202–#204 fixes, the dated brush-pack release, and the installer changes since October 2.

The local package receipt records each archive and executable hash, full extracted payload comparison, required glibc versions and native installation checks. Desktop compilation and tests run locally. Hosted personal-repository site CI is skipped and is not counted as validation.

- Implementation library suite: 1,026 passed, 0 failed, 9 ignored. The template-readiness regression and X11 pressure, egui touch ownership and Wayland side-button regressions passed separately.
- Native implementation replay: 4,620 WGPU frames; no unresolved input targets. See [issue #206](../issue-206/README.md).
- Site: 25 tests passed across seven files; production build and typecheck passed. Both the featured update and dated article use the new version. The brush-library link remains on the site and respects a GitHub Pages base.
- Installers: 35 checks passed; ShellCheck passed. Prefixed installation checks include executable, bundled runtime, docs, skill and plugin content.
- ARM64 release: exact public version; native WGPU About and brush-study captures; editable document inspection, native PNG export, and original tip/settings preservation and byte-exact canonical brush-pack roundtrips.
- Physical IntelPro x86_64 release: exact public version, prefixed installation, editable document inspection, native PNG export and original brush-pack roundtrip. This is native CPU execution; it does not claim new x86_64 GUI or physical tablet acceptance.

[Public download, latest-release, installer and production-site verification](https://github.com/michaelmonetized/omadesign/releases/download/v0.6.3-nightly.20261013.0001/verification.json) is attached to the release after publication. It records the production deployment and downloaded asset hashes.

Physical tablet acceptance and hotplug remain unverified. Tilt, pad buttons/rings/strips, physical tool rotation and automatic reverse-side eraser switching are not implemented. The owner's running application and accepted 0.5.0 evidence are preserved.

[Package hashes and native execution receipt](packages.json) · [ARM64 About](arm64-about.png) · [ARM64 Pixel](arm64-pixel.png).
