#[cfg(test)]
mod clip_tests;
mod coverage;
mod paint;
mod shape_mask;
#[cfg(test)]
mod tests;
mod vector;

use super::Depth;
use crate::{
    color::Blend,
    document::{Document, Layer, Pixels},
    geom::Pt,
};

/// Composite straight RGBA in floating-point precision.
/// Args: backdrop, source, blend mode and source coverage. Returns straight RGBA.
pub fn blend_pixel(b: [f32; 4], mut s: [f32; 4], mode: Blend, coverage: f32) -> [f32; 4] {
    let ab = b[3].clamp(0., 1.);
    s[3] = (s[3] * coverage).clamp(0., 1.);
    let a = s[3] + ab * (1. - s[3]);
    if a <= 0. {
        return [0.; 4];
    }
    let bc = [b[0], b[1], b[2]];
    let sc = [s[0], s[1], s[2]];
    let mixed = match mode {
        Blend::Hue => set_lum(set_sat(sc, sat(bc)), lum(bc)),
        Blend::Saturation => set_lum(set_sat(bc, sat(sc)), lum(bc)),
        Blend::Color => set_lum(sc, lum(bc)),
        Blend::Luminosity => set_lum(bc, lum(sc)),
        _ => std::array::from_fn(|c| match mode {
            Blend::Normal => s[c],
            Blend::Multiply => b[c] * s[c],
            Blend::Screen => b[c] + s[c] - b[c] * s[c],
            Blend::Overlay => hard_light(s[c], b[c]),
            Blend::HardLight => hard_light(b[c], s[c]),
            Blend::Darken => b[c].min(s[c]),
            Blend::Lighten => b[c].max(s[c]),
            Blend::ColorDodge => {
                if b[c] <= 0. {
                    0.
                } else if s[c] >= 1. {
                    1.
                } else {
                    (b[c] / (1. - s[c])).min(1.)
                }
            }
            Blend::ColorBurn => {
                if b[c] >= 1. {
                    1.
                } else if s[c] <= 0. {
                    0.
                } else {
                    1. - ((1. - b[c]) / s[c]).min(1.)
                }
            }
            Blend::SoftLight => {
                if s[c] <= 0.5 {
                    b[c] - (1. - 2. * s[c]) * b[c] * (1. - b[c])
                } else {
                    let d = if b[c] <= 0.25 {
                        ((16. * b[c] - 12.) * b[c] + 4.) * b[c]
                    } else {
                        b[c].sqrt()
                    };
                    b[c] + (2. * s[c] - 1.) * (d - b[c])
                }
            }
            Blend::Difference => (b[c] - s[c]).abs(),
            Blend::Exclusion => b[c] + s[c] - 2. * b[c] * s[c],
            _ => s[c],
        }),
    };
    let mut out = [0., 0., 0., a];
    for c in 0..3 {
        out[c] = (s[3] * (1. - ab) * s[c] + s[3] * ab * mixed[c] + (1. - s[3]) * ab * b[c]) / a;
    }
    out
}
fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        2. * b * s
    } else {
        1. - 2. * (1. - b) * (1. - s)
    }
}
fn lum(c: [f32; 3]) -> f32 {
    c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11
}
fn sat(c: [f32; 3]) -> f32 {
    c.into_iter().fold(f32::NEG_INFINITY, f32::max) - c.into_iter().fold(f32::INFINITY, f32::min)
}
fn set_lum(mut c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    for v in &mut c {
        *v += d;
    }
    let n = c.into_iter().fold(f32::INFINITY, f32::min);
    let x = c.into_iter().fold(f32::NEG_INFINITY, f32::max);
    if n < 0. {
        for v in &mut c {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1. {
        for v in &mut c {
            *v = l + (*v - l) * (1. - l) / (x - l);
        }
    }
    c
}
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let lo = c.into_iter().fold(f32::INFINITY, f32::min);
    let hi = c.into_iter().fold(f32::NEG_INFINITY, f32::max);
    c.map(|v| {
        if hi > lo {
            (v - lo) * s / (hi - lo)
        } else {
            0.
        }
    })
}

/// Render document pixels without an integer color intermediate.
/// Args: document and integer export scale. Returns an authoritative float RGBA buffer.
pub fn composite(doc: &Document, scale: u32) -> Result<Pixels, String> {
    composite_with_cancel(doc, scale, &super::storage::check)
}

/// Render native samples with cancellation before publication.
/// Args: document, export scale and job cancellation. Returns admitted authoritative pixels or an atomic error.
pub fn composite_with_cancel(
    doc: &Document,
    scale: u32,
    cancel: &dyn crate::resources::CancellationCheck,
) -> Result<Pixels, String> {
    cancel.check()?;
    doc.validate_hierarchy()?;
    crate::motion::require_flat_artwork(doc, "Native float compositor")?;
    let factor = scale.clamp(1, 8) as f32;
    composite_ready(
        doc,
        crate::export::RenderSize {
            factor,
            width: (doc.width * factor).round() as u32,
            height: (doc.height * factor).round() as u32,
        },
        cancel,
    )
}

/// Render authoritative samples at a checked fractional output scale.
/// Args: document, positive scale and cancellation. Returns the same admitted float compositor's output.
pub fn composite_scaled_with_cancel(
    doc: &Document,
    scale: f64,
    cancel: &dyn crate::resources::CancellationCheck,
) -> Result<Pixels, String> {
    cancel.check()?;
    doc.validate_hierarchy()?;
    crate::motion::require_flat_artwork(doc, "Native float compositor")?;
    composite_ready(doc, crate::export::RenderSize::new(doc, scale)?, cancel)
}

fn composite_ready(
    doc: &Document,
    size: crate::export::RenderSize,
    cancel: &dyn crate::resources::CancellationCheck,
) -> Result<Pixels, String> {
    if doc.color.model == super::cms::Model::Cmyk
        && doc
            .layers
            .iter()
            .filter_map(|l| l.kind.shapes())
            .flatten()
            .any(|s| s.mask.is_some() || s.layout.frame || s.layout.parent.is_some())
    {
        return Err("CMYK vector clips and shape masks require an ink-aware float coverage compositor; save .oma to preserve their original samples.".into());
    }
    if doc.color.model == super::cms::Model::Cmyk {
        return composite_cmyk(doc, size, cancel);
    }
    let (scale, w, h) = (size.factor, size.width, size.height);
    let count = super::storage::count(w, h, 4)?;
    let mut out = crate::resources::global().allocate_now::<f32, _>(count, cancel)?;
    if !doc.transparent {
        out.as_mut_slice().fill(1.);
    }
    render(
        doc,
        None,
        out.as_mut_slice().as_chunks_mut::<4>().0,
        w,
        h,
        scale,
        0,
        cancel,
    )?;
    let mut pixels =
        Pixels::from_sample_allocation_checked(w, h, Depth::F32, std::sync::Arc::new(out), cancel)?;
    if !doc.color.icc_profile.is_empty() {
        pixels.assign_rgb_profile(doc.color.icc_profile.clone(), doc.color.intent)?;
    }
    cancel.check()?;
    Ok(pixels)
}
fn render(
    doc: &Document,
    parent: Option<u64>,
    out: &mut [[f32; 4]],
    w: u32,
    h: u32,
    scale: f32,
    depth: usize,
    cancel: &dyn crate::resources::CancellationCheck,
) -> Result<(), String> {
    cancel.check()?;
    if depth >= 64 {
        return Err("Native composite group depth exceeds limit".into());
    }
    let siblings: Vec<_> = doc
        .layers
        .iter()
        .enumerate()
        .filter(|(_, l)| l.parent == parent)
        .map(|(i, _)| i)
        .collect();
    let mut start = 0;
    while start < siblings.len() {
        let end = crate::layer_clipping::stack_end(doc, &siblings, start);
        let layer = &doc.layers[siblings[start]];
        if layer.visible {
            if end > start + 1 {
                render_clipped(doc, &siblings[start..end], out, w, h, scale, depth, cancel)?;
            } else {
                render_layer(doc, layer, out, w, h, scale, depth, cancel)?;
            }
        }
        start = end;
    }
    Ok(())
}

fn render_layer(
    doc: &Document,
    layer: &Layer,
    out: &mut [[f32; 4]],
    w: u32,
    h: u32,
    scale: f32,
    depth: usize,
    cancel: &dyn crate::resources::CancellationCheck,
) -> Result<(), String> {
    cancel.check()?;
    if layer.opacity <= 0. {
        return Ok(());
    }
    if needs_rgba8_engine(layer) {
        return render_layer_rgba8(doc, layer, out, w, h, scale, cancel);
    }
    if layer.is_group {
        let pass = layer.pass_through && !layer.filters.active();
        let mut temp = if pass {
            crate::resources::global().copy_now(out, cancel)?
        } else {
            crate::resources::global().allocate_now::<[f32; 4], _>(out.len(), cancel)?
        };
        render(
            doc,
            Some(layer.id),
            temp.as_mut_slice(),
            w,
            h,
            scale,
            depth + 1,
            cancel,
        )?;
        if layer.filters.active() {
            let coverage = |i: usize| mask(layer, grid_point(i, w, scale, 0.), None);
            return blend_effected(doc, layer, temp.as_mut_slice(), out, w, h, scale, &coverage, cancel);
        }
        for (i, (dst, src)) in out.iter_mut().zip(temp.iter().copied()).enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            let p = Pt::new(
                (i % w as usize) as f32 / scale as f32,
                (i / w as usize) as f32 / scale as f32,
            );
            let amount = layer.opacity * layer.fill_opacity * mask(layer, p, None);
            if pass {
                let a = dst[3] * (1. - amount) + src[3] * amount;
                for c in 0..3 {
                    dst[c] = if a > 0. {
                        (dst[c] * dst[3] * (1. - amount) + src[c] * src[3] * amount) / a
                    } else {
                        0.
                    };
                }
                dst[3] = a;
            } else {
                *dst = blend_pixel(*dst, src, layer.blend, amount);
            }
        }
    } else if let Some(source) = layer.kind.pixels() {
        let pixels = source.in_document_color(&doc.color)?;
        let Some(inv) = crate::compositor::layer_pixel_transform(layer).invert() else {
            return Ok(());
        };
        if layer.filters.active() {
            let mut content =
                crate::resources::global().allocate_now::<[f32; 4], _>(out.len(), cancel)?;
            let mut cover = crate::resources::global().allocate_now::<f32, _>(out.len(), cancel)?;
            for (i, (src, m)) in content
                .as_mut_slice()
                .iter_mut()
                .zip(cover.as_mut_slice())
                .enumerate()
            {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                let p = grid_point(i, w, scale, 0.5);
                let mut local = tiny_skia::Point::from_xy(p.x, p.y);
                inv.map_point(&mut local);
                *src = sample(&pixels, local.x, local.y);
                *m = mask(layer, p, Some((local.x, local.y)));
            }
            drop(pixels);
            let coverage = |i: usize| cover[i];
            return blend_effected(doc, layer, content.as_mut_slice(), out, w, h, scale, &coverage, cancel);
        }
        for (i, dst) in out.iter_mut().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            let p = Pt::new(
                (i % w as usize) as f32 / scale as f32 + 0.5 / scale as f32,
                (i / w as usize) as f32 / scale as f32 + 0.5 / scale as f32,
            );
            let mut local = tiny_skia::Point::from_xy(p.x, p.y);
            inv.map_point(&mut local);
            let src = sample(&pixels, local.x, local.y);
            *dst = blend_pixel(
                *dst,
                src,
                layer.blend,
                layer.opacity * layer.fill_opacity * mask(layer, p, Some((local.x, local.y))),
            );
        }
    } else {
        vector::render(doc, layer, out, w, h, scale, cancel)?;
    }
    Ok(())
}

