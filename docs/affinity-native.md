# Native Affinity import

Open `.af`, `.afdesign`, `.afphoto`, `.afpub`, or the document variants through File → Open. The Rust reader reads the native archive and tagged object stream without installing or launching a converter. Supported objects become editable paths, groups, type, paint, image layers, artboards and live masks through the shared artwork importer.

Original bytes are stored once in the OMA project with a SHA-256 and content-derived UUID. Objects reference that UUID. File → Save original source… recovers the exact original container, including fields that Omadesign does not understand. Saving OMA preserves editable artwork and original data; it is not an Affinity writer. Existing OMA files default to no retained sources. The provisional document schema is 14; release integration selects the final schema centrally.

If native parsing fails and the separately installed converter exists, import can use it with an explicit diagnostic. No converter is installed by opening a document. Unsupported layer kinds, brush/pressure strokes, color profiles, publishing structures and effects produce conversion notes. Embedded documents and symbols currently use cached images. Exact rich text and affine paint depend on the shared SVG type and paint changes in the Craft stack. Original data retention does not make an unsupported feature editable.

## Qualification

The pinned fixture manifest is `tests/fixtures/affinity-native.json`. Fetch it with:

```sh
node scripts/fetch-affinity-fixtures.mjs /home/michael/Projects/omadesign/.artifacts/affinity-fixtures
```

Only public MIT and CC0 files are listed; files stay ignored. The fixtures are actual Affinity V1/V2/V3 documents (container versions 9, 11 and 12), pinned and SHA-256 verified against the source reader's upstream manifest. The test `native_affinity_real_files_preserve_editable_artwork_and_original_bytes` requires `OMADESIGN_AFFINITY_FIXTURES` to point to that directory. Run Cargo through the program's serialized helper, on a committed revision. It checks editable feature retention, artwork hierarchy, exact original-source bytes, native OMA encode/reopen stability, and independent SVG parsing/reimport. It writes a concise JSON report beside the ignored fixtures.

These checks do not prove reopening SVG/PDF in Affinity, physical display output, installed desktop interaction, or compatibility with every document version/platform. The vendored reader's MIT notice, pinned source and upstream provenance are in `vendor/affinity/`.
