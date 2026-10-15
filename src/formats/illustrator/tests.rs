use super::*;
use crate::color::Rgba;
use crate::document::Fill;
use crate::geom::Geom;
use lopdf::{Object, Stream, dictionary};
use std::io::Write;

fn program(art: &str) -> Vec<u8> {
    format!("%!PS-Adobe-3.0\n%%Creator: Adobe Illustrator\n%AI3_Cropmarks: 0 0 200 100\n%AI5_BeginLayer\n1 1 1 1 0 0 1 0 79 128 255 0 50 0 Lb\n(Main) Ln\n{art}\nLB\n%%PageTrailer\n").into_bytes()
}

fn rectangle() -> &'static str {
    "10 20 m 50 20 l 50 60 l 10 60 l f"
}

fn pdf(data: &[u8], preview: bool, prefix: &str) -> Vec<u8> {
    let mut pdf = lopdf::Document::with_version("1.7");
    let pages = pdf.new_object_id();
    let cut = data.len() / 2;
    let first = pdf.add_object(Stream::new(dictionary! {}, data[..cut].to_vec()));
    let second = pdf.add_object(Stream::new(dictionary! {}, data[cut..].to_vec()));
    let mut private = dictionary! {"NumBlock"=>2};
    private.set(format!("{prefix}2"), second);
    private.set(format!("{prefix}1"), first);
    let mut page = dictionary! {"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),200.into(),100.into()],
    "PieceInfo"=>dictionary! {"Illustrator"=>dictionary! {"Private"=>private}}};
    if preview {
        let content = pdf.add_object(Stream::new(
            dictionary! {},
            b"0 0 1 rg 0 0 200 100 re f".to_vec(),
        ));
        page.set("Contents", content);
    }
    let page = pdf.add_object(page);
    pdf.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}),
    );
    let catalog = pdf.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    pdf.trailer.set("Root", catalog);
    let mut out = Vec::new();
    pdf.save_to(&mut out).unwrap();
    out
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn encode85(data: &[u8]) -> String {
    let mut out = String::new();
    for part in data.chunks(4) {
        let mut bytes = [0; 4];
        bytes[..part.len()].copy_from_slice(part);
        let mut n = u32::from_be_bytes(bytes);
        let mut group = [b'!'; 5];
        for i in (0..5).rev() {
            group[i] = b'!' + (n % 85) as u8;
            n /= 85;
        }
        out.push_str(std::str::from_utf8(&group[..part.len() + 1]).unwrap());
    }
    out.push_str("~>");
    out
}

#[test]
fn illustrator_private_streams_override_preview_and_work_without_it() {
    let native = program(&format!("u\n0 1 1 0 1 0 0 Xa\n{}\nU", rectangle()));
    for (marker, packed) in [
        (b"%AI12_CompressedData".as_slice(), zlib(&native)),
        (
            b"%AI24_ZStandard_Data".as_slice(),
            zstd::stream::encode_all(native.as_slice(), 1).unwrap(),
        ),
    ] {
        let data = [marker, packed.as_slice()].concat();
        for preview in [true, false] {
            for prefix in ["AIPrivateData", "AIPDFPrivateData"] {
                let (doc, notes) = read(&pdf(&data, preview, prefix), "Private").unwrap();
                assert_eq!((doc.width, doc.height), (200., 100.));
                let main = doc.layers.iter().find(|l| l.name == "Main").unwrap();
                let group = doc
                    .layers
                    .iter()
                    .find(|l| l.is_group && l.parent == Some(main.id))
                    .unwrap();
                let layer = doc
                    .layers
                    .iter()
                    .find(|l| l.parent == Some(group.id))
                    .unwrap();
                let shape = &layer.kind.shapes().unwrap()[0];
                assert_eq!(shape.style.fill, Fill::Solid(Rgba::rgb(255, 0, 0)));
                assert!(shape.style.stroke.is_none());
                assert_eq!(shape.geom.bbox().min, crate::geom::Pt::new(10., 40.));
                assert!(notes[0].contains("editing data"));
                let reopened =
                    crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
                assert_eq!(reopened.layers.len(), doc.layers.len());
                assert_eq!(reopened.artboards, doc.artboards);
                assert_eq!(
                    reopened
                        .layers
                        .iter()
                        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
                        .next()
                        .unwrap()
                        .geom,
                    shape.geom
                );
            }
        }
    }
}

