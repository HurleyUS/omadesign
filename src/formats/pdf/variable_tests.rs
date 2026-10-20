use super::*;
use crate::geom::TypeRun;
use crate::text::variations::FontVariations;
const FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/assets/fonts/EBGaramond.ttf"
);
fn sample() -> Document {
    let mut doc = Document::new("Embedded font instances", 520., 220., 72.);
    doc.layers = vec![Layer::vector("Variable text")];
    for (index, weight) in [400., 800., 400.].into_iter().enumerate() {
        let mut text = TypeRun {
            font: FONT.into(),
            font_variations: FontVariations::new(vec![(*b"wght", weight)]).unwrap(),
            content: if index == 2 {
                "AVAV"
            } else {
                "Variable AV fi12"
            }
            .into(),
            px: 32.,
            origin: Pt::new(24., 50. + index as f32 * 64.),
            ..Default::default()
        };
        if index == 2 {
            text.set_character_style(0, 2, |s| {
                s.font_variations = Some(FontVariations::new(vec![(*b"wght", 800.)]).unwrap())
            });
        }
        let mut shape = Shape::new(
            Geom::Text(text),
            Style {
                fill: Fill::Solid(Rgba::BLACK),
                stroke: None,
            },
        );
        crate::text::fill_contours(&mut shape.geom);
        doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    }
    doc
}
fn fonts(pdf: &Pdf) -> Vec<Vec<u8>> {
    pdf.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .filter_map(|d| d.get(b"FontFile2").ok())
        .map(|id| {
            let stream = resolve(pdf, id).unwrap().as_stream().unwrap();
            decoded(stream, 32 * 1024 * 1024).unwrap()
        })
        .collect()
}
#[test]
fn craft_pdf_variable_instances_are_real_separate_fonts_with_unicode_and_unchanged_source() {
    let source = std::fs::read(FONT).unwrap();
    let doc = sample();
    let before = crate::project::encode(&doc).unwrap();
    let (bytes, notes) = write(&doc).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let pdf = Pdf::load_mem(&bytes).unwrap();
    let programs = fonts(&pdf);
    assert_eq!(programs.len(), 2);
    let mut weights = vec![];
    for program in &programs {
        let face = rustybuzz::ttf_parser::Face::parse(program, 0).unwrap();
        assert!(!face.is_variable());
        assert!(face.tables().glyf.is_some());
        weights.push(face.weight().to_number());
        assert!(face.glyph_index('V').is_some());
    }
    weights.sort();
    assert_eq!(weights, [400, 800]);
    let pages = pdf.get_pages().keys().copied().collect::<Vec<_>>();
    let logical = pdf.extract_text(&pages).unwrap();
    assert!(
        logical.replace('\n', "").contains("Variable AV fi12"),
        "{logical:?}"
    );
    assert_eq!(crate::project::encode(&doc).unwrap(), before);
    assert_eq!(std::fs::read(FONT).unwrap(), source);
    let expected = crate::compositor::render_export(&doc, 1).unwrap();
    let (imported, notes) = read(&bytes, "instances.pdf").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let actual = crate::compositor::render_export(&imported, 1).unwrap();
    let error = expected
        .data()
        .iter()
        .zip(actual.data())
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum::<u64>() as f64
        / expected.data().len() as f64;
    assert!(error < 2., "Native PDF instance import error {error}");
    let encoded = crate::project::encode(&imported).unwrap();
    let reopened = crate::project::decode(&encoded).unwrap();
    assert_eq!(
        crate::compositor::export_png(&reopened, 1).unwrap(),
        crate::compositor::export_png(&imported, 1).unwrap()
    );
    if let Some(root) = std::env::var_os("OMADESIGN_FONT_EXPORT_QA") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("instances.pdf"), &bytes).unwrap();
        std::fs::write(
            root.join("instances-native.png"),
            crate::compositor::export_png(&doc, 1).unwrap(),
        )
        .unwrap();
        std::fs::write(
            root.join("instances-imported.png"),
            crate::compositor::export_png(&imported, 1).unwrap(),
        )
        .unwrap();
        std::fs::write(root.join("source.oma"), before).unwrap();
        std::fs::write(root.join("native-import.json"),serde_json::json!({"mean_channel_error":error,"font_weights":weights,"logical_text":logical}).to_string()).unwrap();
    }
}
#[test]
#[ignore = "External PDF readers and a cold process validate the actual exported fonts"]
fn craft_pdf_variable_instances_poppler_unicode_and_cold_native_reopen() {
    let root = std::env::var_os("OMADESIGN_FONT_EXPORT_QA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(".artifacts/font-export-tests")
                .join(format!("pdf-{}", std::process::id()))
        });
    std::fs::create_dir_all(&root).unwrap();
    let doc = sample();
    let (bytes, notes) = write(&doc).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let path = root.join("instances.pdf");
    std::fs::write(&path, &bytes).unwrap();
    let qpdf = std::process::Command::new("qpdf")
        .arg("--check")
        .arg(&path)
        .output()
        .expect("qpdf");
    std::fs::write(root.join("qpdf.txt"), &qpdf.stdout).unwrap();
    assert!(
        qpdf.status.success(),
        "{}",
        String::from_utf8_lossy(&qpdf.stderr)
    );
    let font = std::process::Command::new("pdffonts")
        .arg(&path)
        .output()
        .expect("pdffonts");
    assert!(font.status.success());
    std::fs::write(root.join("pdffonts.txt"), &font.stdout).unwrap();
    let report = String::from_utf8_lossy(&font.stdout);
    assert!(
        report.contains("ExtraBold") && report.contains("Regular"),
        "{report}"
    );
    let text = std::process::Command::new("pdftotext")
        .arg("-layout")
        .arg(&path)
        .arg("-")
        .output()
        .expect("pdftotext");
    assert!(text.status.success());
    std::fs::write(root.join("pdftotext.txt"), &text.stdout).unwrap();
    let logical = String::from_utf8_lossy(&text.stdout);
    assert_eq!(
        logical.matches("Variable AV fi12").count(),
        2,
        "{logical:?}"
    );
    assert!(logical.contains("AVAV"), "{logical:?}");
    let raster = std::process::Command::new("pdftoppm")
        .args(["-r", "72", "-singlefile", "-png"])
        .arg(&path)
        .arg(root.join("poppler"))
        .output()
        .expect("pdftoppm");
    assert!(
        raster.status.success(),
        "{}",
        String::from_utf8_lossy(&raster.stderr)
    );
    let actual = image::open(root.join("poppler.png")).unwrap().to_rgba8();
    assert_eq!(actual.dimensions(), (520, 220));
    let expected = crate::compositor::render_export(&doc, 1).unwrap();
    let error = expected
        .pixels()
        .iter()
        .zip(actual.pixels())
        .map(|(a, b)| {
            let a = a.demultiply();
            u64::from(a.red().abs_diff(b.0[0]))
                + u64::from(a.green().abs_diff(b.0[1]))
                + u64::from(a.blue().abs_diff(b.0[2]))
        })
        .sum::<u64>() as f64
        / (520. * 220. * 3.);
    std::fs::write(root.join("external.json"),serde_json::json!({"mean_channel_error":error,"limit":2.,"width":520,"height":220,"qpdf_success":true,"font_count":fonts(&Pdf::load_mem(&bytes).unwrap()).len()}).to_string()).unwrap();
    assert!(error < 2., "Poppler instance error {error}");
    let (imported, notes) = read(&bytes, "instances.pdf").unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    crate::project::save_to(&imported, &root.join("portable.oma")).unwrap();
    std::fs::write(
        root.join("portable.png"),
        crate::compositor::export_png(&imported, 1).unwrap(),
    )
    .unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "formats::pdf::variable_tests::craft_pdf_variable_cold_child",
            "--ignored",
            "--nocapture",
        ])
        .env("OMA_VARIABLE_PDF_REOPEN", &root)
        .output()
        .unwrap();
    std::fs::write(
        root.join("cold.txt"),
        [child.stdout.as_slice(), child.stderr.as_slice()].concat(),
    )
    .unwrap();
    assert!(
        child.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
}
#[test]
#[ignore = "Only spawned by the real external-reader test"]
fn craft_pdf_variable_cold_child() {
    let root = std::path::PathBuf::from(
        std::env::var_os("OMA_VARIABLE_PDF_REOPEN").expect("Exact parent fixture"),
    );
    let doc = crate::project::load_from(&root.join("portable.oma")).unwrap();
    assert_eq!(
        crate::compositor::export_png(&doc, 1).unwrap(),
        std::fs::read(root.join("portable.png")).unwrap()
    );
    let fonts = doc
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .filter_map(|s| {
            if let Geom::Text(t) = &s.geom {
                Some(t)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(!fonts.is_empty());
    assert!(
        fonts
            .iter()
            .all(|t| crate::text::project_font_loaded(&t.font))
    );
    std::fs::write(
        root.join("portable-cold.oma"),
        crate::project::encode(&doc).unwrap(),
    )
    .unwrap();
}
