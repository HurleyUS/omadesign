use super::*;
use crate::artwork_mask::MaskSource;
#[path = "group_clipping.rs"]
mod clipping;

/// Build the hierarchy once per draw; documents without groups retain the flat fast path.
pub(super) fn draw(
    pm: &mut Pixmap,
    doc: &Document,
    transform: Transform,
    draft: &Draft<'_>,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Result<(), String> {
    let sources = doc.mask_sources();
    if doc.layers.iter().all(|layer| {
        !layer.clipped && !layer.is_group && layer.parent.is_none() && layer.svg_filter.is_none()
    }) {
        for (index, layer) in doc.layers.iter().enumerate() {
            if sources.contains(&MaskSource {
                layer: layer.id,
                shape: None,
            }) || !layer.visible
                || (layer.kind.pixels().is_none() && layer.opacity <= 0.0)
                || is_paper_raster(layer)
            {
                continue;
            }
            let brush = draft
                .brush
                .and_then(|(i, p, f)| (i == index).then_some((p, f)));
            draw_layer(
                pm,
                layer,
                transform,
                brush,
                draft.preview,
                motion,
                doc,
                overrides,
                &sources,
            );
        }
        return Ok(());
    }
    let mut children: HashMap<Option<u64>, Vec<usize>> = HashMap::new();
    for (index, layer) in doc.layers.iter().enumerate() {
        children.entry(layer.parent).or_default().push(index);
    }
    let context = Context {
        doc,
        transform,
        draft,
        motion,
        overrides,
        children,
        sources,
    };
    context.draw_children(pm, None, 0)
}

struct Context<'a, 'b> {
    doc: &'a Document,
    transform: Transform,
    draft: &'a Draft<'b>,
    motion: Option<f32>,
    overrides: Option<&'a HashMap<u64, Pose>>,
    children: HashMap<Option<u64>, Vec<usize>>,
    sources: std::collections::HashSet<MaskSource>,
}

