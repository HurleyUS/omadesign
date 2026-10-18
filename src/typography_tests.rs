use super::*;
use crate::{
    app::Studio,
    color::Rgba,
    document::{Document, Layer, Shape, Style},
    geom::{Geom, Pt, TypeRun},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "omadesign-type-test-{}-{}",
            std::process::id(),
            crate::document::next_id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fonts() -> Vec<PathBuf> {
    crate::text::fonts()
        .iter()
        .filter(|face| face.path.extension().is_some_and(|ext| ext == "ttf"))
        .take(2)
        .map(|face| face.path.clone())
        .collect()
}

#[test]
fn portable_validation_rollback_and_refresh_preserve_original_font_bytes() {
    let temp = Temp::new();
    let fonts = fonts();
    assert_eq!(fonts.len(), 2, "two installed test fonts are required");
    let root = temp.0.join("kit");
    // Match size and mtime across an atomic replacement: file identity must
    // invalidate the cache, as it does when a sync tool preserves timestamps.
    let mut original = fs::read(&fonts[0]).unwrap();
    let mut replacement = fs::read(&fonts[1]).unwrap();
    let padded_size = original.len().max(replacement.len());
    original.resize(padded_size, 0);
    replacement.resize(padded_size, 0);
    let source_font = temp.0.join("source.ttf");
    fs::write(&source_font, original).unwrap();
    let first = add_fonts(&root, &[source_font], None).unwrap();
    register(&first).unwrap();
    assert_eq!(decode(&encode(&first.kit).unwrap()).unwrap(), first.kit);
    assert!(first.kit.roles[0].font.starts_with("fonts/"));
    for path in [
        "/tmp/font.ttf",
        "fonts/../outside.ttf",
        "fonts/linked/../../font.ttf",
        "fonts\\font.ttf",
    ] {
        let mut kit = first.kit.clone();
        kit.roles[0].font = path.into();
        assert!(encode(&kit).is_err(), "{path}");
    }
    let mut duplicate = first.kit.clone();
    duplicate.roles.push(duplicate.roles[0].clone());
    assert!(encode(&duplicate).is_err());
    let before = fs::read(root.join(FILE)).unwrap();
    let bad = temp.0.join("broken.ttf");
    fs::write(&bad, "not a font").unwrap();
    assert!(add_fonts(&root, &[fonts[1].clone(), bad], Some(first.stamp)).is_err());
    assert_eq!(fs::read(root.join(FILE)).unwrap(), before);
    assert_eq!(
        fs::read_dir(root.join(".omabrand/fonts")).unwrap().count(),
        1
    );

    let run = TypeRun {
        content: "Refresh Ω".into(),
        font: first.fonts[0].id.clone(),
        px: 30.0,
        ..Default::default()
    };
    let original_outline = crate::text::shape(&run);
    let file = root.join(".omabrand").join(&first.kit.roles[0].font);
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    let replacement_path = file.with_extension("replacement");
    fs::write(&replacement_path, replacement).unwrap();
    fs::File::open(&replacement_path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    fs::rename(&replacement_path, &file).unwrap();
    let refreshed = load(&root).unwrap().unwrap();
    assert_ne!(first.fonts[0].id, refreshed.fonts[0].id);
    register(&refreshed).unwrap();
    assert_eq!(
        crate::text::shape(&run),
        original_outline,
        "an applied face stays immutable until explicitly reapplied"
    );
    let mut newer = run.clone();
    newer.font = refreshed.fonts[0].id.clone();
    assert_ne!(crate::text::shape(&newer), original_outline);
    let mut changed = first.kit.clone();
    changed.name = "Other writer".into();
    save(&root, &changed, Some(first.stamp)).unwrap();
    assert!(save(&root, &first.kit, Some(first.stamp)).is_err());
    assert_eq!(load(&root).unwrap().unwrap().kit.name, "Other writer");
}

#[test]
fn project_fonts_copy_reopen_and_edit_without_an_installed_font_path() {
    // A fresh test process ensures a cached font from the source kit cannot hide
    // a portability bug when the original project folder has been deleted.
    if let Some(path) = std::env::var_os("OMADESIGN_TYPE_REOPEN_TEST") {
        let path = PathBuf::from(path);
        let doc = crate::project::load_from(&path).unwrap();
        let mut studio = Studio::new();
        studio.doc = doc;
        let id = studio.doc.layers[0].kind.shapes().unwrap()[0].id;
        studio.selection = vec![(0, id)];
        let run = match &studio.doc.find_shape(0, id).unwrap().geom {
            Geom::Text(run) => run,
            _ => panic!(),
        };
        assert!(run.font.starts_with("omatype:") && crate::text::project_font_loaded(&run.font));
        let old = run.contours.clone();
        studio.patch_type(|run| run.content.push_str(" Ω123"));
        let run = match &studio.doc.find_shape(0, id).unwrap().geom {
            Geom::Text(run) => run,
            _ => panic!(),
        };
        assert!(!run.contours.is_empty() && run.contours != old);
        assert_eq!(run.contours, crate::text::shape(run));
        return;
    }
    let temp = Temp::new();
    let source = temp.0.join("source");
    let moved = temp.0.join("moved");
    let kit = add_fonts(&source, &fonts()[..1], None).unwrap();
    let mut studio = Studio::new();
    studio.doc = Document::new("Type travel", 1.0, 1.0, 72.0);
    studio.doc.layers = vec![Layer::vector("Type")];
    let shape = Shape::new(
        Geom::Text(TypeRun {
            content: "Portable".into(),
            origin: Pt::new(5.0, 35.0),
            px: 22.0,
            ..Default::default()
        }),
        Style {
            fill: crate::document::Fill::Solid(Rgba::BLACK),
            ..Default::default()
        },
    );
    let id = shape.id;
    studio.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
    studio.selection = vec![(0, id)];
    studio.apply_project_font(&kit.fonts[0]).unwrap();
    studio.undo();
    assert!(
        matches!(&studio.doc.find_shape(0,id).unwrap().geom, Geom::Text(run) if run.font.is_empty())
    );
    studio.redo();
    let artwork = source.join("artwork.oma");
    crate::project::save_to(&studio.doc, &artwork).unwrap();
    let saved = fs::read_to_string(&artwork).unwrap();
    assert!(saved.contains(&kit.fonts[0].id));
    assert!(!saved.contains(&source.to_string_lossy().to_string()));
    // Removing the role keeps its archived font available to existing artwork.
    let mut without_roles = kit.kit.clone();
    without_roles.roles.clear();
    save(&source, &without_roles, Some(kit.stamp)).unwrap();
    export_copy(&source, &moved).unwrap();
    fs::copy(&artwork, moved.join("artwork.oma")).unwrap();
    fs::remove_dir_all(&source).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "typography::tests::project_fonts_copy_reopen_and_edit_without_an_installed_font_path",
            "--nocapture",
        ])
        .env("OMADESIGN_TYPE_REOPEN_TEST", moved.join("artwork.oma"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fresh process failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn svg_type_native_span_only_portable_faces_are_validated_and_archived(){
    let bytes=Arc::new(include_bytes!("../tests/assets/fonts/EBGaramond.ttf").to_vec());let id=format!("omatype:{:032x}",fingerprint(&bytes));crate::text::register_memory_font(&id,"Span face",bytes).unwrap();
    let mut doc=Document::new("Span fonts",100.,100.,96.);doc.layers.clear();let mut layer=Layer::vector("Type");let mut run=TypeRun{content:"A B".into(),..Default::default()};run.set_character_style(2,3,|s|s.font=Some(id.clone()));layer.kind.shapes_mut().unwrap().push(Shape::new(Geom::Text(run),Style::default()));doc.layers.push(layer);
    validate_document_fonts(&doc).unwrap();let archive=document_fonts(&doc).unwrap();assert_eq!(archive.len(),1);assert_eq!(archive[0].id,id);
    let Geom::Text(run)=&mut doc.layers[0].kind.shapes_mut().unwrap()[0].geom else{unreachable!()};run.set_character_style(2,3,|s|s.font=Some("omatype:missing-span-face".into()));assert!(validate_document_fonts(&doc).is_err());
}

#[test]
fn svg_unused_fallback_faces_and_missing_instance_baselines_use_the_same_native_font_archive() {
    use base64::Engine;
    let p=include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/assets/fonts/EBGaramond.ttf"));
    let f=fs::read("/usr/share/fonts/liberation/LiberationSans-Regular.ttf").unwrap();
    let source=format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="80"><style>@font-face{{font-family:ArchivePrimary;src:url(data:font/ttf;base64,{})}}@font-face{{font-family:ArchiveUnused;src:url(data:font/ttf;base64,{})}}</style><text x="10" y="50" font-size="32" font-family="ArchivePrimary,ArchiveUnused">AV</text></svg>"#,base64::engine::general_purpose::STANDARD.encode(p),base64::engine::general_purpose::STANDARD.encode(&f));
    let (mut doc,notes)=crate::formats::svg::read(&source,"Unused fallback").unwrap();assert!(notes.is_empty(),"{notes:?}");
    let expected:HashSet<_>=[p.as_slice(),f.as_slice()].into_iter().map(|b|format!("omatype:{:032x}",fingerprint(b))).collect();assert_eq!(document_font_ids(&doc).unwrap(),expected);
    let temp=Temp::new();let path=temp.0.join("unused.oma");crate::project::save_to(&doc,&path).unwrap();
    for id in &expected {let name=id.trim_start_matches("omatype:");assert!(fs::read_dir(temp.0.join(".omabrand/fonts")).unwrap().any(|f|f.unwrap().file_name().to_string_lossy().starts_with(name)));}
    let library=Temp::new();assert_eq!(copy_document_fonts(&path,&library.0).unwrap().len(),2);
    let mut empty=Document::new("Retained fallback",160.,80.,72.);empty.layers.clear();
    let mut replica=crate::collaboration::Replica::new("retained-fallback".into(),"b".repeat(64),crate::collaboration::snapshot(&empty).unwrap());replica.record(&doc).unwrap();assert!(replica.undo());empty.collaboration=Some(replica);
    let retained=temp.0.join("retained.oma");crate::project::save_to(&empty,&retained).unwrap();
    let copied=Temp::new();assert_eq!(copy_document_fonts(&retained,&copied.0).unwrap().len(),2);
    let li=doc.layers.iter().position(|l|l.kind.shapes().is_some_and(|s|s.iter().any(|s|matches!(s.geom,Geom::Text(_))))).unwrap();
    let baseline=doc.layers[li].kind.shapes().unwrap().iter().find(|s|matches!(s.geom,Geom::Text(_))).unwrap().clone();
    doc.layers[li].kind.shapes_mut().unwrap().clear();
    let mut instance=Shape::new(Geom::Rect{origin:Pt::ZERO,size:Pt::new(10.,10.),radius:0.},Style::default());
    instance.layout.component=Some(crate::layout_components::ComponentBinding::Instance{main:987650,nodes:vec![crate::layout_components::InstanceNode{source:987651,instance:987652,slot:987653,nested_main:Some(987654),baseline:Box::new(baseline)}]});
    doc.layers[li].kind.shapes_mut().unwrap().push(instance);assert_eq!(document_font_ids(&doc).unwrap(),expected);
    let separate=Temp::new();let output=separate.0.join("baseline");fs::create_dir_all(&output).unwrap();assert_eq!(archive_document_fonts(&doc,&output).unwrap().len(),2);
}
