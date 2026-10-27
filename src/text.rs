//! Live type: rustybuzz shapes OpenType, ab_glyph traces outlines.

use crate::geom::{Pt, TypeRun};
use ab_glyph::{Font as _, GlyphId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

pub(crate) mod font_inventory;
pub mod variations;
mod font_instances;
pub(crate) mod font_export;
pub(crate) mod webfonts;
pub(crate) mod svg_fonts;
pub(crate) mod font_catalog;
pub(crate) mod font_recovery;

const PREFERRED: &[&str] = &[
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
];

const FONT_CAP: usize = 2000;

static FONTS: OnceLock<Vec<FontFace>> = OnceLock::new();
static BYTES: OnceLock<Mutex<HashMap<PathBuf, Arc<Vec<u8>>>>> = OnceLock::new();
static MEMORY_NAMES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

pub fn project_font_loaded(id: &str) -> bool {
    BYTES.get().is_some_and(|cache| {
        cache
            .lock()
            .is_ok_and(|cache| cache.contains_key(Path::new(id)))
    })
}

pub(crate) fn project_font_bytes(id: &str) -> Option<Arc<Vec<u8>>> {
    if !id.starts_with("omatype:") {
        return None;
    }
    BYTES.get()?.lock().ok()?.get(Path::new(id)).cloned()
}

/// Project fonts are addressed by their bytes, independent of a machine's paths.
/// They do not join the global installed-font picker.
pub fn register_memory_font(id: &str, name: &str, bytes: Arc<Vec<u8>>) -> Result<(), String> {
    let expected = format!("omatype:{:032x}", crate::typography::fingerprint(&bytes));
    if id != expected
        || rustybuzz::Face::from_slice(&bytes, 0).is_none()
        || ab_glyph::FontRef::try_from_slice(&bytes).is_err()
    {
        return Err("Invalid project font data".into());
    }
    let mut cache = BYTES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "Font cache is unavailable")?;
    let path = PathBuf::from(id);
    let added = !cache.contains_key(&path);
    if let Some(old) = cache.get(&path) {
        if **old != *bytes {
            return Err("Project font fingerprint collision".into());
        }
    } else {
        cache.insert(path, bytes);
    }
    MEMORY_NAMES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "Font labels are unavailable")?
        .insert(id.into(), name.into());
    drop(cache);
    if added { FONT_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
    Ok(())
}

pub fn font_name(bytes: &[u8]) -> Option<String> {
    let face = rustybuzz::ttf_parser::Face::parse(bytes, 0).ok()?;
    for id in [4, 1] {
        if let Some(name) = face
            .names()
            .into_iter()
            .filter(|name| name.name_id == id)
            .find_map(|name| name.to_string().filter(|name| !name.trim().is_empty()))
        {
            return Some(name.trim().to_owned());
        }
    }
    None
}

#[derive(Clone)]
pub struct FontFace {
    pub name: String,
    pub path: PathBuf,
}

fn family_name(path: &Path) -> String {
    // Try to read the OpenType name table for a human family + style.
    if let Ok(bytes) = std::fs::read(path)
        && let Ok(face) = rustybuzz::ttf_parser::Face::parse(&bytes, 0)
    {
        let mut family: Option<String> = None;
        let mut style: Option<String> = None;
        for n in face.names() {
            if n.name_id == 1
                && n.is_unicode()
                && let Some(s) = n.to_string()
            {
                let s = s.trim().to_string();
                if !s.is_empty() {
                    family = Some(s);
                }
            }
            if n.name_id == 2
                && n.is_unicode()
                && let Some(s) = n.to_string()
            {
                let s = s.trim().to_string();
                if !s.is_empty() && !s.eq_ignore_ascii_case("regular") {
                    style = Some(s);
                }
            }
        }
        if let Some(fam) = family {
            if let Some(st) = style {
                // Avoid duplicating family when style already contains it.
                if st.to_lowercase().contains(&fam.to_lowercase()) {
                    return st;
                }
                return format!("{fam} {st}");
            }
            return fam;
        }
    }
    path.file_stem()
        .map(|s| {
            s.to_string_lossy()
                .replace(['-', '_'], " ")
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| "Font".into())
}

fn fc_font_files() -> Vec<PathBuf> {
    let output = std::process::Command::new("fc-list")
        .args(["-f", "%{file}\n", ":"])
        .output();
    let Ok(out) = output else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut v = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let p = PathBuf::from(t);
        if p.extension()
            .and_then(|e| e.to_str())
            .map(|e| {
                matches!(
                    e.to_ascii_lowercase().as_str(),
                    "ttf" | "otf" | "ttc" | "otc"
                )
            })
            .unwrap_or(false)
        {
            v.push(p);
        }
    }
    v.sort();
    v.dedup();
    v
}

fn scan_fonts() -> Vec<FontFace> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // Preferred still first so the initial sort is stable for bundled fallbacks.
    for p in PREFERRED {
        let path = PathBuf::from(p);
        if path.exists() && seen.insert(path.clone()) {
            let name = family_name(&path);
            out.push(FontFace { name, path });
        }
    }
    // Canonical set from fontconfig – this is the broadest and respects the
    // user's fontconfig excludes/substitutes.
    for path in fc_font_files() {
        if out.len() >= FONT_CAP {
            break;
        }
        if seen.insert(path.clone()) {
            let name = family_name(&path);
            out.push(FontFace { name, path });
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    // Fallback recursive walk for directories fontconfig may not yet know
    // (e.g. freshly downloaded Google Fonts before fc-cache).
    for root in [
        "/usr/share/fonts",
        "/usr/local/share/fonts",
        &format!("{home}/.local/share/fonts"),
        &format!("{home}/.fonts"),
        "/usr/share/fonts/liberation",
        "/usr/share/fonts/noto",
        "/usr/share/fonts/TTF",
        "/usr/share/fonts/truetype",
        "/usr/share/fonts/adobe-source-code-pro",
        "/usr/share/fonts/gsfonts",
        "/usr/share/fonts/ttf-ia-writer",
    ] {
        if root.is_empty() {
            continue;
        }
        walk_ttfs(Path::new(root), &mut out, &mut seen, 0);
        if out.len() >= FONT_CAP {
            break;
        }
    }
    walk_ttfs(&crate::project::data_dir().join("fonts"), &mut out, &mut seen, 0);
    out.sort_by_cached_key(|f| f.name.to_lowercase());
    out.truncate(FONT_CAP);
    out
}

fn walk_ttfs(
    dir: &Path,
    out: &mut Vec<FontFace>,
    seen: &mut std::collections::HashSet<PathBuf>,
    depth: u8,
) {
    if depth > 8 || out.len() >= FONT_CAP {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_ttfs(&p, out, seen, depth + 1);
            continue;
        }
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());
        if matches!(
            ext.as_deref(),
            Some("ttf") | Some("otf") | Some("ttc") | Some("otc")
        ) && seen.insert(p.clone())
        {
            let name = family_name(&p);
            out.push(FontFace { name, path: p });
        }
    }
}

