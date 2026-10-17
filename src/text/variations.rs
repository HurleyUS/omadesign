//! Editable OpenType axis coordinates use the same shaping and outline engines as ordinary type.
use super::*;
use ab_glyph::VariableFont;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FontVariations(Vec<([u8; 4], f32)>);
impl FontVariations {
    /// Validate a finite, unique set of OpenType axis coordinates.
    /// Args: axis tags and user-space values. Returns a canonical tag-ordered setting set or an error.
    pub fn new(mut settings: Vec<([u8; 4], f32)>) -> Result<Self, String> {
        if settings.len() > 64
            || settings
                .iter()
                .any(|(tag, v)| !v.is_finite() || tag.iter().any(|b| !(0x20..=0x7e).contains(b)))
        {
            return Err("Font axes need at most 64 printable tags with finite coordinates".into());
        }
        settings.sort_by_key(|(tag, _)| *tag);
        if settings.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err("Font axis tags must be unique".into());
        }
        Ok(Self(settings))
    }
    /// Read validated authored coordinates.
    /// Args: none. Returns immutable OpenType tags and user-space values.
    pub fn settings(&self) -> &[([u8; 4], f32)] {
        &self.0
    }
    /// Preserve historical default serialization.
    /// Args: none. Returns true when the face uses its default coordinates.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(crate) fn key(&self) -> Vec<([u8; 4], u32)> {
        self.0.iter().map(|(t, v)| (*t, v.to_bits())).collect()
    }
}
impl<'de> Deserialize<'de> for FontVariations {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Axes;
        impl<'de> serde::de::Visitor<'de> for Axes {
            type Value = FontVariations;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("at most 64 unique, finite OpenType axis coordinates")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element()? {
                    if values.len() == 64 {
                        return Err(serde::de::Error::custom("Font axes exceed 64 coordinates"));
                    }
                    values.push(value);
                }
                FontVariations::new(values).map_err(serde::de::Error::custom)
            }
        }
        d.deserialize_seq(Axes)
    }
}
/// Resolve native character overrides without leaking coordinates into another face.
/// Args: text and scalar index. Returns the effective authored axis settings.
pub(crate) fn at(run: &TypeRun, index: usize) -> FontVariations {
    run.spans
        .iter()
        .find(|s| s.start <= index && index < s.end)
        .map_or_else(
            || run.font_variations.clone(),
            |s| {
                s.font_variations.clone().unwrap_or_else(|| {
                    if s.font.is_some() {
                        FontVariations::default()
                    } else {
                        run.font_variations.clone()
                    }
                })
            },
        )
}
/// Create the existing Rustybuzz face at the authored design-space point.
/// Args: existing cached font data and coordinates. Returns its ordinary shaping face.
pub(super) fn shaping_face<'a>(
    bytes: &'a [u8],
    variations: &FontVariations,
) -> Option<rustybuzz::Face<'a>> {
    let mut face = rustybuzz::Face::from_slice(bytes, 0)?;
    let values: Vec<_> = variations
        .settings()
        .iter()
        .map(|(t, v)| rustybuzz::Variation {
            tag: rustybuzz::ttf_parser::Tag::from_bytes(t),
            value: *v,
        })
        .collect();
    face.set_variations(&values);
    Some(face)
}
/// Create the existing outline face at exactly the same authored point as the shaper.
/// Args: existing cached font data and coordinates. Returns its native outline face.
pub(super) fn outline_face<'a>(
    bytes: &'a [u8],
    variations: &FontVariations,
) -> Option<ab_glyph::FontRef<'a>> {
    let mut face = ab_glyph::FontRef::try_from_slice(bytes).ok()?;
    for (tag, value) in variations.settings() {
        face.set_variation(tag, *value);
    }
    Some(face)
}
/// Serialize authored coordinates as standard SVG CSS.
/// Args: validated settings. Returns an escaped-safe CSS value using the original numeric coordinates.
pub(crate) fn css(settings: &FontVariations) -> String {
    settings
        .settings()
        .iter()
        .map(|(tag, value)| {
            format!(
                "'{}' {value}",
                tag.iter()
                    .map(|b| if b.is_ascii_alphanumeric() || *b == b' ' {
                        char::from(*b).to_string()
                    } else {
                        format!("\\{:x} ", b)
                    })
                    .collect::<String>()
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Detect axis settings in current artwork and stored instance baselines.
/// Args: one composition document. Returns whether its typed text requires the variable-font reader.
pub(crate) fn document(doc: &crate::document::Document) -> bool {
    fn shape(s: &crate::document::Shape) -> bool {
        matches!(&s.geom, crate::geom::Geom::Text(run) if !run.font_variations.is_empty() || run.spans.iter().any(|s| s.font_variations.is_some()))
    }
    doc.layers.iter().filter_map(|l| l.kind.shapes()).flatten().any(|s| {
        shape(s) || matches!(&s.layout.component, Some(crate::layout_components::ComponentBinding::Instance {nodes, ..}) if nodes.iter().any(|n| shape(&n.baseline)))
    })
}
/// Refuse a writer until its text route preserves authored variable coordinates.
/// Args: document and writer name. Returns a named refusal without altering the document.
pub(crate) fn require_writer(doc: &crate::document::Document, writer: &str) -> Result<(), String> {
    if document(doc) {
        Err(format!(
            "VariableFontUnsupported: {writer} does not yet preserve editable font-axis settings"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::{Document, Layer, Shape, Style},
        geom::{Geom, Pt},
    };
    const FONT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/fonts/EBGaramond.ttf"
    );
    fn point(weight: f32) -> FontVariations {
        FontVariations::new(vec![(*b"wght", weight)]).unwrap()
    }
    fn run() -> TypeRun {
        TypeRun {
            content: "AV fi Hgj".into(),
            font: FONT.into(),
            px: 42.,
            origin: Pt::new(20., 70.),
            ..Default::default()
        }
    }
    fn document(run: TypeRun) -> Document {
        let mut doc = Document::new("Editable font axes", 400., 160., 96.);
        let mut layer = Layer::vector("Native type");
        let mut shape = Shape::new(Geom::Text(run), Style::default());
        crate::text::fill_contours(&mut shape.geom);
        layer.kind.shapes_mut().unwrap().push(shape);
        doc.layers = vec![layer];
        doc
    }
    #[test]
    fn craft_variable_font_svg_keeps_source_axes_and_explicit_default_reset() {
        let source = r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="160"><text x="20" y="70" font-family="DejaVu Sans" font-size="42" style="font-variation-settings:'wght' 800">AV fi Hgj</text></svg>"#;
        let (doc, _) = crate::formats::svg::read(source, "Variable source").unwrap();
        let mut original = doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .find_map(|s| {
                if let Geom::Text(t) = &s.geom {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .unwrap();
        let count = original.content.chars().count();
        let mut legacy = original.clone();
        legacy.set_character_style(0, count, |s| s.font = Some(FONT.into()));
        let heavy = super::super::shape(&legacy);
        legacy.set_character_style(0, count, |s| s.font_variations = Some(point(400.)));
        let light = super::super::shape(&legacy);
        assert_ne!(light, heavy);
        legacy.set_character_style(0, count, |s| {
            s.font_variations = Some(FontVariations::default())
        });
        assert_eq!(super::super::shape(&legacy), light);
        let from = super::super::font_inventory::uses(&original)
            .unwrap()
            .into_iter()
            .map(|u| u.request)
            .collect::<Vec<_>>();
        assert!(super::super::font_inventory::replace(&mut original, &from, FONT).unwrap());
        assert!(original.spans.iter().all(|s| {
            s.font_variations
                .as_ref()
                .is_some_and(FontVariations::is_empty)
        }));
        assert_eq!(super::super::shape(&original), light);
        let encoded = crate::project::encode(&document(original)).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["version"],
            34
        );
        let escaped = FontVariations::new(vec![(*b"a'<>", 12.)]).unwrap();
        assert_eq!(css(&escaped), "'a\\27 \\3c \\3e ' 12");
    }
    #[test]
    fn craft_variable_font_coordinates_are_bounded_finite_unique_and_default_wire_stays_empty() {
        let original = serde_json::to_value(run()).unwrap();
        assert!(original.get("font_variations").is_none());
        assert!(
            serde_json::to_value(crate::geom::CharSpan::default())
                .unwrap()
                .get("font_variations")
                .is_none()
        );
        for values in [
            vec![(*b"wght", f32::NAN)],
            vec![(*b"wght", f32::INFINITY)],
            vec![(*b"bad\n", 1.)],
            vec![(*b"wght", 400.), (*b"wght", 500.)],
            vec![(*b"wght", 400.); 65],
        ] {
            assert!(FontVariations::new(values).is_err());
        }
        assert!(
            serde_json::from_str::<FontVariations>(&format!(
                "[{}]",
                vec!["[[119,103,104,116],400]"; 65].join(",")
            ))
            .is_err()
        );
        let settings = FontVariations::new(vec![(*b"wght", 700.), (*b"slnt", -0.)]).unwrap();
        let restored: FontVariations =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(settings.key(), restored.key());
        let mut axes = run();
        axes.font_variations = settings;
        let encoded = serde_json::to_string(&axes).unwrap();
        let decoded: TypeRun = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.font_variations.key(), axes.font_variations.key());
    }
    #[test]
    fn craft_variable_font_real_instance_changes_shared_metrics_curves_pixels_and_preserves_font_bytes()
     {
        let bytes = std::fs::read(FONT).unwrap();
        let face = rustybuzz::ttf_parser::Face::parse(&bytes, 0).unwrap();
        assert!(
            face.variation_axes()
                .into_iter()
                .any(|a| a.tag.to_bytes() == *b"wght"
                    && a.min_value <= 400.
                    && a.max_value >= 800.)
        );
        let mut light = run();
        light.font_variations = point(400.);
        let mut heavy = light.clone();
        heavy.font_variations = point(800.);
        let a = super::super::compose(&light);
        let b = super::super::compose(&heavy);
        assert_ne!(a[0].width, b[0].width);
        assert_ne!(
            super::super::outline_path(&light)
                .unwrap()
                .segments()
                .collect::<Vec<_>>(),
            super::super::outline_path(&heavy)
                .unwrap()
                .segments()
                .collect::<Vec<_>>()
        );
        let light_doc = document(light);
        let heavy_doc = document(heavy);
        let before = crate::compositor::export_png(&light_doc, 1).unwrap();
        let after = crate::compositor::export_png(&heavy_doc, 1).unwrap();
        assert_ne!(before, after);
        let id = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
        register_memory_font(&id, "EB Garamond variable", Arc::new(bytes.clone())).unwrap();
        let mut portable = heavy_doc.clone();
        if let Geom::Text(t) = &mut portable.layers[0].kind.shapes_mut().unwrap()[0].geom {
            t.font = id;
        }
        let encoded = crate::project::encode(&portable).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["version"],
            34
        );
        let reopened = crate::project::decode(&encoded).unwrap();
        assert_eq!(crate::compositor::export_png(&reopened, 1).unwrap(), after);
        let svg = crate::svg::export(&reopened).unwrap();
        assert!(svg.contains("<path"));
        assert!(!svg.contains("<text"));
        assert_eq!(std::fs::read(FONT).unwrap(), bytes);
        if let Some(root) = std::env::var_os("OMADESIGN_VARIABLE_FONT_QA") {
            let root = PathBuf::from(root);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("weight400.png"), before).unwrap();
            std::fs::write(root.join("weight800.png"), after).unwrap();
            std::fs::write(root.join("variable.oma"), encoded).unwrap();
            std::fs::write(root.join("variable.svg"), svg).unwrap();
        }
    }
    #[test]
    fn craft_variable_font_mixed_axis_spans_shape_in_one_story_and_plain_font_override_resets_axes()
    {
        let mut base = run();
        base.content = "fiAV".into();
        base.font_variations = point(400.);
        let default = super::super::compose(&base);
        base.set_character_style(0, 2, |s| s.font_variations = Some(point(800.)));
        assert_eq!(at(&base, 0), point(800.));
        assert_eq!(at(&base, 2), point(400.));
        let varied = super::super::compose(&base);
        assert_ne!(varied[0].width, default[0].width);
        assert_eq!(
            varied[0].glyphs.iter().filter(|g| g.cluster < 2).count(),
            1,
            "Style interval retains the real fi ligature"
        );
        let mut heavy = base.clone();
        heavy.content = "fi".into();
        heavy.spans.clear();
        heavy.font_variations = point(800.);
        assert_eq!(
            varied[0].glyphs[0].advance,
            super::super::compose(&heavy)[0].glyphs[0].advance
        );
        base.set_character_style(0, 2, |s| {
            s.font = Some(FONT.into());
            s.font_variations = None;
        });
        assert!(at(&base, 0).is_empty());
        assert_eq!(at(&base, 2), point(400.));
        let mut destination = run();
        destination.content = "fi".into();
        destination.font_variations = point(800.);
        let copied = crate::clipboard::type_style::RichText::from_run(&base, 0, 2);
        copied.apply_styles(&mut destination, 0);
        assert!(at(&destination, 0).is_empty());
    }
    #[test]
    fn craft_variable_font_project_underdeclare_retained_and_old_live_refuse_before_typed_loss() {
        let mut text = run();
        text.font_variations = point(700.);
        let doc = document(text);
        let encoded = crate::project::encode(&doc).unwrap();
        let mut json: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(json["version"], 34);
        json["version"] = 33.into();
        assert!(crate::project::decode(&json.to_string()).is_err());
        assert!(crate::collaboration::snapshot(&doc).is_err());
        let cells = std::collections::BTreeMap::from([(
            "/@entities/1/geom/Text/font_variations".into(),
            crate::collaboration::Cell::Value(serde_json::to_value(point(700.)).unwrap()),
        )]);
        let replica = crate::collaboration::Replica::new("font-axis".into(), "actor".into(), cells);
        assert!(replica.protocol_requirement().is_err());
        let before = encoded.clone();
        assert!(
            crate::formats::idml::write(&doc)
                .unwrap_err()
                .contains("VariableFontUnsupported")
        );
        assert!(
            crate::formats::pdf::write(&doc)
                .unwrap_err()
                .contains("VariableFontUnsupported")
        );
        assert_eq!(crate::project::encode(&doc).unwrap(), before);
        json["version"] = 34.into();
        json["doc"]["layers"][0]["kind"]["Vector"]["shapes"][0]["geom"]["Text"]["font_variations"] =
            serde_json::json!([[[119, 103, 104, 116], 700], [[119, 103, 104, 116], 800]]);
        assert!(crate::project::decode(&json.to_string()).is_err());
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::geom::{Geom, Pt};
    const FONT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/fonts/EBGaramond.ttf"
    );
    fn run(weight: f32) -> TypeRun {
        TypeRun {
            content: "AV fi Font".into(),
            font: FONT.into(),
            px: 36.,
            font_variations: FontVariations::new(vec![(*b"wght", weight)]).unwrap(),
            ..Default::default()
        }
    }
    #[test]
    fn craft_variable_font_frame_and_path_use_the_same_variable_outlines() {
        let mut light = run(400.);
        light.frame = Some(crate::text_geometry::TextFrame {
            size: Pt::new(300., 120.),
            ..Default::default()
        });
        let mut heavy = light.clone();
        heavy.font_variations = FontVariations::new(vec![(*b"wght", 800.)]).unwrap();
        let a = super::super::shape(&light);
        let b = super::super::shape(&heavy);
        assert!(!a.is_empty());
        assert!(!b.is_empty());
        assert_ne!(a, b);
        let mut path = crate::text_geometry::TextOnPath::default();
        path.cache = Some(crate::text_geometry::ArcPath::new(
            vec![[
                Pt::new(0., 100.),
                Pt::new(100., 100.),
                Pt::new(200., 100.),
                Pt::new(300., 100.),
            ]],
            false,
        ));
        light.frame = None;
        heavy.frame = None;
        light.on_path = Some(path.clone());
        heavy.on_path = Some(path);
        let a = super::super::shape(&light);
        let b = super::super::shape(&heavy);
        assert!(!a.is_empty());
        assert!(!b.is_empty());
        assert_ne!(a, b);
    }
    #[test]
    fn craft_variable_font_find_replace_preserves_unmatched_axis_ranges_and_one_document_undo() {
        let mut s = crate::app::Studio::new();
        s.show_welcome = false;
        s.doc = crate::document::Document::new("Axis replacements", 300., 180., 96.);
        s.doc.layers = vec![crate::document::Layer::vector("Text")];
        let mut run = run(700.);
        run.set_character_style(0, 2, |span| {
            span.font_variations = Some(FontVariations::new(vec![(*b"wght", 800.)]).unwrap())
        });
        let mut shape =
            crate::document::Shape::new(Geom::Text(run), crate::document::Style::default());
        super::super::fill_contours(&mut shape.geom);
        let id = shape.id;
        s.doc.layers[0].kind.shapes_mut().unwrap().push(shape);
        s.selection = vec![(0, id)];
        s.active_layer = Some(0);
        s.mark();
        s.history.clear();
        let before = crate::project::encode(&s.doc).unwrap();
        let rows = s.font_usage().unwrap();
        assert_eq!(rows.len(), 2);
        let row = rows.iter().find(|r| r.info.style.contains("800")).unwrap();
        let target = s
            .capture_font_change(crate::app::type_commands::TextScope::Document, &row.from)
            .unwrap();
        assert_eq!(s.replace_font_usage(&target, FONT).unwrap(), 1);
        assert_eq!(s.history.len(), 1);
        let Geom::Text(run) = &s.doc.find_shape(0, id).unwrap().geom else {
            panic!()
        };
        assert!(at(run, 0).is_empty());
        assert_eq!(at(run, 2).settings(), &[(*b"wght", 700.)]);
        let after = crate::project::encode(&s.doc).unwrap();
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        s.redo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), after);
    }
}
