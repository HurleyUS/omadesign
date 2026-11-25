//! Render native Omadesign documents for file manager thumbnails and previews.

#[path = "oma_preview/cache.rs"]
mod cache;

use image::{DynamicImage, ImageEncoder, ImageFormat, RgbaImage, imageops::FilterType};
use omadesign::{compositor, document::Document, project};
use serde_json::json;
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    if let Err(error) = run() {
        eprintln!("oma-preview: {error}");
        println!("{}", json!({"error": error}));
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("");
    if mode == "--version" {
        println!("oma-preview 0.1.0 (Omadesign 0.6.3)");
        return Ok(());
    }
    if !matches!(mode, "thumbnail" | "thumbnail-cached" | "preview") {
        return Err("Use thumbnail --input FILE --output PNG --size 256, thumbnail-cached --input FILE, or preview --input FILE".into());
    }
    let input = PathBuf::from(value(&args, "--input")?);
    let edge = args
        .windows(2)
        .find(|pair| pair[0] == "--size")
        .map(|pair| pair[1].parse::<u32>())
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(if mode == "preview" { 1280 } else { 256 });
    if !(16..=2048).contains(&edge) {
        return Err("Size must be between 16 and 2048 pixels".into());
    }
    if mode == "thumbnail" {
        let output = PathBuf::from(value(&args, "--output")?);
        if input.canonicalize().ok() == output.canonicalize().ok() && output.exists() {
            return Err("The output must differ from the original document".into());
        }
        let doc = load(&input)?;
        let bytes = still(&doc, edge, poster_time(&doc))?;
        fs::write(output, bytes).map_err(|e| e.to_string())?;
        return Ok(());
    }
    let root = cache::root()?;
    let key = cache::key(&input, mode, edge)?;
    let manifest = root.join(format!("{key}.json"));
    if let Some(answer) = cache::answer(&manifest) {
        println!("{answer}");
        return Ok(());
    }
    let gate = File::create(root.join(if mode == "preview" {
        "preview.lock"
    } else {
        "thumbnail.lock"
    }))
    .map_err(|e| e.to_string())?;
    gate.lock().map_err(|e| e.to_string())?;
    if let Some(answer) = cache::answer(&manifest) {
        println!("{answer}");
        return Ok(());
    }
    let doc = load(&input)?;
    let motion = mode == "preview" && !doc.motion.is_empty() && doc.motion.duration > 0.0;
    let output = root.join(format!("{key}.{}", if motion { "mp4" } else { "png" }));
    let temporary = root.join(format!("{key}.{}.part", std::process::id()));
    let rendered = if motion {
        movie(&doc, edge, &temporary)
    } else {
        still(&doc, edge, poster_time(&doc))
            .and_then(|bytes| fs::write(&temporary, bytes).map_err(|e| e.to_string()))
    };
    if let Err(error) = rendered {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if cache::key(&input, mode, edge)? != key {
        let _ = fs::remove_file(&temporary);
        return Err("The document changed while it was rendering. Open the preview again.".into());
    }
    if fs::metadata(&temporary).map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
        let _ = fs::remove_file(&temporary);
        return Err("This preview exceeds the 512 MiB cache limit".into());
    }
    fs::rename(&temporary, &output).map_err(|e| e.to_string())?;
    let answer = json!({"file": output, "kind": if motion { "motion" } else { "static" },
        "width": doc.width, "height": doc.height, "duration": if motion { doc.motion.duration } else { 0.0 },
        "looped": doc.motion.looped, "fps": doc.motion.fps.clamp(1.0, 30.0)}).to_string();
    fs::write(&manifest, &answer).map_err(|e| e.to_string())?;
    cache::trim(&root, &output);
    println!("{answer}");
    Ok(())
}

fn value<'a>(args: &'a [String], flag: &str) -> Result<&'a str, String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].as_str())
        .ok_or_else(|| format!("Missing {flag}"))
}