fn render_clipped(
    doc: &Document,
    stack: &[usize],
    out: &mut [[f32; 4]],
    w: u32,
    h: u32,
    scale: f32,
    depth: usize,
    cancel: &dyn crate::resources::CancellationCheck,
) -> Result<(), String> {
    let base = &doc.layers[stack[0]];
    if base.opacity <= 0. {
        return Ok(());
    }
    let mut normalized = base.clone();
    // The base's effects apply once to the clipped stack, not to its unclipped content.
    normalized.filters = Default::default();
    normalized.opacity = 1.;
    normalized.fill_opacity = 1.;
    normalized.blend = Blend::Normal;
    normalized.pass_through = false;
    let mut pixels = crate::resources::global().allocate_now::<[f32; 4], _>(out.len(), cancel)?;
    render_layer(
        doc,
        &normalized,
        pixels.as_mut_slice(),
        w,
        h,
        scale,
        depth,
        cancel,
    )?;
    let original = if base.pass_through && base.is_group && !base.filters.active() {
        Some(crate::resources::global().copy_now(pixels.as_slice(), cancel)?)
    } else {
        None
    };
    let mut source = crate::resources::global().allocate_now::<[f32; 4], _>(out.len(), cancel)?;
    for &index in &stack[1..] {
        cancel.check()?;
        let layer = &doc.layers[index];
        if !layer.visible {
            continue;
        }
        source.as_mut_slice().fill([0.; 4]);
        let mut normalized = layer.clone();
        normalized.blend = Blend::Normal;
        normalized.pass_through = false;
        render_layer(
            doc,
            &normalized,
            source.as_mut_slice(),
            w,
            h,
            scale,
            depth,
            cancel,
        )?;
        for (i, (pixel, src)) in pixels
            .as_mut_slice()
            .iter_mut()
            .zip(source.iter().copied())
            .enumerate()
        {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            *pixel = crate::layer_clipping::atop(*pixel, src, layer.blend);
        }
    }
    let amount = base.opacity * base.fill_opacity;
    if let Some(original) = original {
        let before = crate::resources::global().copy_now(out, cancel)?;
        render_layer(doc, base, out, w, h, scale, depth, cancel)?;
        for (i, ((dst, src), plain)) in out
            .iter_mut()
            .zip(pixels.iter().copied())
            .zip(original.iter().copied())
            .enumerate()
        {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            let with = blend_pixel(before[i], src, Blend::Normal, amount);
            let without = blend_pixel(before[i], plain, Blend::Normal, amount);
            let alpha = (dst[3] + with[3] - without[3]).clamp(0., 1.);
            for c in 0..3 {
                dst[c] = if alpha > 0. {
                    (dst[c] * dst[3] + with[c] * with[3] - without[c] * without[3]) / alpha
                } else {
                    0.
                };
            }
            dst[3] = alpha;
        }
    } else if base.filters.active() {
        let coverage = |_: usize| 1.;
        blend_effected(doc, base, pixels.as_mut_slice(), out, w, h, scale, &coverage, cancel)?;
    } else {
        for (i, (dst, src)) in out.iter_mut().zip(pixels.iter().copied()).enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            *dst = blend_pixel(*dst, src, base.blend, amount);
        }
    }
    Ok(())
}
pub(super) fn sample(pixels: &Pixels, x: f32, y: f32) -> [f32; 4] {
    if x < 0. || y < 0. || x >= pixels.w as f32 || y >= pixels.h as f32 {
        return [0.; 4];
    }
    let x = x - 0.5;
    let y = y - 0.5;
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let mut out = [0.; 4];
    for (dx, wx) in [
        (0, 1. - x.fract().rem_euclid(1.)),
        (1, x.fract().rem_euclid(1.)),
    ] {
        for (dy, wy) in [
            (0, 1. - y.fract().rem_euclid(1.)),
            (1, y.fract().rem_euclid(1.)),
        ] {
            let px = (ix + dx).clamp(0, pixels.w as i32 - 1) as usize;
            let py = (iy + dy).clamp(0, pixels.h as i32 - 1) as usize;
            let p = pixels.sample(py * pixels.w as usize + px);
            let a = p[3].clamp(0., 1.);
            for c in 0..3 {
                out[c] += p[c] * a * wx * wy;
            }
            out[3] += a * wx * wy;
        }
    }
    if out[3] > 0. {
        for c in 0..3 {
            out[c] /= out[3];
        }
    }
    out
}
fn mask(layer: &Layer, p: Pt, local: Option<(f32, f32)>) -> f32 {
    let Some(mask) = &layer.mask else {
        return 1.;
    };
    let (x, y) = if let Some(local) = local {
        local
    } else if layer.mask_size.x > 0. && layer.mask_size.y > 0. {
        (
            (p.x - layer.mask_origin.x) * mask.w as f32 / layer.mask_size.x,
            (p.y - layer.mask_origin.y) * mask.h as f32 / layer.mask_size.y,
        )
    } else {
        (p.x, p.y)
    };
    let m = sample(mask, x, y);
    ((m[0] * 0.2126 + m[1] * 0.7152 + m[2] * 0.0722) * m[3]).clamp(0., 1.)
}