impl Context<'_, '_> {
    fn draw_children(
        &self,
        pm: &mut Pixmap,
        parent: Option<u64>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("Layer render group depth exceeds 64".into());
        }
        let Some(children) = self.children.get(&parent) else {
            return Ok(());
        };
        self.draw_list(pm, children, depth, false)
    }

    fn draw_list(
        &self,
        pm: &mut Pixmap,
        children: &[usize],
        depth: usize,
        normalize_first: bool,
    ) -> Result<(), String> {
        let mut start = 0;
        while start < children.len() {
            let mut end = crate::layer_clipping::stack_end(self.doc, children, start);
            if normalize_first && start == 0 {
                while end < children.len() && self.doc.layers[children[end]].clipped {
                    end += 1;
                }
            }
            let index = children[start];
            if self.sources.contains(&MaskSource {
                layer: self.doc.layers[index].id,
                shape: None,
            }) {
                start = end;
                continue;
            }
            if self.doc.layers[index].visible {
                if end > start + 1 {
                    self.draw_clipped(pm, &children[start..end], depth)?;
                } else {
                    self.draw_layer(pm, index, depth)?;
                }
            }
            start = end;
        }
        Ok(())
    }

    fn draw_layer(&self, pm: &mut Pixmap, index: usize, depth: usize) -> Result<(), String> {
        let layer = &self.doc.layers[index];
        if depth > 64 || !layer.visible {
            return Ok(());
        }
        let mut filters = layer.filters.clone();
        crate::filter::scale(&mut filters, self.transform.get_scale().0);
        let pad = crate::filter::svg_pad(&filters).ceil() as u32;
        if (layer.is_group || layer.svg_filter.is_some()) && filters.active() && pad > 0 {
            let border = pad.checked_mul(2).ok_or("Layer effect padding overflow")?;
            let width = pm
                .width()
                .checked_add(border)
                .ok_or("Layer effect width overflow")?;
            let height = pm
                .height()
                .checked_add(border)
                .ok_or("Layer effect height overflow")?;
            let bytes = crate::deep_color::storage::count(width, height, 4)?;
            let _memory = crate::resources::global().reserve_now(bytes as u64, &|| Ok(()))?;
            let mut pixels =
                Pixmap::new(width, height).ok_or("Could not allocate layer effect padding")?;
            let source = PixmapPaint {
                blend_mode: tiny_skia::BlendMode::Source,
                ..Default::default()
            };
            pixels.draw_pixmap(
                pad as i32,
                pad as i32,
                pm.as_ref(),
                &source,
                Transform::identity(),
                None,
            );
            Context {
                doc: self.doc,
                transform: Transform::from_translate(pad as f32, pad as f32)
                    .pre_concat(self.transform),
                draft: self.draft,
                motion: self.motion,
                overrides: self.overrides,
                children: self.children.clone(),
                sources: self.sources.clone(),
            }
            .draw_layer_inner(&mut pixels, index, depth)?;
            pm.draw_pixmap(
                -(pad as i32),
                -(pad as i32),
                pixels.as_ref(),
                &source,
                Transform::identity(),
                None,
            );
            return Ok(());
        }

        self.draw_layer_inner(pm, index, depth)
    }

    fn draw_layer_inner(&self, pm: &mut Pixmap, index: usize, depth: usize) -> Result<(), String> {
        self.draw_layer_content(pm, index, depth, &self.doc.layers[index])
    }

    fn draw_layer_content(
        &self,
        pm: &mut Pixmap,
        index: usize,
        depth: usize,
        layer: &Layer,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("Layer render group depth exceeds 64".into());
        }
        let mut filters = layer.filters.clone();
        crate::filter::scale(&mut filters, self.transform.get_scale().0);
        if !layer.visible
            || (layer.kind.pixels().is_none()
                && (layer.opacity <= 0.0
                    || masked_outside_view(layer, self.transform, pm.width(), pm.height())))
        {
            return Ok(());
        }
        if !layer.is_group && layer.svg_filter.is_none() {
            if is_paper_raster(layer) {
                return Ok(());
            }
            let brush = self
                .draft
                .brush
                .and_then(|(i, p, f)| (i == index).then_some((p, f)));
            draw_layer(
                pm,
                layer,
                self.transform,
                brush,
                self.draft.preview,
                self.motion,
                self.doc,
                self.overrides,
                &self.sources,
            );
            return Ok(());
        }
        if layer.pass_through
            && layer.opacity >= 1.0
            && layer.fill_opacity >= 1.0
            && layer.mask.is_none()
            && layer.artwork_masks.is_empty()
            && !layer.filters.active()
            && layer.svg_filter.is_none()
        {
            self.draw_children(pm, Some(layer.id), depth + 1)?;
            return Ok(());
        }
        let passthrough = layer.pass_through
            && layer.artwork_masks.is_empty()
            && !layer.filters.active()
            && layer.svg_filter.is_none();
        let Some(mut temp) = (if passthrough {
            Some(pm.clone())
        } else {
            Pixmap::new(pm.width(), pm.height())
        }) else {
            return Err("Could not allocate isolated layer pixels".into());
        };
        if layer.svg_filter.is_some() {
            match svg_effects::render(
                self.doc,
                index,
                self.transform,
                pm.width(),
                pm.height(),
                self.motion,
                self.overrides,
            ) {
                Ok(surface) => temp = surface,
                Err(error) => {
                    if error.contains("LayerClippingUnsupported") {
                        return Err(error);
                    }
                    eprintln!("SVG effect graph: {error}");
                    self.draw_children(&mut temp, Some(layer.id), depth + 1)?;
                }
            }
        } else {
            self.draw_children(&mut temp, Some(layer.id), depth + 1)?;
        }
        let mask = layer.mask.as_ref().and_then(|pixels| {
            let mut placed = Pixmap::new(pm.width(), pm.height())?;
            pixels.with_pm(|mask| {
                placed.draw_pixmap(
                    0,
                    0,
                    mask.as_ref(),
                    &PixmapPaint::default(),
                    self.transform.pre_concat(layer_pixel_transform(layer)),
                    None,
                )
            })?;
            Some(tiny_skia::Mask::from_pixmap(
                placed.as_ref(),
                tiny_skia::MaskType::Luminance,
            ))
        });
        let live = artwork_masks::render(
            &layer.artwork_masks,
            self.doc,
            self.transform,
            pm.width(),
            pm.height(),
            self.motion,
            self.overrides,
        );
        if passthrough {
            // Interpolate with the original backdrop so child blend modes still see it.
            let opacity = (layer.opacity * layer.fill_opacity).clamp(0.0, 1.0);
            for (i, (dst, src)) in pm
                .data_mut()
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(temp.data().as_chunks::<4>().0)
                .enumerate()
            {
                let amount = opacity
                    * mask
                        .as_ref()
                        .map(|m| m.data()[i] as f32 / 255.0)
                        .unwrap_or(1.0);
                for c in 0..4 {
                    dst[c] =
                        (dst[c] as f32 * (1.0 - amount) + src[c] as f32 * amount).round() as u8;
                }
            }
        } else {
            if let Some(mask) = mask {
                temp.apply_mask(&mask);
            }
            if filters.independent() || layer.fill_opacity < 1. || layer.blend_interior {
                crate::filter::composite(
                    pm,
                    temp,
                    &filters,
                    Transform::identity(),
                    layer.blend.to_skia(),
                    layer.opacity,
                    layer.fill_opacity,
                    layer.blend_interior,
                    live.as_ref(),
                );
                return Ok(());
            }
            let cached;
            let (pixels, coverage) = if filters.active() {
                cached = surface_cache::render(layer.id, temp, &filters);
                (cached.as_ref(), surface_cache::bounds(layer.id, &cached))
            } else {
                (&temp, bounded_blit::bounds(temp.as_ref()))
            };
            if let Some(mask) = &live {
                pm.draw_pixmap(
                    0,
                    0,
                    pixels.as_ref(),
                    &PixmapPaint {
                        opacity: layer.opacity.clamp(0., 1.),
                        blend_mode: layer.blend.to_skia(),
                        ..Default::default()
                    },
                    Transform::identity(),
                    Some(mask),
                );
            } else {
                bounded_blit::draw(
                    pm,
                    pixels.as_ref(),
                    coverage,
                    layer.opacity,
                    layer.blend,
                    Transform::identity(),
                );
            }
        }
        Ok(())
    }
}

