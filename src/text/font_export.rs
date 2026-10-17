//! Export-only static instances keep the original editable font and coordinates out of the conversion.
use super::{font_catalog, font_instances, font_inventory, variations::FontVariations};
use rustybuzz::ttf_parser::{Face, GlyphId, Permissions, Tag};
use std::{collections::BTreeMap, sync::Arc};

/// Embed the rendered point as a full, independently readable TrueType font.
/// Args: original cached bytes and authored coordinates. Returns the original static font or a new instance with stable glyph IDs.
pub(crate) fn instance(
    bytes: &Arc<Vec<u8>>,
    point: &FontVariations,
) -> Result<Arc<Vec<u8>>, String> {
    let source = Face::parse(bytes, 0).map_err(|_| "Invalid font for export")?;
    if !source.is_variable() {
        return Ok(bytes.clone());
    }
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Font exceeds the portable 16 MiB limit".into());
    }
    if !matches!(
        source.permissions(),
        Some(Permissions::Installable | Permissions::Editable)
    ) || !source.is_outline_embedding_allowed()
    {
        return Err("This font does not permit editable outline embedding".into());
    }
    let effective = FontVariations::new(
        source
            .variation_axes()
            .into_iter()
            .map(|a| {
                let value = point
                    .settings()
                    .iter()
                    .find(|(tag, _)| *tag == a.tag.to_bytes())
                    .map_or(a.def_value, |(_, v)| *v);
                (a.tag.to_bytes(), value.clamp(a.min_value, a.max_value))
            })
            .collect(),
    )?;
    let mut mapper = subsetter::GlyphRemapper::new();
    for gid in 0..source.number_of_glyphs() {
        if mapper.remap(gid) != gid {
            return Err("Font instance changed original glyph identities".into());
        }
    }
    let coordinates: Vec<_> = effective
        .settings()
        .iter()
        .map(|(tag, v)| (subsetter::Tag::new(tag), *v))
        .collect();
    let derived = subsetter::subset_with_variations(bytes, 0, &coordinates, &mapper)
        .map_err(|e| format!("Cannot instantiate this font: {e}"))?;
    if derived.len() > 16 * 1024 * 1024 {
        return Err("Font instance exceeds the portable 16 MiB limit".into());
    }
    let face = Face::parse(&derived, 0).map_err(|_| "Font instancer returned an invalid face")?;
    if face.is_variable()
        || face.tables().glyf.is_none()
        || face.number_of_glyphs() != source.number_of_glyphs()
    {
        return Err("Font instancer did not preserve a static glyph program".into());
    }
    let mut tables = BTreeMap::new();
    for record in face.raw_face().table_records {
        let tag = record.tag.to_bytes();
        if let Some(data) = face.raw_face().table(record.tag) {
            tables.insert(tag, data.to_vec());
        }
    }
    for tag in [b"cmap", b"OS/2", b"GDEF"] {
        if let Some(data) = source.raw_face().table(Tag::from_bytes(tag)) {
            tables.insert(*tag, data.to_vec());
        }
    }
    if let Some(gsub) = source.raw_face().table(Tag::from_bytes(b"GSUB"))
        && (gsub.get(..4) == Some(&[0, 1, 0, 0])
            || gsub.get(..4) == Some(&[0, 1, 0, 1]) && gsub.get(10..14) == Some(&[0, 0, 0, 0]))
    {
        tables.insert(*b"GSUB", gsub.to_vec());
    }
    let style = font_instances::read(&source)
        .1
        .into_iter()
        .find(|i| font_instances::same_point(&source, &effective, &i.variations))
        .map(|i| i.style)
        .or_else(|| {
            font_instances::same_point(&source, &effective, &Default::default())
                .then(|| font_inventory::name(&source, &[17, 2]))
                .flatten()
        })
        .unwrap_or_else(|| format!("Instance {}", super::variations::css(&effective)));
    let family = font_inventory::name(&source, &[16, 1]).unwrap_or_else(|| "Omadesign font".into());
    let fingerprint = crate::typography::fingerprint(&derived);
    let ps = format!(
        "{}-{}-{fingerprint:032x}",
        family
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>(),
        style
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
    );
    tables.insert(*b"name", instance_names(&source, &family, &style, &ps)?);
    let mut bbox = None::<rustybuzz::ttf_parser::Rect>;
    for id in 0..face.number_of_glyphs() {
        if let Some(b) = face.glyph_bounding_box(GlyphId(id)) {
            bbox = Some(bbox.map_or(b, |a| rustybuzz::ttf_parser::Rect {
                x_min: a.x_min.min(b.x_min),
                y_min: a.y_min.min(b.y_min),
                x_max: a.x_max.max(b.x_max),
                y_max: a.y_max.max(b.y_max),
            }));
        }
    }
    if let Some(b) = bbox
        && let Some(head) = tables.get_mut(b"head")
    {
        if head.len() < 44 {
            return Err("Invalid font instance bounds".into());
        }
        for (n, value) in [b.x_min, b.y_min, b.x_max, b.y_max].into_iter().enumerate() {
            head[36 + n * 2..38 + n * 2].copy_from_slice(&value.to_be_bytes());
        }
    }
    if let Some(os2) = tables.get_mut(b"OS/2")
        && os2.len() >= 64
    {
        let weight = effective
            .settings()
            .iter()
            .find(|(tag, _)| tag == b"wght")
            .map_or(source.weight().to_number(), |(_, v)| {
                v.clamp(1., 1000.).round() as u16
            });
        os2[4..6].copy_from_slice(&weight.to_be_bytes());
        let italic = effective
            .settings()
            .iter()
            .find(|(tag, _)| tag == b"ital")
            .map_or(source.is_italic(), |(_, v)| *v >= 0.5)
            || effective
                .settings()
                .iter()
                .any(|(tag, v)| tag == b"slnt" && *v != 0.);
        let flags = u16::from_be_bytes(os2[62..64].try_into().unwrap()) & !0x61;
        let flags = flags
            | u16::from(italic)
            | if weight >= 700 {
                0x20
            } else if !italic {
                0x40
            } else {
                0
            };
        os2[62..64].copy_from_slice(&flags.to_be_bytes());
        if let Some(head) = tables.get_mut(b"head")
            && head.len() >= 46
        {
            let style = u16::from_be_bytes(head[44..46].try_into().unwrap()) & !3;
            head[44..46].copy_from_slice(
                &(style | u16::from(weight >= 700) | (u16::from(italic) << 1)).to_be_bytes(),
            );
        }
    }
    let output = font_catalog::encode_tables(derived[..4].try_into().unwrap(), tables)?;
    Face::parse(&output, 0).map_err(|_| "Invalid complete font instance")?;
    Ok(Arc::new(output))
}