#[test]
fn illustrator_eps_ascii85_and_legacy_editable_type() {
    let native = program(
        "0 0 1 0 0 1 0 Xa\n1 Xw\n1 A\n0 To\n1 0 0 1 25 70 Tp\n(Helvetica) 14 Tf\n(Hello \\050AI\\051) Tx\nTO",
    );
    for (codec, packed) in [
        ("AI9", zlib(&native)),
        (
            "AI24",
            zstd::stream::encode_all(native.as_slice(), 1).unwrap(),
        ),
    ] {
        let eps = format!(
            "%!PS-Adobe-3.0 EPSF-3.0\n%%EOF\n%AI9_PrivateDataBegin\n%{codec}_DataStream\n%{}\n%AI9_PrivateDataEnd\n",
            encode85(&packed)
        );
        let (doc, _) = read(eps.as_bytes(), "Type").unwrap();
        let shape = doc
            .layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .next()
            .unwrap();
        let Geom::Text(text) = &shape.geom else {
            panic!("type remains editable")
        };
        assert_eq!(text.content, "Hello (AI)");
        assert_eq!(text.px, 14.);
        assert_eq!(text.origin, crate::geom::Pt::new(25., 30.));
        assert_eq!(shape.style.fill, Fill::Solid(Rgba::rgb(0, 255, 0)));
        assert!(!shape.visible);
        assert!(shape.locked);
        assert!(text.contours.iter().any(|c| !c.is_empty()));
    }
    assert!(read(&native, "Legacy").is_ok());
}

#[test]
fn illustrator_artboard_dictionary_names_and_layout_survive_reopen() {
    let boards=b"%_/Document :\n%_/Dictionary :\n%_/Array :\n%_/Dictionary :\n%_(Front) /String (Name) ,\n%_0 100 /RealPoint (PositionPoint1) ,\n%_100 0 /RealPoint (PositionPoint2) ,\n%_; ,\n%_/Dictionary :\n%_(Back) /String (Name) ,\n%_150 100 /RealPoint (PositionPoint1) ,\n%_350 -50 /RealPoint (PositionPoint2) ,\n%_; ,\n%_; (ArtboardArray) ,\n%_; /NotRecorded ,\n%_;\n";
    let mut native = program(rectangle());
    let end = native
        .windows(b"%%PageTrailer".len())
        .position(|w| w == b"%%PageTrailer")
        .unwrap();
    native.splice(end..end, boards.iter().copied());
    let (doc, _) = read(&native, "Boards").unwrap();
    assert_eq!(doc.artboards.len(), 2);
    assert_eq!(doc.artboards[0].name, "Front");
    assert_eq!(doc.artboards[1].name, "Back");
    assert_eq!(doc.artboards[1].origin, crate::geom::Pt::new(150., 0.));
    assert_eq!(doc.artboards[1].size, crate::geom::Pt::new(200., 150.));
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(reopened.artboards, doc.artboards);
}

#[test]
fn illustrator_compounds_and_unsupported_features_are_explicit() {
    let native = program(&format!(
        "*u\n{}\n60 20 m 80 20 l 80 40 l f\n*U\n(missing) 0 0 1 1 0 0 0 0 0 [1 0 0 1 0 0] p\n{}\n/Mesh X!\n/End X!",
        rectangle(),
        rectangle()
    ));
    let (doc, notes) = read(&native, "Compound").unwrap();
    let shape = doc
        .layers
        .iter()
        .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
        .next()
        .unwrap();
    let Geom::Paths { paths, .. } = &shape.geom else {
        panic!()
    };
    assert_eq!(paths.len(), 2);
    assert!(shape.style.stroke.is_none());
    assert!(notes.iter().any(|n| n.contains("Pattern paint")));
    assert!(notes.iter().any(|n| n.contains("gradient meshes")));
    assert_eq!(doc.import_notes, notes);
}