pub fn fonts() -> &'static [FontFace] {
    FONTS.get_or_init(load_font_index)
}

/// Whether the installed font index is built; the UI fills font defaults only after this.
pub fn fonts_ready() -> bool {
    FONTS.get().is_some()
}

/// Build the installed font index on a background thread so the window never waits on it.
/// Args: none. Returns immediately; later `fonts()` callers block only until it finishes.
pub fn start_font_index() {
    if fonts_ready() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("font-index".into())
        .spawn(|| {
            fonts();
        });
}

const FONT_INDEX_FILE: &str = "font-index-v1.tsv";

/// A key that changes when fontconfig's file list or any scanned font directory changes.
fn font_index_key(files: &[PathBuf]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    files.hash(&mut hasher);
    let home = std::env::var("HOME").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = ["/usr/share/fonts", "/usr/local/share/fonts"]
        .into_iter()
        .map(PathBuf::from)
        .chain([PathBuf::from(format!("{home}/.local/share/fonts")), PathBuf::from(format!("{home}/.fonts"))])
        .collect();
    dirs.push(crate::project::data_dir().join("fonts"));
    for dir in dirs {
        let stamp = std::fs::metadata(&dir).and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos());
        (dir, stamp).hash(&mut hasher);
    }
    PREFERRED.iter().map(|p| Path::new(p).exists()).collect::<Vec<_>>().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Read the cached font index, or scan fonts and write it.
/// Args: none. Returns the installed faces; a stale or unreadable cache is rebuilt.
fn load_font_index() -> Vec<FontFace> {
    let path = crate::project::data_dir().join(FONT_INDEX_FILE);
    let key = font_index_key(&fc_font_files());
    if let Some(found) = read_font_index(&path, &key) {
        return found;
    }
    let found = scan_fonts();
    write_font_index(&path, &key, &found);
    found
}

fn read_font_index(path: &Path, key: &str) -> Option<Vec<FontFace>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != key {
        return None;
    }
    let faces: Vec<FontFace> = lines
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, path)| FontFace { name: name.to_owned(), path: PathBuf::from(path) })
        .collect();
    (!faces.is_empty()).then_some(faces)
}

fn write_font_index(path: &Path, key: &str, faces: &[FontFace]) {
    let mut text = format!("{key}\n");
    for face in faces {
        if face.name.contains(['\t', '\n']) || face.path.to_string_lossy().contains(['\t', '\n']) {
            continue;
        }
        text.push_str(&format!("{}\t{}\n", face.name, face.path.to_string_lossy()));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

static DYNAMIC_FONTS: OnceLock<Mutex<Vec<FontFace>>> = OnceLock::new();
static FONT_LIST: OnceLock<Mutex<Option<Arc<[FontFace]>>>> = OnceLock::new();
static RESCANNED_FONTS: OnceLock<Mutex<Option<Vec<FontFace>>>> = OnceLock::new();
static INSTALLED_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static FONT_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn register_font(face: FontFace) {
    let path = face.path.clone();
    let m = DYNAMIC_FONTS.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(mut g) = m.lock()
        && !g.iter().any(|f| f.path == face.path)
    {
        g.push(face);
    }
    if let Some(list) = FONT_LIST.get()
        && let Ok(mut cached) = list.lock()
    {
        *cached = None;
    }
    // Reload only the installed face; other open documents keep their fonts.
    if let Some(cache) = BYTES.get()
        && let Ok(mut m) = cache.lock()
    {
        m.remove(&path);
    }
    INSTALLED_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    FONT_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Rescan installed font files without changing authored text or clearing document fonts.
/// Args: none. Returns the current installed face count.
pub(crate) fn refresh_fonts() -> usize {
    let found = scan_fonts();
    let count = found.len();
    *RESCANNED_FONTS.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner()) = Some(found);
    if let Some(list) = FONT_LIST.get() { *list.lock().unwrap_or_else(|e| e.into_inner()) = None; }
    if let Some(bytes) = BYTES.get() {
        bytes.lock().unwrap_or_else(|e| e.into_inner()).retain(|path, _| path.to_string_lossy().starts_with("omatype:"));
    }
    INSTALLED_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    FONT_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    count
}

pub fn all_fonts_cached() -> Arc<[FontFace]> {
    let cache = FONT_LIST.get_or_init(|| Mutex::new(None));
    let mut cached = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(list) = cached.as_ref() {
        return Arc::clone(list);
    }
    let mut v = RESCANNED_FONTS.get().and_then(|f| f.lock().ok()?.clone()).unwrap_or_else(|| fonts().to_vec());
    if let Some(m) = DYNAMIC_FONTS.get()
        && let Ok(g) = m.lock()
    {
        for f in g.iter().filter(|f| f.path.is_file()) {
            if !v.iter().any(|e| e.path == f.path) {
                v.push(f.clone());
            }
        }
    }
    v.sort_by_cached_key(|f| f.name.to_lowercase());
    let list: Arc<[FontFace]> = v.into();
    *cached = Some(Arc::clone(&list));
    list
}

pub fn default_path() -> Option<PathBuf> {
    fonts().first().map(|f| f.path.clone())
}

pub fn face_for(path: &str) -> Option<&'static FontFace> {
    if path.is_empty() {
        return fonts().first();
    }
    fonts().iter().find(|f| f.path.to_string_lossy() == path)
}

pub fn label_for(path: &str) -> String {
    if path.starts_with("omatype:") {
        return MEMORY_NAMES
            .get()
            .and_then(|names| names.lock().ok()?.get(path).cloned())
            .unwrap_or_else(|| "Project font (load its typography kit)".into());
    }
    face_for(path).map(|f| f.name.clone()).unwrap_or_else(|| {
        if path.is_empty() {
            fonts()
                .first()
                .map(|f| f.name.clone())
                .unwrap_or_else(|| "System sans".into())
        } else {
            Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().replace('-', " "))
                .unwrap_or_else(|| "Font".into())
        }
    })
}

