//! Retained vector definitions shared by SVG paint and filter graphs.
use crate::geom::Bounds;
use serde::{Deserialize, Serialize};
use tiny_skia::Transform;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SvgPattern {
    pub definitions: String,
    pub id: String,
    pub transform: [f32; 6],
    pub opacity: f32,
}

/// Rebind definition IDs for an independent exported object.
/// Args: normalized SVG definitions and a unique prefix. Returns collision-free XML.
pub fn namespace(source: &str, prefix: &str) -> String {
    let mut output = source.to_string();
    let wrapper = format!("<svg>{source}</svg>");
    let Ok(xml) = roxmltree::Document::parse(&wrapper) else {
        return output;
    };
    let ids: Vec<_> = xml
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .map(str::to_string)
        .collect();
    for id in ids {
        output = output
            .replace(&format!("id=\"{id}\""), &format!("id=\"{prefix}{id}\""))
            .replace(&format!("url(#{id})"), &format!("url(#{prefix}{id})"))
            .replace(
                &format!("href=\"#{id}\""),
                &format!("href=\"#{prefix}{id}\""),
            );
    }
    output
}

/// Encode a stored affine matrix.
/// Args: six SVG coefficients. Returns the corresponding drawing transform.
pub fn transform(matrix: [f32; 6]) -> Transform {
    let [a, b, c, d, e, f] = matrix;
    Transform::from_row(a, b, c, d, e, f)
}

/// Format an affine map for portable SVG attributes.
/// Args: drawing transform. Returns a matrix expression.
pub fn matrix(t: Transform) -> String {
    format!(
        "matrix({} {} {} {} {} {})",
        t.sx, t.ky, t.kx, t.sy, t.tx, t.ty
    )
}

impl SvgPattern {
    /// Bind a reusable vector pattern to the current object bounds.
    /// Args: unique definition prefix and object bounds. Returns definitions and the paint URL.
    pub fn definition(&self, prefix: &str, bounds: Bounds) -> (String, String) {
        let placement = Transform::from_translate(bounds.min.x, bounds.min.y)
            .pre_scale(bounds.width().max(0.001), bounds.height().max(0.001))
            .pre_concat(transform(self.transform));
        let id = format!("{prefix}paint");
        let definitions = namespace(&self.definitions, prefix);
        (
            format!(
                "{definitions}<pattern id=\"{id}\" href=\"#{prefix}{}\" patternTransform=\"{}\"/>",
                self.id,
                matrix(placement)
            ),
            format!("url(#{id})"),
        )
    }

    /// Render vector paint on a path at the requested viewport resolution.
    /// Args: destination, path, object bounds, view placement, opacity, blend and mask.
    /// Returns without changing the destination when source definitions are invalid.
    pub fn draw(
        &self,
        pm: &mut tiny_skia::Pixmap,
        path: &tiny_skia::Path,
        bounds: Bounds,
        placement: Transform,
        opacity: f32,
        blend: tiny_skia::BlendMode,
        mask: Option<&tiny_skia::Mask>,
        winding: bool,
    ) {
        let (defs, paint) = self.definition("oma-pattern-", bounds);
        let source = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"><defs>{defs}</defs><path d=\"{}\" fill=\"{paint}\" fill-opacity=\"{}\" fill-rule=\"{}\"/></svg>",
            pm.width(),
            pm.height(),
            path_data(path),
            self.opacity.clamp(0., 1.),
            if winding { "nonzero" } else { "evenodd" }
        );
        let Ok(tree) = usvg::Tree::from_str(&source, &usvg::Options::default()) else {
            return;
        };
        let Some(mut surface) = tiny_skia::Pixmap::new(pm.width(), pm.height()) else {
            return;
        };
        resvg::render(&tree, placement, &mut surface.as_mut());
        pm.draw_pixmap(
            0,
            0,
            surface.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity,
                blend_mode: blend,
                ..Default::default()
            },
            Transform::identity(),
            mask,
        );
    }
}

/// Write exact curve segments without flattening them.
/// Args: native path. Returns portable SVG path data.
pub fn path_data(path: &tiny_skia::Path) -> String {
    use tiny_skia::PathSegment::*;
    let mut out = String::new();
    for segment in path.segments() {
        match segment {
            MoveTo(p) => out.push_str(&format!("M{} {}", p.x, p.y)),
            LineTo(p) => out.push_str(&format!("L{} {}", p.x, p.y)),
            QuadTo(a, b) => out.push_str(&format!("Q{} {} {} {}", a.x, a.y, b.x, b.y)),
            CubicTo(a, b, c) => {
                out.push_str(&format!("C{} {} {} {} {} {}", a.x, a.y, b.x, b.y, c.x, c.y))
            }
            Close => out.push('Z'),
        }
    }
    out
}