#[test]
fn illustrator_damaged_containers_fail_instead_of_silent_preview_success() {
    assert!(decode_private(b"%AI12_CompressedData broken").is_err());
    assert!(decode_private(b"%AI24_ZStandard_Data broken").is_err());
    assert!(ascii85(b"!~>").is_err());
    assert!(ascii85(b"uuuuu~>").is_err());
    assert!(read(b"%PDF-1.7 garbage", "Bad").is_err());
    assert!(read(&program("u 0 0 m 10 10 l S"), "Unbalanced").is_err());
}

#[test]
fn illustrator_embedded_picture_keeps_exact_rotation_and_labels_shear() {
    let samples = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0];
    for (matrix, expected_origin, expected_size, sheared) in [
        (
            "0 1 -1 0 40 60",
            crate::geom::Pt::new(38., 40.),
            crate::geom::Pt::new(2., 2.),
            false,
        ),
        (
            "1 0 0.5 1 10 80",
            crate::geom::Pt::new(10., 20.),
            crate::geom::Pt::new(3., 2.),
            true,
        ),
    ] {
        let mut art=format!("%AI5_BeginRaster\n/DeviceRGB XN\n[{matrix}] 0 0 2 2 2 2 8 3 0 0 1\n%%BeginData: {}\nXI\n",samples.len()+4).into_bytes();
        art.extend_from_slice(&samples);
        art.extend_from_slice(b"\n%%EndData\n%AI5_EndRaster\n");
        let mut input = program("");
        let at = input.windows(3).position(|w| w == b"LB\n").unwrap();
        input.splice(at..at, art);
        let (doc, notes) = read(&input, "Picture placement").unwrap();
        let layer = doc
            .layers
            .iter()
            .find(|l| l.kind.pixels().is_some())
            .unwrap();
        let (origin, size, rotation) = layer.kind.raster_xform().unwrap();
        assert_eq!(origin, expected_origin);
        assert_eq!(size, expected_size);
        assert_eq!(
            notes
                .iter()
                .any(|n| n.contains("Sheared Illustrator pictures")),
            sheared
        );
        let pixels = layer.kind.pixels().unwrap();
        assert!(pixels.data.chunks_exact(4).any(|p| p[3] > 0));
        if !sheared {
            assert!((rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
            let transform = crate::compositor::layer_pixel_transform(layer);
            assert!((transform.tx - 40.).abs() < 1e-5);
            assert!((transform.ty - 40.).abs() < 1e-5);
            assert!((transform.ky - 1.).abs() < 1e-5);
            assert!((transform.kx + 1.).abs() < 1e-5);
            assert_eq!(&pixels.data[..4], &[255, 0, 0, 255]);
        }
    }
}

#[test]
fn illustrator_linked_picture_retains_original_once_and_reopens() {
    let directory = std::env::current_dir()
        .unwrap()
        .join(".artifacts")
        .join(format!(
            "illustrator-link-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
    std::fs::create_dir_all(&directory).unwrap();
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _scratch = Scratch(directory.clone());
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(2, 1, image::Rgba([17, 93, 201, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    std::fs::write(directory.join("pixel.png"), &bytes).unwrap();
    let place = "%AI5_BeginPlace\n[1 0 0 1 0 0] 0 0 2 1 (pixel.png) `\n~\n%AI5_EndPlace\n";
    let (doc, notes) = read_with_path(
        &program(&place.repeat(2)),
        "Linked",
        Some(&directory.join("source.ai")),
    )
    .unwrap();
    assert!(!notes.iter().any(|n| n.contains("saved preview")));
    assert_eq!(doc.sources.len(), 2);
    assert_eq!(doc.sources[0].bytes(), bytes);
    assert_eq!(doc.sources[0].name, "pixel.png");
    assert_eq!(
        doc.layers
            .iter()
            .filter(|l| l.source_attachment.as_deref() == Some(doc.sources[0].id.as_str()))
            .count(),
        2
    );
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert_eq!(reopened.sources, doc.sources);
}

#[test]
#[ignore = "Requires the independent real AI fixture directory through OMADESIGN_ILLUSTRATOR_FIXTURE"]
fn illustrator_independent_real_fixture_without_pdf_preview_and_oma_reopen() {
    independent_fixture("Test-Inkscape-LineIssue.ai", 1, false);
}

#[test]
#[ignore = "Requires the independent real AI fixture directory through OMADESIGN_ILLUSTRATOR_FIXTURE"]
fn illustrator_independent_font_template_without_pdf_preview_and_oma_reopen() {
    independent_fixture("font_template.ai", 208, true);
}

#[test]
#[ignore = "Requires the independent real AI fixture directory through OMADESIGN_ILLUSTRATOR_FIXTURE"]
fn illustrator_independent_reuters_without_pdf_preview_and_oma_reopen() {
    independent_fixture("reuters-dynamicText.ai", 1, true);
}

fn independent_fixture(file: &str, expected_boards: usize, needs_type: bool) {
    let location = std::env::var_os("OMADESIGN_ILLUSTRATOR_FIXTURE").expect(
        "set OMADESIGN_ILLUSTRATOR_FIXTURE to the independent fixture directory or one AI in it",
    );
    let location = Path::new(&location);
    let directory = if location.is_dir() {
        location
    } else {
        location.parent().unwrap()
    };
    let path = directory.join(file);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{file}: {e}"));
    let data = editing_data(&bytes).unwrap();
    let expected_stories = String::from_utf8_lossy(&data)
        .matches("/StoryIndex")
        .count();
    let (doc, notes) = read_with_path(&bytes, "Independent", Some(&path))
        .unwrap_or_else(|e| panic!("{file}: {e}"));
    assert_eq!(
        doc.artboards.len(),
        expected_boards,
        "{file}: real artboard count"
    );
    let mut without = lopdf::Document::load_mem(&bytes).unwrap();
    for id in without.get_pages().values() {
        without.get_dictionary_mut(*id).unwrap().remove(b"Contents");
    }
    let mut stripped = Vec::new();
    without.save_to(&mut stripped).unwrap();
    assert_eq!(
        editing_data(&stripped).unwrap(),
        data,
        "{file}: stripping previews must not alter editing data"
    );
    let (alone, alone_notes) =
        read(&stripped, "Independent").unwrap_or_else(|e| panic!("{file} without preview: {e}"));
    assert_eq!(
        doc.artboards
            .iter()
            .map(|a| (&a.name, a.origin, a.size))
            .collect::<Vec<_>>(),
        alone
            .artboards
            .iter()
            .map(|a| (&a.name, a.origin, a.size))
            .collect::<Vec<_>>()
    );
    fn shapes(doc: &Document) -> Vec<(serde_json::Value, crate::document::Style)> {
        doc.layers
            .iter()
            .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
            .map(|s| (serde_json::to_value(&s.geom).unwrap(), s.style.clone()))
            .collect()
    }
    assert_eq!(
        shapes(&doc),
        shapes(&alone),
        "{file}: preview-independent native artwork"
    );
    assert_eq!(notes, alone_notes);
    assert!(!shapes(&doc).is_empty(), "{file}: actual native artwork");
    let text: Vec<_> = doc
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
        .collect();
    if needs_type {
        assert!(expected_stories > 0, "{file}: real source has live stories");
        assert!(
            !text.is_empty(),
            "{file}: real editable type must be reconstructed"
        );
        assert!(
            text.iter().any(|t| !t.content.trim().is_empty()),
            "{file}: text must retain characters"
        );
        let reference = path.with_extension("preview-text.html");
        let output = std::process::Command::new("pdftotext")
            .arg("-bbox-layout")
            .arg(&path)
            .arg(&reference)
            .output()
            .expect("independent text check requires Poppler pdftotext");
        assert!(
            output.status.success(),
            "{file}: independent Poppler text extraction failed"
        );
        let html = std::fs::read_to_string(&reference).unwrap();
        let words = html
            .lines()
            .filter(|l| l.trim_start().starts_with("<word "))
            .map(|line| {
                roxmltree::Document::parse(line.trim())
                    .unwrap()
                    .root_element()
                    .text()
                    .unwrap_or("")
                    .to_string()
            })
            .filter(|s| !s.trim().is_empty())
            .collect::<Vec<_>>();
        let content = text
            .iter()
            .map(|t| t.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let matched = words
            .iter()
            .filter(|w| content.contains(w.as_str()))
            .count();
        let bounds=doc.layers.iter().filter_map(|l|l.kind.shapes()).flatten().filter_map(|s|if let Geom::Text(t)=&s.geom{Some(serde_json::json!({"text":t.content,"origin":t.origin,"bounds":s.world_bbox()}))}else{None}).take(8).collect::<Vec<_>>();
        std::fs::write(path.with_extension("native-type-qualification.json"),serde_json::to_string_pretty(&serde_json::json!({"file":file,"source_story_slots":expected_stories,"native_text_objects":text.len(),"independent_pdf_words":words.len(),"words_retained_as_editable_characters":matched,"native_examples":bounds,"warnings":notes})).unwrap()).unwrap();
        assert!(
            !words.is_empty(),
            "{file}: independent PDF text oracle is empty"
        );
        assert!(
            matched * 10 >= words.len() * 9,
            "{file}: only {matched}/{} independent PDF words survived as editable type",
            words.len()
        );
    }
    let reopened = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
    assert!(
        reopened
            .sources
            .iter()
            .any(|s| s.format == "illustrator" && s.bytes() == bytes),
        "{file}: original AI bytes must survive native import and reopen"
    );
    assert_eq!(
        shapes(&doc),
        shapes(&reopened),
        "{file}: native save/reopen"
    );
    assert_eq!(doc.artboards, reopened.artboards);
    assert_eq!(doc.import_notes, reopened.import_notes);
    if !needs_type {
        let svg = std::fs::read_to_string(directory.join("Test-Inkscape-LineIssue.svg")).unwrap();
        let reference = usvg::Tree::from_str(&svg, &usvg::Options::default()).unwrap();
        let marks = |doc: &Document| {
            let mut marks: Vec<_> = doc
                .layers
                .iter()
                .flat_map(|l| l.kind.shapes().unwrap_or(&[]))
                .filter(|s| !s.guide && s.style.stroke.is_some())
                .map(|s| {
                    let b = s.geom.bbox();
                    let stroke = s.style.stroke.as_ref().unwrap();
                    [b.min.x, b.min.y, b.max.x, b.max.y, stroke.width]
                })
                .collect();
            marks.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
            marks
        };
        let actual = marks(&doc);
        fn reference_marks(group: &usvg::Group, marks: &mut Vec<[f32; 5]>) {
            for node in group.children() {
                match node {
                    usvg::Node::Group(group) => reference_marks(group, marks),
                    usvg::Node::Path(path) => {
                        if let Some(stroke) = path.stroke() {
                            let bounds = path.abs_bounding_box();
                            let (sx, sy) = path.abs_transform().get_scale();
                            assert!((sx - sy).abs() < 1e-6, "author's reference stroke scale");
                            marks.push([
                                bounds.left(),
                                bounds.top(),
                                bounds.right(),
                                bounds.bottom(),
                                stroke.width().get() * sx,
                            ]);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut expected = Vec::new();
        reference_marks(reference.root(), &mut expected);
        expected.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
        assert_eq!(
            expected.len(),
            28,
            "independent author's reference has28 strokes"
        );
        assert_eq!(
            actual.len(),
            expected.len(),
            "author's SVG stroke object count"
        );
        for (a, b) in actual.iter().zip(&expected) {
            for (a, b) in a.iter().zip(b) {
                assert!(
                    (a - b).abs() < 0.025,
                    "AI vs author's SVG paint/placement: {a} vs {b}"
                );
            }
        }
    }
    eprintln!(
        "Independent AI {file}: {} bytes, {} artboards, {} layers, {} native objects, {} editable text from {expected_stories} real story slots; {} notes",
        bytes.len(),
        doc.artboards.len(),
        doc.layers.len(),
        shapes(&doc).len(),
        text.len(),
        notes.len()
    );
}