fn resolve_path(run: &TypeRun) -> Option<PathBuf> {
    if run.font.starts_with("omatype:") {
        let path = PathBuf::from(&run.font);
        return BYTES
            .get()
            .and_then(|cache| cache.lock().ok()?.contains_key(&path).then_some(path));
    }
    if !run.font.is_empty() {
        let p = PathBuf::from(&run.font);
        if p.exists() {
            return Some(p);
        }
    }
    default_path()
}

fn font_bytes(path: &Path) -> Option<Arc<Vec<u8>>> {
    let cache = BYTES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().ok()?;
    if let Some(b) = map.get(path) {
        return Some(b.clone());
    }
    let raw = std::fs::read(path).ok()?;
    let arc = Arc::new(raw);
    map.insert(path.to_path_buf(), arc.clone());
    Some(arc)
}

/// Share the composer's resolved font bytes with format writers.
/// Args: text run. Returns cached font bytes, including project fonts, or None.
pub(crate) fn font_bytes_for(run: &TypeRun) -> Option<Arc<Vec<u8>>> {
    resolve_path(run).and_then(|path| font_bytes(&path))
}

/// Read the composer's font ascent, descent and line gap in design pixels.
/// Args: authored text run. Returns real OpenType metrics, or None without a font.
pub(crate) fn font_metrics_for(run: &TypeRun) -> Option<(f32, f32, f32)> {
    let bytes = font_bytes_for(run)?;
    let face = variations::shaping_face(&bytes, &run.font_variations)?;
    let scale = run.px * character_metrics(run, 0).vscale.unwrap_or(100.0) / 100.0 / face.units_per_em() as f32;
    Some((face.ascender() as f32 * scale, -(face.descender() as f32) * scale, face.line_gap() as f32 * scale))
}

fn ot_features(run: &TypeRun) -> Vec<rustybuzz::Feature> {
    let tag = |b: &[u8; 4]| rustybuzz::ttf_parser::Tag::from_bytes(b);
    let feat = |bytes: &[u8; 4], on: bool| rustybuzz::Feature::new(tag(bytes), u32::from(on), ..);
    vec![
        feat(b"kern", run.kern),
        feat(b"liga", run.liga),
        feat(b"clig", run.liga),
        feat(b"tnum", run.tnum),
        feat(b"smcp", run.smcp),
        feat(b"c2sc", run.smcp),
    ]
}

fn map_pt(p: ab_glyph::Point, ox: f32, oy: f32, scale: f32) -> ab_glyph::Point {
    ab_glyph::point(ox + p.x * scale, oy - p.y * scale)
}

fn map_curves(
    curves: &[ab_glyph::OutlineCurve],
    ox: f32,
    oy: f32,
    scale: f32,
) -> Vec<ab_glyph::OutlineCurve> {
    use ab_glyph::OutlineCurve;
    curves
        .iter()
        .map(|c| match *c {
            OutlineCurve::Line(p0, p1) => {
                OutlineCurve::Line(map_pt(p0, ox, oy, scale), map_pt(p1, ox, oy, scale))
            }
            OutlineCurve::Quad(p0, p1, p2) => OutlineCurve::Quad(
                map_pt(p0, ox, oy, scale),
                map_pt(p1, ox, oy, scale),
                map_pt(p2, ox, oy, scale),
            ),
            OutlineCurve::Cubic(p0, p1, p2, p3) => OutlineCurve::Cubic(
                map_pt(p0, ox, oy, scale),
                map_pt(p1, ox, oy, scale),
                map_pt(p2, ox, oy, scale),
                map_pt(p3, ox, oy, scale),
            ),
        })
        .collect()
}

fn flatten_outline(curves: &[ab_glyph::OutlineCurve], tolerance: f32) -> Vec<Vec<Pt>> {
    use ab_glyph::OutlineCurve;
    let start_of = |c: &OutlineCurve| match c {
        OutlineCurve::Line(p0, _)
        | OutlineCurve::Quad(p0, _, _)
        | OutlineCurve::Cubic(p0, _, _, _) => *p0,
    };
    let end_of = |c: &OutlineCurve| match c {
        OutlineCurve::Line(_, p1)
        | OutlineCurve::Quad(_, _, p1)
        | OutlineCurve::Cubic(_, _, _, p1) => *p1,
    };
    let mut out = vec![];
    let mut pts: Vec<Pt> = vec![];
    let mut last_end: Option<ab_glyph::Point> = None;
    for c in curves {
        let s = start_of(c);
        let broke = last_end
            .map(|e| (e.x - s.x).abs() > 1e-3 || (e.y - s.y).abs() > 1e-3)
            .unwrap_or(true);
        if broke {
            if pts.len() >= 3 {
                out.push(std::mem::take(&mut pts));
            } else {
                pts.clear();
            }
            pts.push(Pt::new(s.x, s.y));
        }
        match c {
            OutlineCurve::Line(_, p1) => pts.push(Pt::new(p1.x, p1.y)),
            OutlineCurve::Quad(p0, p1, p2) => {
                let [p0, p1, p2] = [p0, p1, p2].map(|p| Pt::new(p.x, p.y));
                flatten_type_curve([p0, p0.lerp(p1, 2. / 3.), p2.lerp(p1, 2. / 3.), p2], tolerance, 0, &mut pts);
            }
            OutlineCurve::Cubic(p0, p1, p2, p3) => {
                flatten_type_curve([p0, p1, p2, p3].map(|p| Pt::new(p.x, p.y)), tolerance, 0, &mut pts);
            }
        }
        last_end = Some(end_of(c));
    }
    if pts.len() >= 3 {
        out.push(pts);
    }
    out
}