fn composite_cmyk(
    doc: &Document,
    size: crate::export::RenderSize,
    cancel: &dyn crate::resources::CancellationCheck,
) -> Result<Pixels, String> {
    cancel.check()?;
    if doc.layers.iter().any(|l| {
        matches!(
            l.blend,
            Blend::Hue | Blend::Saturation | Blend::Color | Blend::Luminosity
        )
    }) {
        let mut rgb = doc.clone();
        rgb.color = super::cms::DocumentColor {
            depth: Depth::F32,
            ..Default::default()
        };
        for layer in &mut rgb.layers {
            if let Some(p) = layer.kind.pixels_mut() {
                *p = p.in_document_color(&rgb.color)?;
            }
        }
        return composite_ready(&rgb, size, cancel)?.in_document_color(&doc.color);
    }
    let mut inks = None;
    let mut dimensions = (0, 0);
    for component in 0..4 {
        cancel.check()?;
        let mut plane = doc.clone();
        plane.color = Default::default();
        for layer in &mut plane.layers {
            separate_effect_colors(&mut layer.filters, &doc.color, component)?;
            if layer.is_group {
                continue;
            }
            let source = if let Some(p) = layer.kind.pixels() {
                p.clone()
            } else {
                let mut one = doc.clone();
                one.color = Default::default();
                let mut single = layer.clone();
                single.parent = None;
                single.blend = Blend::Normal;
                single.opacity = 1.;
                one.layers = vec![single];
                one.transparent = true;
                let p = composite_with_cancel(&one, 1, cancel)?;
                layer.kind = crate::document::LayerKind::Raster {
                    pixels: p.clone(),
                    origin: Pt::ZERO,
                    size: Pt::new(doc.width, doc.height),
                    rotation: 0.,
                    shear: 0.,
                };
                layer.mask = None;
                layer.filters = Default::default();
                p
            };
            let source = source.in_document_color(&doc.color)?;
            let native = source.native.as_ref().ok_or("Missing CMYK samples")?;
            let authored = native.cmyk.as_ref().ok_or("Missing CMYK inks")?;
            let values = super::storage::collect(authored.chunks_exact(5).flat_map(|p| {
                let v = 1. - p[component];
                [v, v, v, p[4]]
            }))?;
            *layer.kind.pixels_mut().unwrap() =
                Pixels::from_sample_allocation(source.w, source.h, Depth::F32, values)?;
        }
        let result = composite_ready(&plane, size, cancel)?;
        dimensions = (result.w, result.h);
        if inks.is_none() {
            inks =
                Some(crate::resources::global().allocate_now::<f32, _>(
                    super::storage::count(result.w, result.h, 5)?,
                    cancel,
                )?);
        }
        for (i, p) in inks
            .as_mut()
            .unwrap()
            .as_mut_slice()
            .chunks_exact_mut(5)
            .enumerate()
        {
            let value = result.sample(i);
            p[component] = 1. - value[0];
            p[4] = value[3];
        }
    }
    Pixels::from_cmyk_allocation(
        dimensions.0,
        dimensions.1,
        doc.color.depth,
        std::sync::Arc::new(inks.ok_or("Missing composite ink planes")?),
        doc.color.icc_profile.clone(),
        doc.color.intent,
    )
}

