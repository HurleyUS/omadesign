//! Named OpenType instances retain the original face and exact design-space coordinates.
use super::{font_inventory, variations::FontVariations};
use rustybuzz::ttf_parser::{Face, Tag};

pub(super) struct Instance {
    pub style: String,
    pub aliases: Vec<String>,
    pub variations: FontVariations,
}
fn names(face: &Face<'_>, id: u16) -> Vec<String> {
    let mut aliases: Vec<_> = face
        .names()
        .into_iter()
        .filter(|n| n.name_id == id)
        .filter_map(|n| n.to_string())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    aliases.sort_by_key(|s| s.to_lowercase());
    aliases.dedup_by(|a, b| a.to_lowercase() == b.to_lowercase());
    aliases
}
/// Read bounded named instances without deriving or modifying any font bytes.
/// Args: parsed face. Returns its default style aliases and named nondefault coordinate sets.
pub(super) fn read(face: &Face<'_>) -> (Vec<String>, Vec<Instance>) {
    let Some(data) = face.raw_face().table(Tag::from_bytes(b"fvar")) else {
        return Default::default();
    };
    parse(face, data).unwrap_or_default()
}
fn parse(face: &Face<'_>, data: &[u8]) -> Option<(Vec<String>, Vec<Instance>)> {
    let u16_at = |p: usize| {
        data.get(p..p.checked_add(2)?)
            .map(|v| u16::from_be_bytes(v.try_into().unwrap()) as usize)
    };
    let fixed = |p: usize| {
        data.get(p..p.checked_add(4)?)
            .map(|v| i32::from_be_bytes(v.try_into().unwrap()) as f32 / 65536.)
    };
    if data.get(..4) != Some(&[0, 1, 0, 0]) {
        return None;
    }
    let offset = u16_at(4)?;
    let axis_count = u16_at(8)?;
    let axis_size = u16_at(10)?;
    let count = u16_at(12)?;
    let size = u16_at(14)?;
    if offset < 16
        || axis_count == 0
        || axis_count > 64
        || axis_size != 20
        || count > 512
        || ![4 + axis_count * 4, 6 + axis_count * 4].contains(&size)
    {
        return None;
    }
    let start = offset.checked_add(axis_count.checked_mul(axis_size)?)?;
    data.get(offset..start.checked_add(count.checked_mul(size)?)?)?;
    let mut axes = Vec::with_capacity(axis_count);
    for n in 0..axis_count {
        let p = offset + n * axis_size;
        let tag: [u8; 4] = data.get(p..p + 4)?.try_into().ok()?;
        let (min, default, max) = (fixed(p + 4)?, fixed(p + 8)?, fixed(p + 12)?);
        if min > default || default > max {
            return None;
        }
        axes.push((tag, min, default, max));
    }
    FontVariations::new(axes.iter().map(|(tag, _, v, _)| (*tag, *v)).collect()).ok()?;
    let mut default_aliases = vec![];
    let mut instances = vec![];
    let mut taken = font_inventory::name(face, &[17, 2])
        .map(|s| vec![s.to_lowercase()])
        .unwrap_or_default();
    for n in 0..count {
        let p = start + n * size;
        let id = u16_at(p)? as u16;
        let Some(style) = font_inventory::name(face, &[id]) else {
            continue;
        };
        let coords: Vec<_> = axes
            .iter()
            .enumerate()
            .map(|(j, (tag, _, _, _))| Some((*tag, fixed(p + 4 + j * 4)?)))
            .collect::<Option<_>>()?;
        if coords
            .iter()
            .zip(&axes)
            .any(|((_, v), (_, min, _, max))| v < min || v > max)
        {
            continue;
        }
        let aliases = names(face, id);
        if coords
            .iter()
            .zip(&axes)
            .all(|((_, v), (_, _, default, _))| v == default)
        {
            taken.extend(aliases.iter().map(|name| name.to_lowercase()));
            default_aliases.extend(aliases);
            continue;
        }
        let key = style.to_lowercase();
        if taken.contains(&key) {
            continue;
        }
        taken.push(key);
        let variations = FontVariations::new(coords).ok()?;
        instances.push(Instance {
            style,
            aliases,
            variations,
        });
    }
    default_aliases.sort();
    default_aliases.dedup();
    Some((default_aliases, instances))
}
/// Compare an authored point against one named instance, including omitted default coordinates.
/// Args: parsed face, authored settings and candidate full instance point. Returns exact semantic equality.
pub(super) fn same_point(
    face: &Face<'_>,
    authored: &FontVariations,
    candidate: &FontVariations,
) -> bool {
    let axes = face.variation_axes();
    if authored
        .settings()
        .iter()
        .any(|(tag, _)| !axes.into_iter().any(|a| a.tag.to_bytes() == *tag))
    {
        return false;
    }
    axes.into_iter().all(|a| {
        let value = |settings: &FontVariations| {
            settings
                .settings()
                .iter()
                .find(|(t, _)| *t == a.tag.to_bytes())
                .map_or(a.def_value, |(_, v)| *v)
        };
        value(authored) == value(candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const FONT: &[u8] = include_bytes!("../../tests/assets/fonts/EBGaramond.ttf");
    #[test]
    fn craft_variable_font_named_records_keep_real_names_points_defaults_and_input_bounds() {
        let face = Face::parse(FONT, 0).unwrap();
        let (aliases, instances) = read(&face);
        assert!(aliases.contains(&"Regular".into()));
        assert_eq!(
            instances
                .iter()
                .map(|i| i.style.as_str())
                .collect::<Vec<_>>(),
            ["Medium", "SemiBold", "Bold", "ExtraBold"]
        );
        for (instance, weight) in instances.iter().zip([500., 600., 700., 800.]) {
            assert_eq!(instance.variations.settings(), &[(*b"wght", weight)]);
            assert!(instance.aliases.contains(&instance.style));
            assert!(same_point(
                &face,
                &instance.variations,
                &instance.variations
            ));
            assert!(!same_point(
                &face,
                &FontVariations::default(),
                &instance.variations
            ));
        }
        let data = face.raw_face().table(Tag::from_bytes(b"fvar")).unwrap();
        for end in [0, 4, 15, 16, data.len() - 1] {
            assert!(parse(&face, &data[..end]).is_none());
        }
        for (at, value) in [(8, 65u16), (10, 19),
            (10, 21), (12, 513), (14, 1), (4, 15)] {
            let mut bad = data.to_vec();
            bad[at..at + 2].copy_from_slice(&value.to_be_bytes());
            assert!(parse(&face, &bad).is_none());
        }
        let mut renamed = data.to_vec();
        let offset = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
        let axes = u16::from_be_bytes(data[8..10].try_into().unwrap()) as usize;
        let axis_size = u16::from_be_bytes(data[10..12].try_into().unwrap()) as usize;
        let size = u16::from_be_bytes(data[14..16].try_into().unwrap()) as usize;
        let first = offset + axes * axis_size;
        renamed[first..first + 2].copy_from_slice(&data[first + size..first + size + 2]);
        let (aliases, instances) = parse(&face, &renamed).unwrap();
        assert!(aliases.contains(&"Medium".into()));
        assert!(!instances.iter().any(|i| i.style == "Medium"));
        assert_eq!(
            FONT,
            include_bytes!("../../tests/assets/fonts/EBGaramond.ttf")
        );
    }
    #[test]
    fn craft_variable_font_catalog_named_choice_closest_and_original_portable_face_agree() {
        let file = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/assets/fonts/EBGaramond.ttf"
        );
        let catalog = crate::text::font_catalog::Catalog::new([file.into()]);
        let choices = catalog.styles("EB Garamond");
        for (style, weight) in [
            ("Regular", 400.),
            ("Medium", 500.),
            ("SemiBold", 600.),
            ("Bold", 700.),
            ("ExtraBold", 800.),
        ] {
            let choice = catalog
                .choice(
                    "EB Garamond",
                    Some(style),
                    &font_inventory::Request::Stored(file.into()),
                )
                .unwrap();
            assert_eq!(choice.style, style);
            assert_eq!(choice.weight, weight as u16);
            if weight == 400. {
                assert!(choice.variations.is_empty());
            } else {
                assert_eq!(choice.variations.settings(), &[(*b"wght", weight)]);
            }
            let reference = catalog.reference(&choice).unwrap();
            assert_eq!(
                reference,
                format!("omatype:{:032x}", crate::typography::fingerprint(FONT))
            );
            assert_eq!(
                super::super::project_font_bytes(&reference)
                    .unwrap()
                    .as_slice(),
                FONT
            );
            assert!(choices.iter().any(|c| c.style == style));
        }
        let request = font_inventory::Request::Variable {
            source: Box::new(font_inventory::Request::Stored(file.into())),
            coordinates: vec![(*b"wght", 700f32.to_bits())],
        };
        assert_eq!(
            catalog.choice("EB Garamond", None, &request).unwrap().style,
            "Bold"
        );
        let info = font_inventory::describe_with(&catalog, &request, file, &['A'].into());
        assert_eq!(info.style, "Bold (wght 700)");
    }
}
