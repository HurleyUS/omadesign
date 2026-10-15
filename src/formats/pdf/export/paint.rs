//! PDF vector paint fields and reusable SVG pattern tiles.
use super::*;
use crate::gradient::Spread;

/// Keep all visible page coordinates when a paint is evaluated before object rotation.
/// Args: page bounds, rotation center and radians. Returns a conservative unrotated page rectangle.
pub(super) fn local_bounds(bounds: Bounds, center: Pt, rotation: f32) -> Bounds {
    let corners = [
        bounds.min,
        Pt::new(bounds.max.x, bounds.min.y),
        bounds.max,
        Pt::new(bounds.min.x, bounds.max.y),
    ];
    let mut result = Bounds::from_pt(corners[0].rotate_about(center, -rotation));
    for point in corners {
        result.union_pt(point.rotate_about(center, -rotation));
    }
    result
}

/// Encode a periodic native paint field without a pixel surface.
/// Args: object store, normalized gradient, object bounds and channel selection. Returns a PDF shading.
pub(super) fn field(
    pdf: &mut Pdf,
    g: &Gradient,
    bounds: Bounds,
    extent: Bounds,
    alpha: bool,
) -> Result<ObjectId, String> {
    let mut domain = extent;
    let position = if g.kind == GradientKind::Radial {
        let inverse = g
            .radial_transform(bounds)
            .invert()
            .ok_or("PDF radial paint matrix is singular")?;
        let mut corners = [
            tiny_skia::Point::from_xy(domain.min.x, domain.min.y),
            tiny_skia::Point::from_xy(domain.max.x, domain.min.y),
            tiny_skia::Point::from_xy(domain.max.x, domain.max.y),
            tiny_skia::Point::from_xy(domain.min.x, domain.max.y),
        ];
        inverse.map_points(&mut corners);
        domain = Bounds::from_pt(Pt::new(corners[0].x, corners[0].y));
        for p in corners {
            domain.union_pt(Pt::new(p.x, p.y));
        }
        let [x, y] = g.focal;
        let r = g.inner_radius.max(0.);
        let a = x * x + y * y - (1. - r).powi(2);
        let c = format!("{y} sub dup mul exch {x} sub dup mul add {} sub", r * r);
        let b = format!(
            "{y} sub {y} mul exch {x} sub {x} mul add {} sub 2 mul",
            r * (1. - r)
        );
        if a.abs() < 1e-6 {
            format!("2 copy {c} 3 1 roll {b} div neg")
        } else {
            format!(
                "2 copy {c} 3 1 roll {b} dup dup mul 3 -1 roll {} mul sub dup 0 lt {{pop 0}} if sqrt exch neg exch {} {} div",
                4. * a,
                if a < 0. { "sub" } else { "add" },
                2. * a
            )
        }
    } else {
        let (a, b) = g.endpoints(bounds);
        let d = b - a;
        if d.length_sq() < 1e-12 {
            return Err("PDF periodic linear paint has coincident endpoints".into());
        }
        format!(
            "exch {} sub {} mul exch {} sub {} mul add {} div",
            a.x,
            d.x,
            a.y,
            d.y,
            d.length_sq()
        )
    };
    let spread = match g.spread {
        Spread::Repeat => "dup floor sub",
        Spread::Reflect => "dup 2 div floor 2 mul sub dup 1 gt {2 exch sub} if",
        Spread::Pad => "dup 0 lt {pop 0} if dup 1 gt {pop 1} if",
    };
    let channel = |component: fn(Rgba) -> u8| {
        let mut code = format!(
            "pop {}",
            component(g.stops.last().unwrap().color) as f32 / 255.
        );
        for pair in g.stops.windows(2).rev() {
            if pair[1].offset <= pair[0].offset {
                continue;
            }
            let first = component(pair[0].color) as f32 / 255.;
            let delta = component(pair[1].color) as f32 / 255. - first;
            code = format!(
                "dup {} lt {{{} sub {} div {delta} mul {first} add}} {{{code}}} ifelse",
                pair[1].offset,
                pair[0].offset,
                pair[1].offset - pair[0].offset
            );
        }
        format!(
            "dup {} lt {{pop {}}} {{{code}}} ifelse",
            g.stops[0].offset,
            component(g.stops[0].color) as f32 / 255.
        )
    };
    let channels = if alpha {
        channel(|c| c.a)
    } else {
        format!(
            "dup {} exch dup {} exch {}",
            channel(|c| c.r),
            channel(|c| c.g),
            channel(|c| c.b)
        )
    };
    let domain = vec![domain.min.x, domain.max.x, domain.min.y, domain.max.y];
    if domain.iter().any(|v| !v.is_finite()) {
        return Err("PDF paint field must have finite bounds".into());
    }
    let domain = domain.into_iter().map(Object::Real).collect::<Vec<_>>();
    let range = (0..if alpha { 1 } else { 3 })
        .flat_map(|_| [Object::Integer(0), Object::Integer(1)])
        .collect::<Vec<_>>();
    let function = pdf.add_object(Stream::new(
        dictionary! {"FunctionType"=>4,"Domain"=>domain.clone(),"Range"=>range},
        format!("{{ {position} {spread} {channels} }}").into_bytes(),
    ));
    Ok(pdf.add_object(dictionary!{"ShadingType"=>1,"ColorSpace"=>if alpha{"DeviceGray"}else{"DeviceRGB"},"Domain"=>domain,"Function"=>function}))
}