/// Map an output index to document coordinates.
fn grid_point(i: usize, w: u32, scale: f32, offset: f32) -> Pt {
    Pt::new(
        ((i % w as usize) as f32 + offset) / scale,
        ((i / w as usize) as f32 + offset) / scale,
    )
}

/// Report layer features only the RGBA8 engine draws.
/// Args: layer. Returns true for SVG filter graphs, live artwork masks and filtered vector content.
fn needs_rgba8_engine(layer: &Layer) -> bool {
    layer.svg_filter.as_ref().is_some_and(|graph| !graph.filters.is_empty())
        || !layer.artwork_masks.is_empty()
        || !layer.is_group && layer.kind.shapes().is_some_and(|shapes| {
            layer.filters.active()
                || shapes.iter().any(|s| {
                    s.filters.active()
                        || matches!(s.style.fill, crate::document::Fill::Pattern(_))
                        || !s.artwork_masks.is_empty()
                        || s.layout.image.is_some()
                })
        })
}

/// Composite float content with its layer effects in document color.
/// Content keeps its samples; appearance effects are built from its alpha and only pixel
/// filters (blur, color adjustments, legacy stacks) run through the RGBA8 engine.
/// Args: document, effected layer, straight content grid, destination, extent, scale, per-pixel mask and cancellation.
/// Returns after blending outer effects, content and inner effects into `out`.
#[allow(clippy::too_many_arguments)]
fn blend_effected(
    doc: &Document,
    layer: &Layer,
    content: &mut [[f32; 4]],
    out: &mut [[f32; 4]],
    w: u32,
    h: u32,
    scale: f32,
    coverage: &dyn Fn(usize) -> f32,
    cancel: &dyn crate::resources::CancellationCheck,
) -> Result<(), String> {
    let mut stack = layer.filters.clone();
    crate::filter::scale(&mut stack, scale);
    let independent = stack.independent();
    let pixel_filters: Vec<_> = stack
        .items
        .iter()
        .filter(|fx| !independent || fx.appearance().is_none())
        .copied()
        .collect();
    if !pixel_filters.is_empty() {
        let mut pm = to_pixmap(content, w, h, |p| p)?;
        for fx in &pixel_filters {
            crate::filter::apply_one(&mut pm, fx);
        }
        cancel.check()?;
        for (dst, p) in content.iter_mut().zip(pm.pixels()) {
            let p = p.demultiply();
            *dst = [p.red(), p.green(), p.blue(), p.alpha()].map(|c| c as f32 / 255.);
        }
    }
    let mut outer = Vec::new();
    let mut inner = Vec::new();
    if independent {
        let alpha = to_pixmap(content, w, h, |p| [0., 0., 0., p[3]])?;
        let color = super::cms::DocumentColor {
            depth: Depth::F32,
            ..doc.color.clone()
        };
        for fx in stack.items.iter() {
            let Some((blend, opacity)) = fx.appearance() else {
                continue;
            };
            let Some(effect) = crate::filter::effect_pixels(&alpha, fx) else {
                continue;
            };
            cancel.check()?;
            let bytes = effect
                .pixels()
                .iter()
                .flat_map(|p| {
                    let p = p.demultiply();
                    [p.red(), p.green(), p.blue(), p.alpha()]
                })
                .collect();
            // Effect colors are sRGB; convert them into the document's working space.
            let plane = Pixels::from_rgba(w, h, bytes)
                .ok_or("Invalid effect plane")?
                .in_document_color(&color)?
                .samples()?;
            if fx.outer() {
                outer.push((plane, blend, opacity));
            } else {
                inner.push((plane, blend, opacity));
            }
        }
    }
    let sample = |plane: &[f32], i: usize| [plane[i * 4], plane[i * 4 + 1], plane[i * 4 + 2], plane[i * 4 + 3]];
    for (i, (dst, src)) in out.iter_mut().zip(content.iter().copied()).enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        let amount = layer.opacity * coverage(i);
        for (plane, blend, opacity) in &outer {
            *dst = blend_pixel(*dst, sample(plane, i), *blend, amount * opacity);
        }
        *dst = blend_pixel(*dst, src, layer.blend, amount * layer.fill_opacity);
        for (plane, blend, opacity) in &inner {
            *dst = blend_pixel(*dst, sample(plane, i), *blend, amount * opacity);
        }
    }
    Ok(())
}