/// Flatten a type curve within its document-space error budget.
/// Args: cubic controls, local tolerance, bounded depth and output points. Returns nothing.
fn flatten_type_curve(points: [Pt; 4], tolerance: f32, depth: u8, out: &mut Vec<Pt>) {
    let [a, b, c, d] = points;
    let error = crate::geom::seg_dist(b, a, d).max(crate::geom::seg_dist(c, a, d));
    if depth == 10 || error <= tolerance.max(1e-6) {
        out.push(d);
        return;
    }
    let (left, right) = crate::geom::split_cubic(a, b, c, d, 0.5);
    flatten_type_curve(left, tolerance, depth + 1, out);
    flatten_type_curve(right, tolerance, depth + 1, out);
}

fn fallback_block(content: &str, px: f32, origin: Pt) -> Vec<Vec<Pt>> {
    let lines = content.split('\n').count().max(1);
    let cols = content
        .split('\n')
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .max(1);
    let w = px * 0.55 * cols as f32;
    let h = px * lines as f32;
    vec![vec![
        Pt::new(origin.x, origin.y - px),
        Pt::new(origin.x + w, origin.y - px),
        Pt::new(origin.x + w, origin.y - px + h),
        Pt::new(origin.x, origin.y - px + h),
    ]]
}

/// Shape `run` into closed contours in world space.
pub fn shape(run: &TypeRun) -> Vec<Vec<Pt>> {
    if run.svg_text.is_some() {return svg_type::layout(run).map(|l|l.contours.clone()).unwrap_or_default();}
    if run.on_path.is_some() { return crate::text_geometry::shape_on_path(run); }
    if run.frame.is_some() { return crate::text_geometry::shape_frame(run); }
    let Some(path) = resolve_path(run) else {
        return fallback_block(&run.content, run.px, run.origin);
    };
    let Some(bytes) = font_bytes(&path) else {
        return fallback_block(&run.content, run.px, run.origin);
    };
    let mut contours=buzz_shape(&bytes, run).unwrap_or_else(|| fallback_block(&run.content, run.px, run.origin));
    for point in contours.iter_mut().flatten(){*point=run.map_point(*point);}
    contours
}

fn buzz_shape(_bytes: &[u8], run: &TypeRun) -> Option<Vec<Vec<Pt>>> {
    Some(compose(run).iter().flat_map(|line|line.glyphs.iter().flat_map(|g|glyph_contours(run,g))).collect())
}

/// Keep native glyph curves for painting ordinary editable text.
/// Args: the composed text run. Returns its exact path, or no path for other layout routes.
pub(crate) fn outline_path(run: &TypeRun) -> Option<tiny_skia::Path> {
    if !has_outline_path(run) {
        return None;
    }
    let mut path = tiny_skia::PathBuilder::new();
    for line in compose(run).iter() {
        for glyph in &line.glyphs {
            append_glyph_outline(&mut path, run, glyph);
        }
    }
    path.finish()
}

fn has_outline_path(run: &TypeRun) -> bool {
    !run.content.is_empty() && run.frame.is_none() && run.on_path.is_none()
        && run.svg_text.is_none() && run.contours == shape(run)
}

/// Append one positioned glyph using its original font curves.
/// Args: output builder, semantic text run and composed glyph. Returns nothing.
fn append_glyph_outline(path: &mut tiny_skia::PathBuilder, run: &TypeRun, glyph: &LayoutGlyph) {
    use ab_glyph::OutlineCurve;
    if run.inlines.iter().any(|item| item.at == glyph.cluster) {
        return;
    }
    let Some((outline, scale)) = composer::glyph_outline(run, glyph) else { return; };
    let center = if glyph.turn == 0 {
        Pt::ZERO
    } else {
        composer::upright_center(run, glyph)
    };
    let map = |p: ab_glyph::Point| {
        let local = Pt::new(p.x * scale * glyph.hscale, -p.y * scale * glyph.vscale);
        let turned = if glyph.turn == 2 {
            composer::turn_clockwise(local, center)
        } else {
            local
        };
        let placed = if glyph.turn == 0 {
            Pt::new(glyph.x + local.x, glyph.y + local.y)
        } else {
            turned + (Pt::new(glyph.x, glyph.y) - center)
        };
        run.map_point(Pt::new(run.origin.x + placed.x, run.origin.y + placed.y))
    };
    let mut last = None;
    for curve in &outline.curves {
        let start = match *curve {
            OutlineCurve::Line(a, _) | OutlineCurve::Quad(a, _, _) | OutlineCurve::Cubic(a, _, _, _) => a,
        };
        if last != Some(start) {
            if last.is_some() { path.close(); }
            let a = map(start);
            path.move_to(a.x, a.y);
        }
        let end = match *curve {
            OutlineCurve::Line(_, b) | OutlineCurve::Quad(_, _, b) | OutlineCurve::Cubic(_, _, _, b) => b,
        };
        match *curve {
            OutlineCurve::Line(_, b) => {
                let b = map(b);
                path.line_to(b.x, b.y);
            }
            OutlineCurve::Quad(_, b, c) => {
                let (b, c) = (map(b), map(c));
                path.quad_to(b.x, b.y, c.x, c.y);
            }
            OutlineCurve::Cubic(_, b, c, d) => {
                let (b, c, d) = (map(b), map(c), map(d));
                path.cubic_to(b.x, b.y, c.x, c.y, d.x, d.y);
            }
        }
        last = Some(end);
    }
    if last.is_some() { path.close(); }
}