/// Draw a selected sibling scope while keeping original mask and animation owners.
/// Args: destination, complete document, painter-ordered roots, placement and current poses. Returns a render/allocation error before publication.
pub(super) fn draw_items(
    pm: &mut Pixmap,
    doc: &Document,
    roots: &[usize],
    transform: Transform,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Result<(), String> {
    let mut children: HashMap<Option<u64>, Vec<usize>> = HashMap::new();
    for (i, layer) in doc.layers.iter().enumerate() {
        children.entry(layer.parent).or_default().push(i);
    }
    Context {
        doc,
        transform,
        draft: &Draft::none(),
        motion,
        overrides,
        children,
        sources: doc.mask_sources(),
    }
    .draw_list(pm, roots, 0, true)
}

pub(super) fn draw_item(
    pm: &mut Pixmap,
    doc: &Document,
    index: usize,
    transform: Transform,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Result<(), String> {
    let mut children: HashMap<Option<u64>, Vec<usize>> = HashMap::new();
    for (i, layer) in doc.layers.iter().enumerate() {
        children.entry(layer.parent).or_default().push(i);
    }
    Context {
        doc,
        transform,
        draft: &Draft::none(),
        motion,
        overrides,
        children,
        sources: doc.mask_sources(),
    }
    .draw_layer(pm, index, 0)
}

/// Draw the live transparency that owns a clipping stack, excluding its outer effects.
/// Args: destination, complete document, base index and current viewport/poses. Returns a render/allocation error.
pub(super) fn clipping_base(
    pm: &mut Pixmap,
    doc: &Document,
    index: usize,
    transform: Transform,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Result<(), String> {
    let mut layer = doc
        .layers
        .get(index)
        .ok_or("Missing clipping base")?
        .clone();
    layer.filters = Default::default();
    layer.blend = crate::color::Blend::Normal;
    layer.pass_through = false;
    let mut children: HashMap<Option<u64>, Vec<usize>> = HashMap::new();
    for (i, layer) in doc.layers.iter().enumerate() {
        children.entry(layer.parent).or_default().push(i);
    }
    Context {
        doc,
        transform,
        draft: &Draft::none(),
        motion,
        overrides,
        children,
        sources: doc.mask_sources(),
    }
    .draw_layer_content(pm, index, 0, &layer)
}

/// Restrict an isolated layer or object to its live clipping base.
/// Args: rendered item, document, owning layer and actual viewport/poses. Returns without changing unclipped artwork.
pub(super) fn apply_clipping_mask(
    pixels: &mut Pixmap,
    doc: &Document,
    index: usize,
    transform: Transform,
    motion: Option<f32>,
    overrides: Option<&HashMap<u64, Pose>>,
) -> Result<(), String> {
    for base in doc.clipping_bases(index) {
        if !doc.layer_visible(base) {
            pixels.fill(tiny_skia::Color::TRANSPARENT);
            return Ok(());
        }
        let bytes = crate::deep_color::storage::count(pixels.width(), pixels.height(), 5)?;
        let _memory = crate::resources::global().reserve_now(bytes as u64, &|| Ok(()))?;
        let mut mask = Pixmap::new(pixels.width(), pixels.height())
            .ok_or("Could not allocate clipping mask")?;
        clipping_base(&mut mask, doc, base, transform, motion, overrides)?;
        pixels.apply_mask(&tiny_skia::Mask::from_pixmap(
            mask.as_ref(),
            tiny_skia::MaskType::Alpha,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        color::Blend,
        document::{Layer, Pixels},
    };

    #[test]
    fn filtered_group_keeps_the_same_pixels_when_the_viewport_crops_its_source() {
        let mut doc = Document::new("Group edge", 16., 12., 96.);
        let mut group = Layer::group("Shadow");
        group.pass_through = false;
        group.filters.items.push(crate::filter::Fx::Shadow {
            dx: 2.,
            dy: 1.,
            blur: 0.,
            spread: 0.,
            color: Rgba::rgb(30, 20, 10),
            knockout: false,
            blend: Blend::Normal,
            opacity: 1.,
        });
        let mut layer = Layer::raster("Source", 4, 4).unwrap();
        layer
            .kind
            .pixels_mut()
            .unwrap()
            .replace_bytes([210, 60, 40, 128].repeat(16))
            .unwrap();
        layer
            .kind
            .set_raster_xform(Pt::new(-2., -2.), Pt::splat(8.), 0.);
        layer.parent = Some(group.id);
        doc.layers = vec![layer, group];
        let mut full = Pixmap::new(64, 48).unwrap();
        let mut cropped = Pixmap::new(16, 12).unwrap();
        draw(
            &mut full,
            &doc,
            Transform::from_translate(12., 12.),
            &Draft::none(),
            None,
            None,
        )
        .unwrap();
        draw(
            &mut cropped,
            &doc,
            Transform::identity(),
            &Draft::none(),
            None,
            None,
        )
        .unwrap();
        let expected: Vec<_> = full
            .data()
            .chunks_exact(64 * 4)
            .skip(12)
            .take(12)
            .flat_map(|row| row[12 * 4..28 * 4].iter().copied())
            .collect();
        assert!(expected.chunks_exact(4).any(|pixel| pixel[3] > 0));
        assert_eq!(cropped.data(), expected);
    }
    #[test]
    fn group_opacity_is_applied_once_and_hidden_parent_hides_children() {
        let mut doc = Document::new("groups", 2., 2., 72.);
        doc.transparent = true;
        doc.layers.clear();
        let mut group = Layer::group("Group");
        group.pass_through = false;
        group.opacity = 0.5;
        for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
            let mut layer = Layer::placed_raster(
                "child",
                Pixels::from_rgba(2, 2, color.repeat(4)).unwrap(),
                Pt::ZERO,
                Pt::new(2., 2.),
            );
            layer.parent = Some(group.id);
            doc.layers.push(layer);
        }
        doc.layers.push(group);
        let pm = render_export(&doc, 1).unwrap();
        assert_eq!(pm.pixels()[0].alpha(), 128);
        assert_eq!(pm.pixels()[0].blue(), 128);
        doc.layers[2].visible = false;
        assert_eq!(render_export(&doc, 1).unwrap().pixels()[0].alpha(), 0);
        assert!(doc.hit_test(Pt::new(1., 1.), 1.).is_none());
        doc.layers[2].visible = true;
        doc.layers[2].pass_through = true;
        doc.layers[1].blend = Blend::Multiply;
        assert_eq!(render_export(&doc, 1).unwrap().pixels()[0].alpha(), 128);
        let opacity_pixels = render_export(&doc, 1).unwrap();
        doc.layers[2].opacity = 1.;
        doc.layers[2].fill_opacity = 0.5;
        assert_eq!(
            render_export(&doc, 1).unwrap().data(),
            opacity_pixels.data(),
            "Fill opacity must preserve a pass-through child's backdrop"
        );
        let decoded = crate::project::decode(&crate::project::encode(&doc).unwrap()).unwrap();
        assert_eq!(decoded.layers[0].parent, Some(decoded.layers[2].id));
    }
}