fn instance_names(
    source: &Face<'_>,
    family: &str,
    style: &str,
    ps: &str,
) -> Result<Vec<u8>, String> {
    let data = source
        .raw_face()
        .table(Tag::from_bytes(b"name"))
        .ok_or("Font has no name table")?;
    let word = |p: usize| {
        data.get(p..p.checked_add(2)?)
            .map(|b| u16::from_be_bytes(b.try_into().unwrap()))
    };
    let version = word(0)
        .filter(|n| *n <= 1)
        .ok_or("Unsupported font name table")?;
    let count = word(2).ok_or("Invalid font names")? as usize;
    let start = word(4).ok_or("Invalid font names")? as usize;
    let mut records = Vec::new();
    let string = |length: u16, offset: u16| {
        data.get(
            start.checked_add(offset as usize)?
                ..start
                    .checked_add(offset as usize)?
                    .checked_add(length as usize)?,
        )
        .map(<[u8]>::to_vec)
    };
    for n in 0..count {
        let p = 6 + n * 12;
        let mut words = [0; 6];
        for (j, w) in words.iter_mut().enumerate() {
            *w = word(p + j * 2).ok_or("Invalid font name record")?;
        }
        if ![2, 3, 4, 6, 16, 17].contains(&words[3]) {
            records.push((
                words,
                string(words[4], words[5]).ok_or("Invalid font name string")?,
            ));
        }
    }
    for (id, value) in [
        (2, style.to_owned()),
        (3, ps.to_owned()),
        (4, format!("{family} {style}")),
        (6, ps.to_owned()),
        (16, family.to_owned()),
        (17, style.to_owned()),
    ] {
        let bytes = value
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect::<Vec<_>>();
        records.push(([3, 1, 0x409, id, 0, 0], bytes));
    }
    records.sort_by_key(|(r, _)| [r[0], r[1], r[2], r[3]]);
    let mut languages = vec![];
    if version == 1 {
        let n = word(6 + count * 12).ok_or("Invalid font language tags")? as usize;
        for i in 0..n {
            let at = 8 + count * 12 + i * 4;
            languages.push(
                string(
                    word(at).ok_or("Invalid font language tag")?,
                    word(at + 2).ok_or("Invalid font language tag")?,
                )
                .ok_or("Invalid font language tag")?,
            );
        }
    }
    let fail = || "Font instance names exceed their table bounds".to_owned();
    let header = 6
        + records.len() * 12
        + if version == 1 {
            2 + languages.len() * 4
        } else {
            0
        };
    let header = u16::try_from(header).map_err(|_| fail())?;
    let mut output = Vec::new();
    for n in [
        version,
        u16::try_from(records.len()).map_err(|_| fail())?,
        header,
    ] {
        output.extend_from_slice(&n.to_be_bytes());
    }
    let mut pool = Vec::new();
    let mut strings = std::collections::HashMap::<Vec<u8>, u16>::new();
    let mut intern = |bytes: Vec<u8>| -> Result<(u16, u16), String> {
        let len = u16::try_from(bytes.len()).map_err(|_| fail())?;
        let at = if let Some(&at) = strings.get(&bytes) {
            at
        } else {
            let at = u16::try_from(pool.len()).map_err(|_| fail())?;
            pool.extend_from_slice(&bytes);
            strings.insert(bytes, at);
            at
        };
        Ok((len, at))
    };
    for (mut record, bytes) in records {
        let (len, at) = intern(bytes)?;
        record[4] = len;
        record[5] = at;
        for n in record {
            output.extend_from_slice(&n.to_be_bytes());
        }
    }
    if version == 1 {
        output.extend_from_slice(&(languages.len() as u16).to_be_bytes());
        for bytes in languages {
            let (len, at) = intern(bytes)?;
            output.extend_from_slice(&len.to_be_bytes());
            output.extend_from_slice(&at.to_be_bytes());
        }
    }
    output.extend_from_slice(&pool);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    const FONT: &[u8] = include_bytes!("../../tests/assets/fonts/EBGaramond.ttf");
    #[test]
    fn craft_font_export_static_instances_keep_names_metrics_glyphs_mapping_and_original_bytes() {
        let bytes = Arc::new(FONT.to_vec());
        let original = Face::parse(FONT, 0).unwrap();
        let mut outputs = vec![];
        for (weight, name) in [(400., "Regular"), (700., "Bold"), (800., "ExtraBold")] {
            let point = FontVariations::new(vec![(*b"wght", weight)]).unwrap();
            let data = instance(&bytes, &point).unwrap();
            let face = Face::parse(&data, 0).unwrap();
            assert!(!face.is_variable());
            assert!(face.raw_face().table(Tag::from_bytes(b"gvar")).is_none());
            assert_eq!(face.number_of_glyphs(), original.number_of_glyphs());
            assert_eq!(face.weight().to_number(), weight as u16);
            assert_eq!(font_inventory::name(&face, &[17, 2]).as_deref(), Some(name));
            assert_eq!(
                font_inventory::name(&face, &[0]),
                font_inventory::name(&original, &[0])
            );
            assert_eq!(
                font_inventory::name(&face, &[13, 14]),
                font_inventory::name(&original, &[13, 14])
            );
            let mut expected = Face::parse(FONT, 0).unwrap();
            expected.set_variation(Tag::from_bytes(b"wght"), weight);
            for ch in "AVfi12éΩ".chars() {
                let glyph = original.glyph_index(ch).unwrap();
                assert_eq!(face.glyph_index(ch), Some(glyph));
                let a = face.glyph_hor_advance(glyph).unwrap();
                let b = expected.glyph_hor_advance(glyph).unwrap();
                assert!(a.abs_diff(b) <= 1, "{name} {ch}: {a} vs {b}");
                let a = face.glyph_bounding_box(glyph).unwrap();
                let b = expected.glyph_bounding_box(glyph).unwrap();
                for (x, y) in [a.x_min, a.y_min, a.x_max, a.y_max]
                    .into_iter()
                    .zip([b.x_min, b.y_min, b.x_max, b.y_max])
                {
                    assert!(x.abs_diff(y) <= 1, "{name} {ch}: {a:?} vs {b:?}");
                }
            }
            let checksum = data.chunks(4).fold(0u32, |s, b| {
                let mut word = [0; 4];
                word[..b.len()].copy_from_slice(b);
                s.wrapping_add(u32::from_be_bytes(word))
            });
            assert_eq!(checksum, 0xb1b0afba);
            let same = instance(&data, &point).unwrap();
            assert!(Arc::ptr_eq(&data, &same));
            if let Some(root) = std::env::var_os("OMADESIGN_FONT_EXPORT_QA") {
                let root = std::path::PathBuf::from(root);
                std::fs::create_dir_all(&root).unwrap();
                std::fs::write(root.join(format!("font-{weight}.ttf")), data.as_slice()).unwrap();
            }
            outputs.push(data);
        }
        assert_ne!(outputs[0], outputs[1]);
        assert_ne!(outputs[1], outputs[2]);
        assert_eq!(bytes.as_slice(), FONT);
    }
    #[test]
    fn craft_font_export_permissions_reject_restricted_and_preserve_whole_face_requirements() {
        let face = Face::parse(FONT, 0).unwrap();
        let table = face.raw_face().table(Tag::from_bytes(b"OS/2")).unwrap();
        let offset = table.as_ptr() as usize - FONT.as_ptr() as usize;
        let point = FontVariations::new(vec![(*b"wght", 800.)]).unwrap();
        for flags in [2u16, 4, 512] {
            let mut bytes = FONT.to_vec();
            bytes[offset + 8..offset + 10].copy_from_slice(&flags.to_be_bytes());
            assert!(
                instance(&Arc::new(bytes), &point)
                    .unwrap_err()
                    .contains("embedding")
            );
        }
        let mut bytes = FONT.to_vec();
        bytes[offset + 8..offset + 10].copy_from_slice(&0x108u16.to_be_bytes());
        let data = instance(&Arc::new(bytes), &point).unwrap();
        let out = Face::parse(&data, 0).unwrap();
        assert_eq!(out.number_of_glyphs(), face.number_of_glyphs());
        assert_eq!(
            &out.raw_face().table(Tag::from_bytes(b"OS/2")).unwrap()[8..10],
            &0x108u16.to_be_bytes()
        );
        assert!(instance(&Arc::new(vec![1, 2, 3]), &point).is_err());
        assert_eq!(
            FONT,
            include_bytes!("../../tests/assets/fonts/EBGaramond.ttf")
        );
    }
}