/// Load a native document without changing it.
///
/// `path` identifies the source and its project font folder. Returns the document
/// with its native rasters and fonts, or an error for unreadable or oversized input.
fn load(path: &Path) -> Result<Document, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
        return Err("Choose a regular .oma document smaller than 128 MiB".into());
    }
    let doc = project::load_from(path)?;
    if !doc.width.is_finite()
        || !doc.height.is_finite()
        || doc.width <= 0.0
        || doc.height <= 0.0
        || doc.width > 32768.0
        || doc.height > 32768.0
    {
        return Err("The document dimensions are outside the preview limit".into());
    }
    Ok(doc)
}

fn poster_time(doc: &Document) -> f32 {
    if doc.motion.is_empty() || !doc.motion.duration.is_finite() {
        0.0
    } else {
        doc.motion.duration.max(0.0) / 3.0
    }
}

/// Render one fitted frame with the native compositor.
///
/// `doc` is the artwork, `edge` bounds its longer side, and `time` is the motion
/// clock in seconds. Returns straight RGBA pixels with the original aspect ratio.
fn frame(doc: &Document, edge: u32, time: f32) -> Result<RgbaImage, String> {
    let scale = (edge as f32 / doc.width.max(doc.height)).min(1.0);
    let pixmap = compositor::render_export_at(doc, scale, time, doc.transparent)?;
    let png = pixmap.encode_png().map_err(|e| e.to_string())?;
    let decoded = image::load_from_memory(&png).map_err(|e| e.to_string())?;
    Ok(if decoded.width().max(decoded.height()) > edge {
        decoded
            .resize(edge, edge, FilterType::Lanczos3)
            .into_rgba8()
    } else {
        decoded.into_rgba8()
    })
}

fn still(doc: &Document, edge: u32, time: f32) -> Result<Vec<u8>, String> {
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(frame(doc, edge, time)?)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(output.into_inner())
}

/// Stream the entire motion timeline into a silent H.264 preview.
///
/// `doc` supplies native animation and `edge` bounds resolution. `output` is a
/// temporary destination. Returns success after ffmpeg has written every frame.
fn movie(doc: &Document, edge: u32, output: &Path) -> Result<(), String> {
    let duration = doc.motion.duration;
    if !duration.is_finite() || duration > 600.0 {
        return Err("Motion previews support timelines up to ten minutes".into());
    }
    let fps = if doc.motion.fps.is_finite() {
        doc.motion.fps.clamp(1.0, 30.0)
    } else {
        30.0
    };
    let count = (duration * fps).ceil().max(1.0) as u32;
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "image2pipe",
            "-framerate",
            &fps.to_string(),
            "-vcodec",
            "png",
            "-i",
            "pipe:0",
            "-an",
            "-vf",
            "pad=ceil(iw/2)*2:ceil(ih/2)*2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
        ])
        .arg(output)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Install ffmpeg for motion previews: {e}"))?;
    let mut pipe = child.stdin.take().ok_or("The video encoder has no input")?;
    let render = (|| {
        for index in 0..count {
            let mut pixels = frame(doc, edge, index as f32 / fps)?;
            for (x, y, pixel) in pixels.enumerate_pixels_mut() {
                let background = if (x / 12 + y / 12) % 2 == 0 {
                    220_u32
                } else {
                    245_u32
                };
                let alpha = u32::from(pixel[3]);
                for channel in &mut pixel.0[..3] {
                    *channel =
                        ((u32::from(*channel) * alpha + background * (255 - alpha)) / 255) as u8;
                }
                pixel[3] = 255;
            }
            image::codecs::png::PngEncoder::new(&mut pipe)
                .write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| e.to_string())?;
        }
        pipe.flush().map_err(|e| e.to_string())
    })();
    drop(pipe);
    if render.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    render?;
    if !status.success() {
        return Err("The video encoder could not finish the preview".into());
    }
    Ok(())
}