/// Quantize straight float samples into a premultiplied RGBA8 working pixmap.
fn to_pixmap(
    content: &[[f32; 4]],
    w: u32,
    h: u32,
    pick: impl Fn([f32; 4]) -> [f32; 4],
) -> Result<tiny_skia::Pixmap, String> {
    let mut pm = tiny_skia::Pixmap::new(w, h).ok_or("Invalid effect working size")?;
    for (dst, src) in pm.data_mut().chunks_exact_mut(4).zip(content) {
        let p = pick(*src).map(|c| c.clamp(0., 1.));
        for c in 0..3 {
            dst[c] = (p[c] * p[3] * 255.).round() as u8;
        }
        dst[3] = (p[3] * 255.).round() as u8;
    }
    Ok(pm)
}

/// Draw a layer the float compositor cannot express through the RGBA8 engine.
/// Used for vector or SVG-filtered content whose authored colors are already 8-bit sRGB.
/// Args: source document, layer and output grid. Returns blended samples; only temporary copies change.
fn render_layer_rgba8(
    doc: &Document,
    layer: &Layer,
    out: &mut [[f32; 4]],
    w: u32,
    h: u32,
    scale: f32,
    cancel: &dyn crate::resources::CancellationCheck,
) -> Result<(), String> {
    let mut stage = doc.clone();
    let mut ids = vec![layer.id];
    stage.layers = Vec::new();
    for candidate in &doc.layers {
        if candidate.id == layer.id
            || candidate.parent.is_some_and(|parent| ids.contains(&parent))
        {
            ids.push(candidate.id);
            let mut copy = candidate.clone();
            if copy.id == layer.id {
                copy.parent = None;
                copy.clipped = false;
                copy.blend = Blend::Normal;
            }
            super::strip_native(&mut copy);
            stage.layers.push(copy);
        }
    }
    stage.color = Default::default();
    stage.color_tag = None;
    stage.transparent = true;
    stage.channels.clear();
    stage.motion = Default::default();
    let size = crate::export::RenderSize {
        factor: scale,
        width: w,
        height: h,
    };
    let pm = crate::compositor::render_export_sized(&stage, size)?;
    cancel.check()?;
    let bytes = pm
        .pixels()
        .iter()
        .flat_map(|p| {
            let p = p.demultiply();
            [p.red(), p.green(), p.blue(), p.alpha()]
        })
        .collect();
    let color = super::cms::DocumentColor {
        depth: Depth::F32,
        ..doc.color.clone()
    };
    let rendered = Pixels::from_rgba(w, h, bytes)
        .ok_or("Invalid layer raster")?
        .in_document_color(&color)?
        .samples()?;
    for (i, (dst, src)) in out.iter_mut().zip(rendered.chunks_exact(4)).enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        *dst = blend_pixel(*dst, [src[0], src[1], src[2], src[3]], layer.blend, 1.);
    }
    Ok(())
}

/// Express appearance-effect colors as one ink plane's gray value.
/// Args: a plane's temporary filter copy, CMYK document color and ink index. Returns an error for an invalid profile.
fn separate_effect_colors(
    stack: &mut crate::filter::FilterStack,
    color: &super::cms::DocumentColor,
    component: usize,
) -> Result<(), String> {
    use crate::filter::Fx;
    for fx in &mut stack.items {
        let (Fx::Shadow { color: c, .. }
        | Fx::InnerShadow { color: c, .. }
        | Fx::OuterGlow { color: c, .. }
        | Fx::InnerGlow { color: c, .. }
        | Fx::ColorOverlay { color: c, .. }) = fx
        else {
            continue;
        };
        let rgb = [c.r, c.g, c.b].map(|v| v as f32 / 255.);
        let inks = super::cms::transform(&rgb, super::cms::Model::Cmyk, &color.icc_profile, false, color.intent)?;
        let v = ((1. - inks[component]).clamp(0., 1.) * 255.).round() as u8;
        *c = crate::color::Rgba::new(v, v, v, c.a);
    }
    Ok(())
}
