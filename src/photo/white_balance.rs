//! Gray-world automatic white balance for the develop temperature/tint pair.
use super::{DevelopParams, RgbaImage};

/// Channel gains the develop pipeline uses for temperature and tint (−100..100).
pub(crate) fn gains(temperature: f32, tint: f32) -> [f32; 3] {
    let t = temperature.clamp(-100., 100.) / 100.;
    let ti = tint.clamp(-100., 100.) / 100.;
    [1. + t * 0.22 + ti * 0.08, 1. - t * 0.05 - ti * 0.18, 1. - t * 0.22 + ti * 0.08]
}

/// Set temperature and tint so the image's mean color becomes neutral.
/// Args: params and preview. Leaves params unchanged for empty or near-black images.
pub fn auto_white_balance(p: &mut DevelopParams, img: &RgbaImage) {
    let mut sum = [0f64; 3];
    let mut n = 0f64;
    for px in img.data.chunks_exact(4).filter(|px| px[3] > 0) {
        for k in 0..3 {
            sum[k] += px[k] as f64;
        }
        n += 1.;
    }
    if n == 0. || sum.iter().sum::<f64>() / n < 3. {
        return;
    }
    let [r, g, b] = sum.map(|v| (v / n).max(1.));
    // Solve r·gr = b·gb and r·gr = g·gg for (t, ti) in the pipeline's linear gain model.
    let (a1, b1, c1) = (0.22 * (r + b), 0.08 * (r - b), b - r);
    let (a2, b2, c2) = (0.22 * r + 0.05 * g, 0.08 * r + 0.18 * g, g - r);
    let det = a1 * b2 - a2 * b1;
    if det.abs() < 1e-9 {
        return;
    }
    let t = (c1 * b2 - c2 * b1) / det;
    let ti = (a1 * c2 - a2 * c1) / det;
    p.temperature = ((t * 100.) as f32).clamp(-100., 100.);
    p.tint = ((ti * 100.) as f32).clamp(-100., 100.);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn craft_develop_auto_white_balance_neutralizes_a_cast() {
        let data: Vec<u8> = (0..64).flat_map(|i| [130u8 + (i % 2) as u8 * 2, 120, 110, 255]).collect();
        let img = RgbaImage::new(8, 8, data).unwrap();
        let mut p = DevelopParams::default();
        auto_white_balance(&mut p, &img);
        let gain = gains(p.temperature, p.tint);
        let mean = [131. * gain[0], 120. * gain[1], 110. * gain[2]];
        assert!((mean[0] - mean[2]).abs() < 0.5 && (mean[0] - mean[1]).abs() < 0.5, "{mean:?} {p:?}");
        assert!(p.temperature < 0. && p.temperature > -100.);
        let mut q = DevelopParams::default();
        auto_white_balance(&mut q, &RgbaImage::new(1, 1, vec![0, 0, 0, 255]).unwrap());
        assert_eq!(q, DevelopParams::default());
        let mut neutral = DevelopParams::default();
        auto_white_balance(&mut neutral, &RgbaImage::new(1, 1, vec![128, 128, 128, 255]).unwrap());
        assert!(neutral.temperature.abs() < 0.01 && neutral.tint.abs() < 0.01);
    }
}