impl Writer {
    /// Paint a retained SVG pattern as a reusable PDF vector tile.
    /// Args: current object, retained paint, page bounds and output. Returns a source or encoding error.
    pub(super) fn pattern(
        &mut self,
        shape: &Shape,
        pattern: &crate::svg_source::SvgPattern,
        bounds: Bounds,
        out: &mut Scene,
    ) -> Result<(), String> {
        if self.pattern_depth >= 32 {
            return Err("PDF pattern nesting exceeds32".into());
        }
        self.pattern_depth += 1;
        let result = self.pattern_inner(shape, pattern, bounds, out);
        self.pattern_depth -= 1;
        result
    }
    /// Resolve a tile using the retained SVG definitions and current object bounds.
    /// Args: object, source pattern, page bounds and destination. Returns any normalization or paint error.
    fn pattern_inner(
        &mut self,
        shape: &Shape,
        pattern: &crate::svg_source::SvgPattern,
        bounds: Bounds,
        out: &mut Scene,
    ) -> Result<(), String> {
        let b = shape.geom.bbox();
        if pattern.transform.iter().any(|v| !v.is_finite()) || !pattern.opacity.is_finite() {
            return Err("PDF pattern placement and opacity must be finite".into());
        }
        let (defs, paint) = pattern.definition("pdf-pattern-", b);
        let source = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"><defs>{defs}</defs><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{paint}\"/></svg>",
            b.max.x.max(1.),
            b.max.y.max(1.),
            b.min.x,
            b.min.y,
            b.width(),
            b.height()
        );
        let tree = crate::svg_source::scene(&source)?;
        let p = tree
            .patterns()
            .iter()
            .find(|p| p.id() == "pdf-pattern-paint")
            .ok_or("PDF pattern source could not be resolved")?;
        let rect = p.rect();
        let normalized = tree.to_string(&usvg::WriteOptions::default());
        let xml = roxmltree::Document::parse(&normalized).map_err(|e| e.to_string())?;
        let definition = xml
            .descendants()
            .find(|n| n.has_tag_name("pattern") && n.attribute("id") == Some(p.id()))
            .ok_or("Missing normalized PDF pattern tile")?;
        let content = definition
            .children()
            .filter(|n| n.is_element())
            .map(|n| &normalized[n.range()])
            .collect::<String>();
        let definitions = xml
            .descendants()
            .find(|n| n.has_tag_name("defs"))
            .map(|n| &normalized[n.range()])
            .unwrap_or("");
        let tile = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{}\" height=\"{}\">{definitions}{content}</svg>",
            rect.width(),
            rect.height()
        );
        let (doc, notes) = crate::formats::svg::read(&tile, "PDF pattern tile")?;
        for note in notes {
            self.warnings
                .insert(format!("PDF pattern on object {}: {note}", shape.id));
        }
        let (prepared, notes) = fallback::prepare(&doc)?;
        for note in notes {
            self.warnings
                .insert(format!("PDF pattern on object {}: {note}", shape.id));
        }
        let mut scene = Scene::default();
        self.children(
            &prepared,
            None,
            Bounds::from_min_size(Pt::ZERO, Pt::new(rect.width(), rect.height())),
            &mut scene,
            0,
            true,
        )?;
        let rotation = tiny_skia::Transform::from_rotate_at(
            shape.rotation.to_degrees(),
            b.center().x,
            b.center().y,
        );
        let t = rotation
            .pre_concat(p.transform())
            .pre_translate(rect.x(), rect.y());
        let encoded = scene.encode()?;
        let id=self.pdf.add_object(Stream::new(dictionary!{"Type"=>"Pattern","PatternType"=>1,"PaintType"=>1,"TilingType"=>1,"BBox"=>vec![0.into(),0.into(),Object::Real(rect.width()),Object::Real(rect.height())],"XStep"=>rect.width(),"YStep"=>rect.height(),"Matrix"=>vec![t.sx,t.ky,t.kx,t.sy,t.tx,t.ty].into_iter().map(Object::Real).collect::<Vec<_>>(),"Resources"=>scene.resources},encoded));
        let mut painted = Scene::default();
        let state = self.state(byte(pattern.opacity), byte(pattern.opacity), Blend::Normal);
        painted.invoke("gs", "ExtGState", state);
        painted.ops.push(Operation::new(
            "cs",
            vec![Object::Name(b"Pattern".to_vec())],
        ));
        painted.invoke("scn", "Pattern", id);
        path(&mut painted.ops, shape);
        op(
            &mut painted.ops,
            if shape.geom.nonzero_winding() {
                "f"
            } else {
                "f*"
            },
            &[],
        );
        let form = self.form(painted, bounds, true)?;
        out.invoke("Do", "XObject", form);
        Ok(())
    }
}
