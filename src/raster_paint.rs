//! Bounded brush working tiles and pixel-patch history for native raster layers.
use crate::document::{Layer, LayerKind, Pixels};
use crate::geom::{Bounds, Pt};
use crate::paint::Brush;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};

pub const TILE: i32 = 256;
const MAX_WORKING_PIXELS: usize = 64_000_000;
const MAX_LAYER_PIXELS: u64 = crate::upscale::MAX_PIXELS;

#[derive(Clone, Debug, Default)]
pub struct BrushBuffer {
    pub tiles: BTreeMap<(i32, i32), Pixmap>,
    pub dirty: BTreeSet<(i32, i32)>,
    pub base: Option<Bounds>,
    pub span: Option<Bounds>,
    cache: Vec<CachedDab>,
}

#[derive(Clone, Debug)]
struct CachedDab {
    brush: Brush,
    fraction: Pt,
    tip: Option<usize>,
    pixels: Arc<Pixmap>,
}

impl BrushBuffer {
    /// Check a segment before allocating working tiles.
    /// Takes native endpoints and settings; returns whether the eventual image fits its memory budget.
    pub fn accepts(&self, from: Pt, to: Pt, brush: &Brush) -> bool {
        if ![from.x, from.y, to.x, to.y]
            .iter()
            .all(|v| v.is_finite() && v.abs() < 1_000_000_000.)
        {
            return false;
        }
        let radius = brush.stroke_radius();
        let mut bounds = Bounds {
            min: from.min(to),
            max: from.max(to),
        }
        .inflate(radius);
        if let Some(base) = self.base {
            bounds = bounds.union(base);
        }
        if let Some(span) = self.span {
            bounds = bounds.union(span);
        }
        f64::from(bounds.width().ceil()) * f64::from(bounds.height().ceil())
            <= MAX_LAYER_PIXELS as f64
    }