/// Paint character ranges with the same glyph curves as whole-object text.
/// Args: run and inherited object fill. Returns ordered fill paths in document coordinates.
pub(crate) fn paint_paths(run: &TypeRun, inherited: &crate::document::Fill) -> Vec<(crate::document::Fill, tiny_skia::Path)> {
    if !has_outline_path(run) {
        return paint_groups(run, inherited).into_iter().filter_map(|(fill, contours)| {
            let mut path = tiny_skia::PathBuilder::new();
            for contour in contours {
                if contour.len() < 2 { continue; }
                path.move_to(contour[0].x, contour[0].y);
                for p in &contour[1..] { path.line_to(p.x, p.y); }
                path.close();
            }
            path.finish().map(|path| (fill, path))
        }).collect();
    }
    let mut paths: Vec<(crate::document::Fill, tiny_skia::PathBuilder)> = Vec::new();
    for line in compose(run).iter() {
        for glyph in &line.glyphs {
            let fill = run.character_style(glyph.cluster).fill.unwrap_or_else(|| inherited.clone());
            if !paths.last().is_some_and(|(previous, _)| *previous == fill) {
                paths.push((fill, tiny_skia::PathBuilder::new()));
            }
            append_glyph_outline(&mut paths.last_mut().unwrap().1, run, glyph);
        }
    }
    paths.into_iter().filter_map(|(fill, path)| path.finish().map(|path| (fill, path))).collect()
}

mod composer;
mod vertical;
pub(crate) use vertical::layout_vertical_frame;
mod inline;
pub(crate) use inline::{hidden_marks, inline_paths, paint_run, refresh_document, HiddenMark};
pub mod svg_type;
mod edit;
pub(crate) use edit::CharacterRewrite;
mod metrics;
pub use metrics::character_metrics;
mod opentype;
pub use opentype::{font_features, feature_value, feature_css, glyph_alternates};
pub mod hyphenation;
pub use composer::{compose, compose_plain, glyph_contours, paragraph_style, LayoutGlyph, LayoutLine};

pub fn fill_contours(geom: &mut crate::geom::Geom) { if let crate::geom::Geom::Text(run)=geom {run.contours=shape(run);} }

pub(crate) fn has_character_paint(run:&TypeRun)->bool{run.spans.iter().any(|s|s.fill.is_some()) || run.layout.as_ref().is_some_and(|layout|layout.paint_ranges.iter().any(|(fill,_)|fill.is_some()))}

pub(crate) fn paint_groups(run:&TypeRun,inherited:&crate::document::Fill)->Vec<(crate::document::Fill,Vec<Vec<Pt>>)> {
    let mut groups:Vec<(crate::document::Fill,Vec<Vec<Pt>>)>=Vec::new();
    if run.frame.is_some() || run.on_path.is_some(){
        let computed;let layout=if let Some(layout)=&run.layout{layout}else{computed=if run.on_path.is_some(){crate::text_geometry::layout_on_path(run)}else{crate::text_geometry::layout_frame(run,run,0,&[],false)};&computed};
        for (fill,range) in &layout.paint_ranges{let fill=fill.clone().unwrap_or_else(||inherited.clone());let contours=layout.contours[range.clone()].to_vec();if let Some((_,all))=groups.last_mut().filter(|(old,_)|*old==fill){all.extend(contours);}else{groups.push((fill,contours));}}return groups;
    }
    for line in compose(run).iter(){for glyph in &line.glyphs {
        let fill=run.character_style(glyph.cluster).fill.unwrap_or_else(||inherited.clone());
        let mut contours=glyph_contours(run,glyph);
        for point in contours.iter_mut().flatten(){*point=run.map_point(*point);}
        if let Some((_,all))=groups.last_mut().filter(|(old,_)|*old==fill){all.extend(contours);}else{groups.push((fill,contours));}
    }}
    groups
}
/// Point type written in a column. Area type, path type and SVG text keep their own layout.
/// Args: text run. Returns whether carets, hits and measure use the column.
fn vertical_point(run: &TypeRun) -> bool {
    run.vertical && run.on_path.is_none() && run.frame.is_none() && run.svg_text.is_none()
}
pub fn measure(run:&TypeRun)->(f32,f32) {
    let lines=compose(run);
    if vertical_point(run) {
        let pitch = run.px.max(1.);
        let width = pitch * lines.len().max(1) as f32;
        let height = lines.iter().map(|line| line.height).fold(0., f32::max);
        return (width, height);
    }
    (run.wrap_width.unwrap_or_else(||lines.iter().map(|l|l.width).fold(0.,f32::max)),lines.iter().map(|l|l.height).sum())
}
pub fn caret_pt(run:&TypeRun,char_idx:usize)->Pt {
    if run.svg_text.is_some() && let Some(layout)=svg_type::layout(run) && let Some(caret)=layout.carets.iter().min_by_key(|c|c.0.abs_diff(char_idx)){return caret.1;}
    if let Some((point,_)) = crate::text_geometry::caret_frame(run,char_idx) { return point; }
    if vertical_point(run) {
        let lines = compose(run);
        let idx = char_idx.min(run.content.chars().count());
        let line = &lines[lines.iter().rposition(|line| line.start <= idx).unwrap_or(0)];
        return run.map_point(Pt::new(run.origin.x + line.offset, run.origin.y + line.caret_x(idx)));
    }
    let lines=compose(run);let idx=char_idx.min(run.content.chars().count());
    let line=&lines[lines.iter().rposition(|l|l.start<=idx).unwrap_or(0)];
    run.map_point(Pt::new(run.origin.x+line.caret_x(idx),run.origin.y+line.baseline-character_metrics(run,idx.min(run.content.chars().count().saturating_sub(1))).baseline_shift.unwrap()))
}

