use super::*;

fn straight(pixel: &[u8]) -> [f32; 4] {
    if pixel[3] == 0 {
        return [0.; 4];
    }
    [
        pixel[0] as f32 / pixel[3] as f32,
        pixel[1] as f32 / pixel[3] as f32,
        pixel[2] as f32 / pixel[3] as f32,
        pixel[3] as f32 / 255.,
    ]
}

fn finish(destination: &mut Pixmap, pixels: Pixmap, layer: &Layer, scale: f32) {
    let mut filters = layer.filters.clone();
    crate::filter::scale(&mut filters, scale);
    crate::filter::composite(
        destination,
        pixels,
        &filters,
        Transform::identity(),
        layer.blend.to_skia(),
        layer.opacity,
        layer.fill_opacity,
        layer.blend_interior,
        None,
    );
}

impl Context<'_, '_> {
    pub(super) fn draw_clipped(
        &self,
        pm: &mut Pixmap,
        stack: &[usize],
        depth: usize,
    ) -> Result<(), String> {
        let layer = &self.doc.layers[stack[0]];
        let mut filters = layer.filters.clone();
        crate::filter::scale(&mut filters, self.transform.get_scale().0);
        let pad = crate::filter::svg_pad(&filters).ceil() as u32;
        if pad == 0 {
            return self.draw_clipped_inner(pm, stack, depth);
        }
        let border = pad
            .checked_mul(2)
            .ok_or("Clipping effect padding overflow")?;
        let width = pm
            .width()
            .checked_add(border)
            .ok_or("Clipping width overflow")?;
        let height = pm
            .height()
            .checked_add(border)
            .ok_or("Clipping height overflow")?;
        let bytes = crate::deep_color::storage::count(width, height, 4)?;
        let _memory = crate::resources::global().reserve_now(bytes as u64, &|| Ok(()))?;
        let mut pixels =
            Pixmap::new(width, height).ok_or("Could not allocate clipping effect padding")?;
        let copy = PixmapPaint {
            blend_mode: tiny_skia::BlendMode::Source,
            ..Default::default()
        };
        pixels.draw_pixmap(
            pad as i32,
            pad as i32,
            pm.as_ref(),
            &copy,
            Transform::identity(),
            None,
        );
        Context {
            doc: self.doc,
            transform: Transform::from_translate(pad as f32, pad as f32).pre_concat(self.transform),
            draft: self.draft,
            motion: self.motion,
            overrides: self.overrides,
            children: self.children.clone(),
            sources: self.sources.clone(),
        }
        .draw_clipped_inner(&mut pixels, stack, depth)?;
        pm.draw_pixmap(
            -(pad as i32),
            -(pad as i32),
            pixels.as_ref(),
            &copy,
            Transform::identity(),
            None,
        );
        Ok(())
    }

    fn draw_clipped_inner(
        &self,
        pm: &mut Pixmap,
        stack: &[usize],
        depth: usize,
    ) -> Result<(), String> {
        let base = &self.doc.layers[stack[0]];
        if !base.visible || base.opacity <= 0. {
            return Ok(());
        }
        let pass = base.is_group
            && base.pass_through
            && !base.filters.active()
            && base.artwork_masks.is_empty()
            && base.svg_filter.is_none();
        let bytes = crate::deep_color::storage::count(pm.width(), pm.height(), 4)?;
        let reserved = (bytes as u64)
            .checked_mul(if pass { 5 } else { 2 })
            .ok_or("Clipping scratch overflow")?;
        let _memory = crate::resources::global().reserve_now(reserved, &|| Ok(()))?;
        let mut pixels =
            Pixmap::new(pm.width(), pm.height()).ok_or("Could not allocate clipping base")?;
        let mut normalized = base.clone();
        normalized.opacity = 1.;
        normalized.fill_opacity = 1.;
        normalized.blend = crate::color::Blend::Normal;
        normalized.filters = Default::default();
        normalized.pass_through = false;
        self.draw_layer_content(&mut pixels, stack[0], depth, &normalized)?;
        let original = pass.then(|| pixels.clone());
        let mut source =
            Pixmap::new(pm.width(), pm.height()).ok_or("Could not allocate clipped layer")?;
        for &index in &stack[1..] {
            if !self.doc.layers[index].visible {
                continue;
            }
            source.fill(tiny_skia::Color::TRANSPARENT);
            self.draw_layer(&mut source, index, depth)?;
            for (base, src) in pixels
                .data_mut()
                .chunks_exact_mut(4)
                .zip(source.data().chunks_exact(4))
            {
                let value = crate::layer_clipping::atop(
                    straight(base),
                    straight(src),
                    self.doc.layers[index].blend,
                );
                for c in 0..3 {
                    base[c] = (value[c] * base[3] as f32)
                        .round()
                        .clamp(0., base[3] as f32) as u8;
                }
            }
        }
        drop(source);
        let scale = self.transform.get_scale().0;
        if let Some(original) = original {
            let before = pm.clone();
            self.draw_layer(pm, stack[0], depth)?;
            let mut with = before.clone();
            let mut without = before;
            finish(&mut with, pixels, base, scale);
            finish(&mut without, original, base, scale);
            for ((dst, with), without) in pm
                .data_mut()
                .chunks_exact_mut(4)
                .zip(with.data().chunks_exact(4))
                .zip(without.data().chunks_exact(4))
            {
                let alpha =
                    (i32::from(dst[3]) + i32::from(with[3]) - i32::from(without[3])).clamp(0, 255);
                for c in 0..3 {
                    dst[c] = (i32::from(dst[c]) + i32::from(with[c]) - i32::from(without[c]))
                        .clamp(0, alpha) as u8;
                }
                dst[3] = alpha as u8;
            }
        } else {
            finish(pm, pixels, base, scale);
        }
        Ok(())
    }
}