    /// Paint one varied dab without allocating the empty canvas around it.
    /// Takes a native center, brush settings and optional sampled tip; returns allocation/range errors.
    pub fn stamp(
        &mut self,
        point: Pt,
        brush: &Brush,
        tip: Option<&crate::brushes::Tip>,
    ) -> Result<(), &'static str> {
        let (point, brush) = brush.dab(point);
        let radius = (brush.size * std::f32::consts::FRAC_1_SQRT_2)
            .max(1.)
            .ceil()
            + 2.;
        if ![point.x, point.y, radius]
            .iter()
            .all(|v| v.is_finite() && v.abs() < 1_000_000_000.)
        {
            return Err("Brush position is outside the supported pixel range");
        }
        let padding = radius as i32;
        let side = (padding * 2 + 1) as u32;
        if u64::from(side) * u64::from(side) > MAX_WORKING_PIXELS as u64 {
            return Err("Brush tip is too large to paint at this resolution");
        }
        let anchor = Pt::new(point.x.floor(), point.y.floor());
        let fraction = point - anchor;
        let tip_key = tip.map(|tip| tip as *const crate::brushes::Tip as usize);
        let dab = if let Some(index) = self
            .cache
            .iter()
            .position(|c| c.brush == brush && c.fraction == fraction && c.tip == tip_key)
        {
            let cached = self.cache.remove(index);
            let pixels = cached.pixels.clone();
            self.cache.push(cached);
            pixels
        } else {
            let mut pixels =
                Pixmap::new(side, side).ok_or("Not enough memory for this brush tip")?;
            let local = Pt::new(padding as f32, padding as f32) + fraction;
            if let Some(tip) = tip {
                crate::brushes::stamp_dab(&mut pixels, local, &brush, tip, false);
            } else {
                crate::paint::stamp_dab(&mut pixels, local, &brush, false);
            }
            let pixels = Arc::new(pixels);
            let bytes = pixels.data().len();
            if bytes <= 16 * 1024 * 1024 {
                while !self.cache.is_empty()
                    && self
                        .cache
                        .iter()
                        .map(|c| c.pixels.data().len())
                        .sum::<usize>()
                        + bytes
                        > 16 * 1024 * 1024
                {
                    self.cache.remove(0);
                }
                self.cache.push(CachedDab {
                    brush,
                    fraction,
                    tip: tip_key,
                    pixels: pixels.clone(),
                });
            }
            pixels
        };
        let x0 = anchor.x as i32 - padding;
        let y0 = anchor.y as i32 - padding;
        let extent = Bounds::from_center_size(point, Pt::new(radius * 2., radius * 2.));
        self.span = Some(self.span.map_or(extent, |old| old.union(extent)));
        let left = ((point.x - radius).floor() as i32).div_euclid(TILE);
        let top = ((point.y - radius).floor() as i32).div_euclid(TILE);
        let right = ((point.x + radius).ceil() as i32).div_euclid(TILE);
        let bottom = ((point.y + radius).ceil() as i32).div_euclid(TILE);
        if (right as i64 - left as i64 + 1) * (bottom as i64 - top as i64 + 1)
            > (MAX_WORKING_PIXELS / (TILE * TILE) as usize) as i64
        {
            return Err("Brush tip is too large to paint at this resolution");
        }
        for y in top..=bottom {
            for x in left..=right {
                if !self.tiles.contains_key(&(x, y)) {
                    if (self.tiles.len() + 1) * (TILE * TILE) as usize > MAX_WORKING_PIXELS {
                        return Err("Finish this stroke before adding more paint");
                    }
                    self.tiles.insert(
                        (x, y),
                        Pixmap::new(TILE as u32, TILE as u32)
                            .ok_or("Not enough memory for this stroke")?,
                    );
                }
                let pm = self.tiles.get_mut(&(x, y)).unwrap();
                pm.draw_pixmap(
                    x0 - x * TILE,
                    y0 - y * TILE,
                    dab.as_ref().as_ref(),
                    &Default::default(),
                    Transform::identity(),
                    None,
                );
                self.dirty.insert((x, y));
            }
        }
        Ok(())
    }

    /// Find the actual ink rectangle, excluding empty tile borders.
    /// Takes this stroke; returns native pixel bounds, or None for an empty stroke.
    pub fn bounds(&self) -> Option<Bounds> {
        let mut result: Option<Bounds> = None;
        for (&(x, y), pm) in &self.tiles {
            for (row, pixels) in pm.pixels().chunks(TILE as usize).enumerate() {
                let Some(left) = pixels.iter().position(|p| p.alpha() > 0) else {
                    continue;
                };
                let right = pixels.iter().rposition(|p| p.alpha() > 0).unwrap() + 1;
                let line = Bounds::from_min_size(
                    Pt::new(
                        (x * TILE) as f32 + left as f32,
                        (y * TILE) as f32 + row as f32,
                    ),
                    Pt::new((right - left) as f32, 1.),
                );
                result = Some(result.map_or(line, |old| old.union(line)));
            }
        }
        result
    }

    #[cfg(test)]
    pub fn data(&self) -> Vec<u8> {
        self.tiles
            .values()
            .flat_map(|p| p.data().iter().copied())
            .collect()
    }
    #[cfg(test)]
    pub fn pixels(&self) -> Vec<tiny_skia::PremultipliedColorU8> {
        self.tiles
            .values()
            .flat_map(|p| p.pixels().iter().copied())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterGrid {
    pub w: u32,
    pub h: u32,
    pub origin: Pt,
    pub size: Pt,
    pub rotation: f32,
    pub shear: f32,
    pub mask: Option<(u32, u32)>,
}

impl RasterGrid {
    /// Capture grid placement without copying image bytes.
    /// Takes a layer; returns its geometry, or None for a vector layer.
    pub fn capture(layer: &Layer) -> Option<Self> {
        let LayerKind::Raster {
            pixels,
            origin,
            size,
            rotation,
            shear,
        } = &layer.kind
        else {
            return None;
        };
        Some(Self {
            w: pixels.w,
            h: pixels.h,
            origin: *origin,
            size: *size,
            rotation: *rotation,
            shear: *shear,
            mask: layer.mask.as_ref().map(|m| (m.w, m.h)),
        })
    }

    fn dimensions(self) -> Pt {
        if self.size.x.abs() > 0.5 && self.size.y.abs() > 0.5 {
            self.size
        } else {
            Pt::new(self.w as f32, self.h as f32)
        }
    }

    /// Map native pixels to the document, retaining the editor's rotation and shear order.
    /// Takes this grid; returns its affine transform.
    pub fn transform(self) -> Transform {
        let dimensions = self.dimensions();
        let mut transform = Transform::from_translate(self.origin.x, self.origin.y).pre_scale(
            dimensions.x / self.w.max(1) as f32,
            dimensions.y / self.h.max(1) as f32,
        );
        if self.rotation.abs() > 1e-5 {
            let centre = self.origin + dimensions * 0.5;
            transform = Transform::from_translate(centre.x, centre.y)
                .pre_concat(Transform::from_rotate(self.rotation.to_degrees()))
                .pre_concat(Transform::from_translate(-centre.x, -centre.y))
                .pre_concat(transform);
        }
        if self.shear.abs() > 1e-4 {
            let slope = self.shear / dimensions.y.max(1.);
            transform = Transform::from_row(1., 0., slope, 1., -slope * self.origin.y, 0.)
                .pre_concat(transform);
        }
        transform
    }

    fn expanded(self, left: i32, top: i32, w: u32, h: u32) -> Self {
        let dimensions = self.dimensions();
        let size = Pt::new(
            dimensions.x * w as f32 / self.w as f32,
            dimensions.y * h as f32 / self.h as f32,
        );
        let mut next = Self {
            w,
            h,
            size,
            shear: self.shear / dimensions.y.max(1.) * size.y.max(1.),
            mask: self.mask.map(|_| (w, h)),
            ..self
        };
        let mut old = tiny_skia::Point::from_xy(left as f32, top as f32);
        self.transform().map_point(&mut old);
        let mut new = tiny_skia::Point::from_xy(0., 0.);
        next.transform().map_point(&mut new);
        next.origin = next.origin + Pt::new(old.x - new.x, old.y - new.y);
        next
    }
}

/// Copy nonzero pixel blocks into transparent padding without populating empty pages.
/// Takes bytes, old/new dimensions and an integer destination offset; returns the new bytes.
pub(crate) fn reframe(
    data: &[u8],
    old: (u32, u32),
    new: (u32, u32),
    offset: (i32, i32),
) -> Option<Vec<u8>> {
    if data.len()
        != (old.0 as usize)
            .checked_mul(old.1 as usize)?
            .checked_mul(4)?
    {
        return None;
    }
    let len = (new.0 as usize)
        .checked_mul(new.1 as usize)?
        .checked_mul(4)?;
    let mut output = vec![0; len];
    let left = offset.0.max(0) as i64;
    let right = (i64::from(offset.0) + i64::from(old.0)).min(i64::from(new.0));
    if left >= right {
        return Some(output);
    }
    for y in
        i64::from(offset.1).max(0)..(i64::from(offset.1) + i64::from(old.1)).min(i64::from(new.1))
    {
        let source = ((y - i64::from(offset.1)) as usize * old.0 as usize
            + (left - i64::from(offset.0)) as usize)
            * 4;
        let target = (y as usize * new.0 as usize + left as usize) * 4;
        let count = (right - left) as usize * 4;
        for (i, block) in data[source..source + count].chunks(4096).enumerate() {
            if block.iter().any(|b| *b != 0) {
                let start = target + i * 4096;
                output[start..start + block.len()].copy_from_slice(block);
            }
        }
    }
    Some(output)
}

fn resize(layer: &mut Layer, grid: RasterGrid, offset: (i32, i32)) -> Result<(), &'static str> {
    let old = RasterGrid::capture(layer).ok_or("Choose a pixel layer to paint")?;
    if old == grid && offset == (0, 0) {
        return Ok(());
    }
    let LayerKind::Raster {
        pixels,
        origin,
        size,
        rotation,
        shear,
    } = &mut layer.kind
    else {
        unreachable!()
    };
    let data = reframe(&pixels.data, (old.w, old.h), (grid.w, grid.h), offset)
        .ok_or("Could not resize the pixel layer")?;
    let cached = pixels.cached_pm.get_mut().take().and_then(|(_, pm)| {
        let data = reframe(pm.data(), (old.w, old.h), (grid.w, grid.h), offset)?;
        Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(grid.w, grid.h)?)
    });
    let version = pixels.version.wrapping_add(1);
    *pixels = Pixels::from_rgba(grid.w, grid.h, data).ok_or("Could not resize the pixel layer")?;
    pixels.version = version;
    *pixels.cached_pm.get_mut() = cached.map(|pm| (version, pm));
    *origin = grid.origin;
    *size = grid.size;
    *rotation = grid.rotation;
    *shear = grid.shear;
    if let Some(mask) = &mut layer.mask
        && let Some((w, h)) = grid.mask
    {
        *mask = Pixels::from_rgba(
            w,
            h,
            reframe(&mask.data, (mask.w, mask.h), (w, h), offset)
                .ok_or("Could not resize the layer mask")?,
        )
        .ok_or("Could not resize the layer mask")?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct PixelPatch {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub before: Arc<[u8]>,
    pub after: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct RasterChange {
    pub before: RasterGrid,
    pub after: RasterGrid,
    pub offset: (i32, i32),
    pub patches: Vec<PixelPatch>,
}

impl RasterChange {
    /// Reverse a stroke's patches and grid translation for Undo.
    /// Takes the change; returns its inverse without copying pixel payloads.
    pub fn inverted(self) -> Self {
        let offset = (-self.offset.0, -self.offset.1);
        Self {
            before: self.after,
            after: self.before,
            offset,
            patches: self
                .patches
                .into_iter()
                .map(|p| PixelPatch {
                    x: p.x + offset.0,
                    y: p.y + offset.1,
                    before: p.after,
                    after: p.before,
                    ..p
                })
                .collect(),
        }
    }

    /// Apply one stroke or its inverse to the destination layer.
    /// Takes a layer; returns an error if its grid cannot be restored.
    pub fn apply(&self, layer: &mut Layer) -> Result<(), &'static str> {
        resize(layer, self.after, self.offset)?;
        let pixels = layer
            .kind
            .pixels_mut()
            .ok_or("Choose a pixel layer to paint")?;
        for patch in &self.patches {
            write_patch(pixels, patch);
        }
        pixels.version = pixels.version.wrapping_add(1);
        pixels.cached_uniform.get_mut().take();
        if let Some((version, _)) = pixels.cached_pm.get_mut() {
            *version = pixels.version;
        }
        Ok(())
    }
}

fn write_patch(pixels: &mut Pixels, patch: &PixelPatch) {
    for y in 0..patch.h as i32 {
        let dy = patch.y + y;
        if dy < 0 || dy >= pixels.h as i32 {
            continue;
        }
        let left = patch.x.max(0);
        let right = (patch.x + patch.w as i32).min(pixels.w as i32);
        if left >= right {
            continue;
        }
        let source = (y as usize * patch.w as usize + (left - patch.x) as usize) * 4;
        let target = (dy as usize * pixels.w as usize + left as usize) * 4;
        let count = (right - left) as usize * 4;
        if patch.after.is_empty() {
            pixels.data[target..target + count].fill(0);
            if let Some((_, pm)) = pixels.cached_pm.get_mut() {
                pm.data_mut()[target..target + count].fill(0);
            }
            continue;
        }
        pixels.data[target..target + count].copy_from_slice(&patch.after[source..source + count]);
        if let Some((_, pm)) = pixels.cached_pm.get_mut() {
            for (dst, src) in pm.pixels_mut()[target / 4..(target + count) / 4]
                .iter_mut()
                .zip(patch.after[source..source + count].chunks_exact(4))
            {
                *dst = tiny_skia::ColorU8::from_rgba(src[0], src[1], src[2], src[3]).premultiply();
            }
        }
    }
}

fn ink_bounds(pm: &Pixmap) -> Option<Bounds> {
    let mut bounds: Option<Bounds> = None;
    for (y, row) in pm.pixels().chunks(pm.width() as usize).enumerate() {
        let Some(left) = row.iter().position(|p| p.alpha() > 0) else {
            continue;
        };
        let right = row.iter().rposition(|p| p.alpha() > 0).unwrap() + 1;
        let line = Bounds::from_min_size(
            Pt::new(left as f32, y as f32),
            Pt::new((right - left) as f32, 1.),
        );
        bounds = Some(bounds.map_or(line, |old| old.union(line)));
    }
    bounds
}

fn patch_bytes(bytes: Vec<u8>) -> Arc<[u8]> {
    if bytes.iter().all(|v| *v == 0) {
        Arc::from([])
    } else {
        bytes.into()
    }
}

/// Commit only a stroke's touched tiles and retain small, shared Undo patches.
/// Takes a layer, overlay and opacity; returns its reversible edit or None when unchanged.
pub fn commit(
    layer: &mut Layer,
    buffer: BrushBuffer,
    opacity: f32,
) -> Result<Option<RasterChange>, &'static str> {
    if opacity <= 0. {
        return Ok(None);
    }
    let tiles: Vec<_> = buffer
        .tiles
        .into_iter()
        .filter_map(|(key, pm)| ink_bounds(&pm).map(|bounds| (key, pm, bounds)))
        .collect();
    let Some(bounds) = tiles
        .iter()
        .map(|((x, y), _, bounds)| {
            let offset = Pt::new((x * TILE) as f32, (y * TILE) as f32);
            Bounds {
                min: bounds.min + offset,
                max: bounds.max + offset,
            }
        })
        .reduce(|a, b| a.union(b))
    else {
        return Ok(None);
    };
    let before = RasterGrid::capture(layer).ok_or("Choose a pixel layer to paint")?;
    let outside = bounds.min.x < 0.
        || bounds.min.y < 0.
        || bounds.max.x > before.w as f32
        || bounds.max.y > before.h as f32;
    let blank = outside && layer.mask.is_none() && layer.kind.pixels().unwrap().is_invisible();
    let left = (bounds.min.x.floor() as i32).min(if blank { i32::MAX } else { 0 });
    let top = (bounds.min.y.floor() as i32).min(if blank { i32::MAX } else { 0 });
    let right = (bounds.max.x.ceil() as i64).max(if blank {
        i64::MIN
    } else {
        i64::from(before.w.max(if outside {
            before.mask.map_or(0, |m| m.0)
        } else {
            0
        }))
    });
    let bottom = (bounds.max.y.ceil() as i64).max(if blank {
        i64::MIN
    } else {
        i64::from(before.h.max(if outside {
            before.mask.map_or(0, |m| m.1)
        } else {
            0
        }))
    });
    let w = right - i64::from(left);
    let h = bottom - i64::from(top);
    if w <= 0 || h <= 0 || w as u64 * h as u64 > MAX_LAYER_PIXELS {
        return Err(
            "The painted span exceeds the image memory budget. Start another pixel layer here",
        );
    }
    let offset = (-left, -top);
    let after = if left == 0 && top == 0 && w == i64::from(before.w) && h == i64::from(before.h) {
        before
    } else {
        before.expanded(left, top, w as u32, h as u32)
    };
    resize(layer, after, offset)?;
    let pixels = layer.kind.pixels_mut().unwrap();
    let mut patches = vec![];
    for ((tx, ty), tile, ink) in tiles {
        let x = (tx * TILE + offset.0 + ink.min.x as i32).max(0);
        let y = (ty * TILE + offset.1 + ink.min.y as i32).max(0);
        let right = (tx * TILE + offset.0 + ink.max.x as i32).min(pixels.w as i32);
        let bottom = (ty * TILE + offset.1 + ink.max.y as i32).min(pixels.h as i32);
        if x >= right || y >= bottom {
            continue;
        }
        let (w, h) = ((right - x) as u32, (bottom - y) as u32);
        let mut original = vec![0; (w * h * 4) as usize];
        for row in 0..h as usize {
            let start = ((y as usize + row) * pixels.w as usize + x as usize) * 4;
            original[row * w as usize * 4..(row + 1) * w as usize * 4]
                .copy_from_slice(&pixels.data[start..start + w as usize * 4]);
        }
        let mut pm = crate::color::rgba_to_pixmap(w, h, &original)
            .ok_or("Could not commit this brush stroke")?;
        pm.draw_pixmap(
            tx * TILE + offset.0 - x,
            ty * TILE + offset.1 - y,
            tile.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity: opacity.clamp(0., 1.),
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
        let output = Pixels::from_pixmap(&pm).data;
        if original != output {
            let patch = PixelPatch {
                x,
                y,
                w,
                h,
                before: patch_bytes(original),
                after: patch_bytes(output),
            };
            write_patch(pixels, &patch);
            patches.push(patch);
        }
    }
    if patches.is_empty() {
        resize(layer, before, (-offset.0, -offset.1))?;
        return Ok(None);
    }
    pixels.version = pixels.version.wrapping_add(1);
    pixels.cached_uniform.get_mut().take();
    if let Some((version, _)) = pixels.cached_pm.get_mut() {
        *version = pixels.version;
    }
    Ok(Some(RasterChange {
        before,
        after,
        offset,
        patches,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiled_dabs_match_dense_paint_at_negative_coordinates_and_tile_edges() {
        let tip = crate::brushes::Tip {
            width: 47,
            height: 29,
            alpha: (0..47 * 29).map(|i| (i * 37) as u8).collect(),
        };
        for sampled in [false, true] {
            let brush = Brush {
                size: 75.5,
                angle: 0.7,
                scatter: 0.4,
                size_jitter: 0.2,
                angle_jitter: 0.3,
                opacity_jitter: 0.2,
                seed: 42,
                ..Default::default()
            };
            let mut buffer = BrushBuffer::default();
            let mut dense = Pixmap::new(768, 768).unwrap();
            for point in [
                Pt::new(-0.25, 0.5),
                Pt::new(255.5, -12.25),
                Pt::new(35.75, 255.25),
            ] {
                buffer
                    .stamp(point, &brush, sampled.then_some(&tip))
                    .unwrap();
                let (point, brush) = brush.dab(point);
                if sampled {
                    crate::brushes::stamp_dab(
                        &mut dense,
                        point + Pt::new(256., 256.),
                        &brush,
                        &tip,
                        false,
                    );
                } else {
                    crate::paint::stamp_dab(&mut dense, point + Pt::new(256., 256.), &brush, false);
                }
            }
            let mut tiled = Pixmap::new(768, 768).unwrap();
            for (&(x, y), pm) in &buffer.tiles {
                tiled.draw_pixmap(
                    x * TILE + 256,
                    y * TILE + 256,
                    pm.as_ref(),
                    &Default::default(),
                    Transform::identity(),
                    None,
                );
            }
            for (actual, expected) in tiled.data().iter().zip(dense.data()) {
                assert!(
                    (i16::from(*actual) - i16::from(*expected)).abs() <= 1,
                    "tile seam at byte {}, sampled {sampled}: {actual}/{expected}",
                    tiled
                        .data()
                        .iter()
                        .zip(dense.data())
                        .position(|(a, b)| (i16::from(*a) - i16::from(*b)).abs() > 1)
                        .unwrap()
                );
            }
        }
    }

    #[test]
    fn large_sparse_stroke_commits_patches_and_undo_restores_the_original_grid() {
        let mut layer = Layer::raster("Paint", 1920, 1080);
        let original = RasterGrid::capture(&layer).unwrap();
        let mut buffer = BrushBuffer::default();
        let brush = Brush {
            size: 16.,
            flow: 1.,
            ..Default::default()
        };
        let points = [Pt::new(54000., -30000.), Pt::new(64000., -23000.)];
        for p in points {
            buffer.stamp(p, &brush, None).unwrap();
        }
        assert_eq!(RasterGrid::capture(&layer).unwrap(), original);
        assert!(buffer.tiles.len() < 12);
        let change = commit(&mut layer, buffer, 1.).unwrap().unwrap();
        assert!(u64::from(change.after.w) * u64::from(change.after.h) > 64_000_000);
        assert!(
            change
                .patches
                .iter()
                .map(|p| p.before.len() + p.after.len())
                .sum::<usize>()
                < 1024 * 1024
        );
        let cloned = change.clone();
        assert!(Arc::ptr_eq(
            &change.patches[0].after,
            &cloned.patches[0].after
        ));
        cloned.inverted().apply(&mut layer).unwrap();
        assert_eq!(RasterGrid::capture(&layer).unwrap(), original);
        assert!(layer.kind.pixels().unwrap().is_invisible());
        change.apply(&mut layer).unwrap();
        let pixels = layer.kind.pixels().unwrap();
        for p in points {
            let mut point = tiny_skia::Point::from_xy(p.x, p.y);
            change
                .after
                .transform()
                .invert()
                .unwrap()
                .map_point(&mut point);
            assert!(
                pixels.data[(point.y as usize * pixels.w as usize + point.x as usize) * 4 + 3] > 0
            );
        }
    }

    #[test]
    fn pixel_history_has_a_memory_budget_and_keeps_the_latest_edit() {
        let grid = RasterGrid::capture(&Layer::raster("Paint", 1, 1)).unwrap();
        let data: Arc<[u8]> = vec![0; 8 * 1024 * 1024].into();
        let mut history = crate::document::History::default();
        for _ in 0..20 {
            history.push(crate::document::Cmd::PaintRaster {
                layer: 0,
                change: RasterChange {
                    before: grid,
                    after: grid,
                    offset: (0, 0),
                    patches: vec![PixelPatch {
                        x: 0,
                        y: 0,
                        w: 1,
                        h: 1,
                        before: data.clone(),
                        after: data.clone(),
                    }],
                },
            });
        }
        assert_eq!(history.len(), 16);
        assert!(history.can_undo());
    }
}