/// Return both ends of the insertion caret in document coordinates.
/// Args: text run and character index. Returns top and bottom after all type transforms.
pub fn caret_segment(run:&TypeRun,index:usize)->(Pt,Pt) {
    if run.svg_text.is_some(){return svg_type::caret_segment(run,index);}
    let point=caret_pt(run,index);let height=caret_height(run,index);
    let geometry=crate::text_geometry::caret_frame(run,index);
    let tangent=if vertical_point(run){Pt::new(0.,1.)}else{geometry.map_or(Pt::new(1.,0.),|(_,t)|t)};
    if run.transform.is_some(){let local=run.unmap_point(point);let tangent=geometry.map_or(Pt::new(1.,0.),|(_,t)|(run.unmap_point(point+t)-local).normalized());return (run.map_point(local-tangent.perp()*height*0.9),run.map_point(local+tangent.perp()*height*0.2));}
    (point-tangent.perp()*height*0.9,point+tangent.perp()*height*0.2)
}
pub fn hit_char(run:&TypeRun,p:Pt)->usize {
    if run.svg_text.is_some() && let Some(layout)=svg_type::layout(run){return layout.carets.iter().min_by(|a,b|(a.1-p).length_sq().total_cmp(&(b.1-p).length_sq())).map_or(0,|c|c.0);}
    if let Some(index) = crate::text_geometry::hit_char(run,p) { return index; }
    let p=run.unmap_point(p);
    if vertical_point(run) {
        let mut best = (0usize, f32::MAX);
        for line in compose(run).iter() {
            for glyph in &line.glyphs {
                let center = Pt::new(run.origin.x + glyph.x, run.origin.y + glyph.y);
                let distance = (center - p).length_sq();
                if distance < best.1 {
                    best = (glyph.cluster, distance);
                }
            }
        }
        return best.0;
    }
    let lines=compose(run);
    lines.iter().flat_map(|line|line.carets.iter().map(move |(index,x)| {
        let style=character_metrics(run,(*index).min(run.content.chars().count().saturating_sub(1)));
        let y=run.origin.y+line.baseline-style.baseline_shift.unwrap();let height=style.px.unwrap_or(run.px)*style.vscale.unwrap()/100.;
        let dy=if p.y<y-height*0.9 {y-height*0.9-p.y}else if p.y>y+height*0.25{p.y-y-height*0.25}else{0.};
        let dx=run.origin.x+x-p.x;(*index,dx*dx+dy*dy)
    })).min_by(|(_,a),(_,b)|a.total_cmp(b)).map_or(0,|(index,_)|index)
}
pub fn selection_rects(run:&TypeRun,a:usize,b:usize)->Vec<(Pt,Pt)> {
    let lo=a.min(b);let hi=a.max(b);let mut rects=Vec::new();
    if vertical_point(run) {
        let half = run.px.max(1.) * 0.5;
        for line in compose(run).iter() {
            let from = lo.max(line.start);
            let end = hi.min(line.end);
            if from >= end {
                continue;
            }
            let y0 = line.caret_x(from);
            let y1 = line.caret_x(end);
            let x = run.origin.x + line.offset;
            rects.push((
                Pt::new(x - half, run.origin.y + y0.min(y1)),
                Pt::new(x + half, run.origin.y + y0.max(y1)),
            ));
        }
        return rects;
    }
    for line in compose(run).iter() {
        let mut from=lo.max(line.start);let end=hi.min(line.end);
        while from<end {
            let style=character_metrics(run,from);let mut to=from+1;
            while to<end {let next=character_metrics(run,to);if next.baseline_shift!=style.baseline_shift||next.vscale!=style.vscale||next.px!=style.px{break;}to+=1;}
            let y=run.origin.y+line.baseline-style.baseline_shift.unwrap();let px=style.px.unwrap_or(run.px)*style.vscale.unwrap()/100.;
            rects.push((Pt::new(run.origin.x+line.caret_x(from),y-px*0.9),Pt::new(run.origin.x+line.caret_x(to),y+px*0.25)));from=to;
        }
    }
    rects
}
pub fn caret_height(run:&TypeRun,index:usize)->f32 {if let Some(layout)=&run.layout&&let Some((_,height))=layout.caret_heights.iter().min_by_key(|(i,_)|i.abs_diff(index)){return *height;}{let style=character_metrics(run,index.min(run.content.chars().count().saturating_sub(1)));style.px.unwrap_or(run.px)*style.vscale.unwrap()/100.}}
pub fn char_to_byte(s:&str,char_idx:usize)->usize {s.char_indices().nth(char_idx).map(|(i,_)|i).unwrap_or(s.len())}
pub fn paragraph_spacing_css(run: &TypeRun, style: &crate::geom::ParagraphStyle) -> String {
    // CSS word spacing adds to the font's space advance; the inspector stores
    // a percentage of that advance. Measure the same space as the composer.
    let space_run = TypeRun { content: " ".into(), wrap_width: None, paragraphs: vec![], spans: vec![], ..run.clone() };
    let space = compose_plain(&space_run)[0].glyphs.iter().map(|glyph| glyph.advance).sum::<f32>();
    let untracked = TypeRun { tracking: 0., ..space_run };
    let base_space = compose_plain(&untracked)[0].glyphs.iter().map(|glyph| glyph.advance).sum::<f32>();
    let px = run.px.max(1.);
    format!("--oma-paragraph-letter-spacing:{}em;letter-spacing:calc({}em + var(--oma-paragraph-letter-spacing));word-spacing:{}em;", style.letter_spacing[1] / 100., (space - base_space) / px, space * (style.word_spacing[1] / 100. - 1.) / px)
}
pub fn character_css(run:&TypeRun,style:&crate::geom::CharSpan)->String {
    use std::fmt::Write;
    let mut css=String::new();
    if let Some(font)=&style.font {let _=write!(css,"font-family:'{}';",label_for(font).replace(['\'','"','<','>'],""));}
    if let Some(px)=style.px {let _=write!(css,"font-size:{px}px;");}
    if let Some(crate::document::Fill::Solid(color))=&style.fill {let _=write!(css,"color:{};",color.hex());}
    if let Some(value)=style.tracking{let _=write!(css,"letter-spacing:calc({}em + var(--oma-paragraph-letter-spacing, 0em));",value/1000.);}
    if let Some(value)=style.leading{let _=write!(css,"line-height:{}px;",value.pixels(run.px));}
    if let Some(mode)=style.kerning{let _=write!(css,"font-kerning:{};",if mode==crate::geom::KernMode::Metrics{"normal"}else{"none"});}
    if let Some(value)=style.baseline_shift{let _=write!(css,"position:relative;top:{}px;",-value);}
    if style.hscale.is_some()||style.vscale.is_some(){let _=write!(css,"display:inline-block;transform-origin:left baseline;transform:scale({},{});",style.hscale.unwrap_or(100.)/100.,style.vscale.unwrap_or(100.)/100.);}
    css
}
pub fn styled_html(run:&TypeRun,start:usize,text:&str)->String {
    let escape=|s:&str|s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;");
    let mut out=String::new();let mut from=0;let chars:Vec<_>=text.chars().collect();
    while from<chars.len() {
        let style=run.character_style(start+from);let mut to=from+1;
        while to<chars.len()&&run.character_style(start+to)==style {to+=1;}
        let mut features=vec![(*b"kern",u32::from(run.kern)),(*b"liga",u32::from(run.liga)),(*b"clig",u32::from(run.liga)),(*b"tnum",u32::from(run.tnum)),(*b"smcp",u32::from(run.smcp)),(*b"c2sc",u32::from(run.smcp))];
        for &(tag,value) in run.features.iter().chain(&style.features) {features.retain(|(t,_)|*t!=tag);features.push((tag,value));}
        if let Some(mode)=style.kerning {features.retain(|(tag,_)|*tag!=*b"kern");features.push((*b"kern",u32::from(mode==crate::geom::KernMode::Metrics)));}
        out.push_str(&format!("<span style=\"font-feature-settings:{};{}\">{}</span>",feature_css(&features),format!("{}{}",if style.no_break{"white-space:pre;"}else{""},character_css(run,&style)),escape(&chars[from..to].iter().collect::<String>())));
        from=to;
    }
    out
}
pub fn paragraph_html(run:&TypeRun)->String {
    use crate::geom::{TextAlign,LastLine,BreakMode};
    let mut base=0;let mut html=String::new();
    for text in run.content.split('\n') {
        let p=paragraph_style(run,base);
        let (align,last)=match p.align {TextAlign::Start=>("left","auto"),TextAlign::Center=>("center","auto"),TextAlign::End=>("right","auto"),TextAlign::Justify{last}=>("justify",match last{LastLine::Start=>"left",LastLine::Center=>"center",LastLine::End=>"right",LastLine::Justify=>"justify"})};
        html.push_str(&format!("<span style=\"display:block;min-height:1em;text-align:{align};text-align-last:{last};{}white-space:{};word-break:{};overflow-wrap:{};hyphens:{}\">{}</span>",paragraph_spacing_css(run,&p),if p.break_mode==BreakMode::KeepAll{"pre"}else{"pre-wrap"},if p.break_mode==BreakMode::BreakAll{"break-all"}else{"normal"},if p.overflow_wrap{"anywhere"}else{"normal"},if p.hyphenate{"auto"}else{"manual"},styled_html(run,base,text)));
        base+=text.chars().count()+1;
    }
    html
}
#[cfg(test)]
mod paragraph_export_spacing_tests {
    use super::*;
    #[test]
    fn paragraph_export_preserves_desired_spacing_and_adds_tracking() {
        let mut run = TypeRun { content: "one two\nthree four".into(), font: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/fonts/EBGaramond.ttf").into(), px: 20., tracking: 2., ..Default::default() };
        run.update_paragraphs(0, 0, |p| { p.word_spacing=[80.,150.,200.]; p.letter_spacing=[0.,10.,20.]; });
        run.update_paragraphs(8, 8, |p| { p.word_spacing=[50.,75.,100.]; p.letter_spacing=[-10.,-5.,0.]; });
        let html=paragraph_html(&run);
        assert!(html.contains("--oma-paragraph-letter-spacing:0.1em;letter-spacing:calc(0.1em + var(--oma-paragraph-letter-spacing))"));
        assert!(html.contains("--oma-paragraph-letter-spacing:-0.05em;"));
        for paragraph in &run.paragraphs {
            let mut probe=run.clone(); probe.content="x x".into(); probe.paragraphs=vec![crate::geom::ParagraphStyle { start:0, ..paragraph.clone() }];
            let laid=compose(&probe); let actual=laid[0].glyphs.iter().find(|g|g.is_space).unwrap().advance;
            probe.paragraphs.clear(); let normal=compose(&probe)[0].glyphs.iter().find(|g|g.is_space).unwrap().advance;
            let word_extra=(actual-normal-run.px*paragraph.letter_spacing[1]/100.)/run.px;
            let css=paragraph_spacing_css(&run,paragraph); let emitted=css.split("word-spacing:").nth(1).unwrap().trim_end_matches("em;").parse::<f32>().unwrap();
            assert!((emitted-word_extra).abs()<0.00001,"HTML spacing {emitted} differs from composed {word_extra}");
        }
    }
}
pub fn glyph_count(run:&TypeRun)->usize {compose(run).iter().map(|l|l.glyphs.len()).sum()}

/// Scan ~/Projects for `next/font/google` imports and return the most frequent
/// family (e.g. “Inter”). Underscores in the identifier are turned into spaces
/// so `Noto_Sans_Mono` → “Noto Sans Mono”.
/// Resolve the preferred default font file: the max95 family, then the omarchy
/// desktop font, then Inter, then the first system font. It never reads the
/// user's project folders (that crawl cost ~9 s on a cold start).
pub fn preferred_default_path() -> Option<PathBuf> {
    // 1. Custom max font from uncap.us (max95 for headings/type tool)
    if let Some(p) = find_installed_for_family("max") {
        return Some(p);
    }
    // If the family is a Google Font but not installed, still return None so
    // the caller can suggest a download. The UI will show “Inter not installed”.
    // 3. Omarchy desktop font (e.g. "JetBrainsMono Nerd Font") – try to find its file.
    if let Ok(out) = std::process::Command::new("omarchy")
        .args(["font", "current"])
        .output()
        && out.status.success()
    {
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !name.is_empty()
            && let Some(p) = find_installed_for_family(&name)
        {
            return Some(p);
        }
    }
    // 4. Hard-coded Inter – the most common branding sans in the user's repos.
    if let Some(p) = find_installed_for_family("Inter") {
        return Some(p);
    }
    // 5. First system font
    default_path()
}

fn find_installed_for_family(family: &str) -> Option<PathBuf> {
    let q = family.to_ascii_lowercase();
    // Prefer exact family name match, then substring.
    let all = all_fonts_cached();
    if let Some(f) = all.iter().find(|f| f.name.to_ascii_lowercase() == q) {
        return Some(f.path.clone());
    }
    if let Some(f) = all
        .iter()
        .find(|f| f.name.to_ascii_lowercase().contains(&q))
    {
        return Some(f.path.clone());
    }
    // Also try file-stem contains.
    for f in all.iter() {
        let stem = f
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if stem.contains(&q.replace(' ', "").to_ascii_lowercase())
            || stem.contains(&q.replace(' ', "_").to_ascii_lowercase())
            || q.contains(&stem.replace('-', " "))
        {
            return Some(f.path.clone());
        }
    }
    None
}

/// The family the app defaults to; shown in the status hint without enumerating fonts.
pub fn preferred_default_family_name() -> Option<String> {
    Some("max95".into())
}

static DEFAULT_FONT: OnceLock<String> = OnceLock::new();

/// The resolved default font path, computed once. Blocks only if the background
/// font index is still being built. Returns "" when no font is installed.
pub fn default_font() -> &'static str {
    DEFAULT_FONT.get_or_init(|| {
        preferred_default_path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
    })
}

pub fn available() -> bool {
    default_path().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Geom;

    fn run(content: &str) -> TypeRun {
        TypeRun {
            content: content.into(),
            px: 48.0,
            ..TypeRun::default()
        }
    }

    #[test]
    fn svg_type_outline_flattening_respects_curved_edge_error() {
        use ab_glyph::{OutlineCurve, point};
        let curves = [
            OutlineCurve::Quad(point(0., 0.), point(0., 64.), point(64., 64.)),
            OutlineCurve::Cubic(point(64., 64.), point(80., 64.), point(80., 0.), point(128., 0.)),
            OutlineCurve::Line(point(128., 0.), point(0., 0.)),
        ];
        let tolerance = 1. / 64.;
        let contours = flatten_outline(&curves, tolerance);
        assert_eq!(contours.len(), 1);
        assert!(contours[0].len() <= 2050);
        for i in 0..=512 {
            let t = i as f32 / 512.;
            let quad = Pt::new(64. * t * t, 128. * t - 64. * t * t);
            let cubic = crate::geom::eval_cubic(Pt::new(64., 64.), Pt::new(80., 64.), Pt::new(80., 0.), Pt::new(128., 0.), t);
            for point in [quad, cubic] {
                let error = contours[0].windows(2).map(|p| crate::geom::seg_dist(point, p[0], p[1])).fold(f32::INFINITY, f32::min);
                assert!(error <= tolerance * 1.01, "curve edge error={error}");
            }
        }
    }

    #[test]
    fn shapes_when_font_exists() {
        let r = run("Ag");
        let sub = shape(&r);
        assert!(!sub.is_empty());
        for sp in &sub {
            assert!(sp.len() >= 3);
        }
        let (w, h) = measure(&r);
        assert!(w > 10.0 && h > 10.0);
    }

    #[test]
    fn fill_contours_writes_cache() {
        let mut g = Geom::Text(run("Type"));
        fill_contours(&mut g);
        let Geom::Text(t) = g else { panic!("text") };
        assert!(!t.contours.is_empty());
    }

    #[test]
    fn caret_walks_forward() {
        let r = run("Hi");
        let a = caret_pt(&r, 0);
        let b = caret_pt(&r, 2);
        assert!(b.x > a.x);
        assert_eq!(hit_char(&r, a), 0);
    }

    #[test]
    fn wrapped_paragraph_keeps_source_and_caret_positions() {
        let mut r = run("A quiet place to make excellent work.\nCafé 東京");
        r.px = 20.0;
        r.wrap_width = Some(110.0);
        let rows = compose(&r);
        assert!(rows.len() > 2);
        assert_eq!(
            rows.iter().map(|l| l.text.as_str()).collect::<String>(),
            r.content.replace('\n', "")
        );
        for line in rows.iter() {
            let p = caret_pt(&r, line.start);
            assert_eq!(hit_char(&r, p), line.start);
        }
        let before = r.content.clone();
        let h = measure(&r).1;
        r.wrap_width = Some(220.0);
        assert!(measure(&r).1 < h);
        assert_eq!(r.content, before);
        assert_eq!(r.px, 20.0);
    }

    #[test]
    fn liga_reduces_fi_when_font_supports_it() {
        let on = TypeRun {
            liga: true,
            ..run("fi")
        };
        let off = TypeRun {
            liga: false,
            ..run("fi")
        };
        let a = glyph_count(&on);
        let b = glyph_count(&off);
        assert!(a >= 1 && b >= 1);
        // If the face has an `fi` ligature, on < off; otherwise they match.
        assert!(a <= b);
    }

    #[test]
    fn old_json_defaults_kern_liga_on() {
        let g: Geom = serde_json::from_str(
            r#"{"Text":{"origin":{"x":0.0,"y":0.0},"content":"Hi","px":24.0,"tracking":0.0}}"#,
        )
        .unwrap();
        let Geom::Text(t) = g else { panic!("text") };
        assert!(t.kern && t.liga);
        assert!(!t.tnum && !t.smcp);
        assert!(t.font.is_empty());
    }

}
