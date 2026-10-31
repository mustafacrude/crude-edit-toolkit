//! Image tools: strip metadata without recompressing (exiftool), convert to
//! WebP (ffmpeg's libwebp), which strips by default too (BRIEF §9), and convert
//! what Premiere can't import (WebP, AVIF, HEIC, SVG…) to PNG.
//!
//! Two things keep photos looking right. The EXIF orientation tag survives a
//! strip, because the pixels are stored sideways and the tag is what turns them
//! upright. And the colour profile is kept unless asked otherwise: it isn't
//! personal, and dropping it shifts the colours of wide-gamut phone photos.

use std::path::Path;

use super::schema::{Field, ShowWhen};
use super::{Accepts, Category, JobInput, Lane, Module, Plan, PlanContext, Step, Suggestion};
use crate::exif::{self, ImageMeta};
use crate::media::{self, MediaInfo};
use crate::progress::ProgressKind;
use crate::tools::Tool;

/// What a strip removes from this image, in words, or None if nothing personal.
fn removable(meta: &ImageMeta, c2pa: bool) -> Option<String> {
    let mut parts = Vec::new();
    if meta.gps {
        parts.push("the location");
    }
    if meta.camera.is_some() {
        parts.push("camera details");
    }
    if meta.software.is_some() {
        parts.push("editing software");
    }
    if c2pa || meta.c2pa {
        parts.push("Content Credentials (C2PA)");
    }
    if meta.ai_marked {
        parts.push("the AI-generated label");
    }
    if parts.is_empty() {
        return (meta.fields > 0).then(|| format!("{} metadata fields", meta.fields));
    }
    let last = parts.pop().unwrap_or_default();
    Some(match parts.as_slice() {
        [] => last.into(),
        [one] => format!("{one} and {last}"),
        many => format!("{}, and {last}", many.join(", ")),
    })
}

fn image_info(ctx: &PlanContext, file: &Path) -> Result<(MediaInfo, ImageMeta), String> {
    let info = media::probe(&ctx.tool(Tool::Ffprobe)?, file)?;
    if !info.is_image() {
        return Err("This isn't an image. Video tools are on the Video screen.".into());
    }
    let meta = exif::image_meta(&ctx.tool(Tool::Exiftool)?, ctx.temp_dir, file)?;
    Ok((info, meta))
}

/// An exiftool step reading `args` from an argument file, which joins the plan's scratch files.
fn exiftool_step(
    ctx: &PlanContext,
    plan: &mut Plan,
    label: &str,
    args: Vec<String>,
) -> Result<(), String> {
    let (launch, argfile) = exif::argfile(ctx.temp_dir, &args)?;
    plan.temp_files.push(argfile);
    plan.steps.push(Step {
        label: label.into(),
        tool: Tool::Exiftool,
        args: launch,
        progress: ProgressKind::None,
        redact: vec![],
    });
    Ok(())
}

/// Pixel formats that can carry transparency. Palette PNGs may (via tRNS).
fn has_alpha(pix_fmt: &str) -> bool {
    pix_fmt == "pal8"
        || pix_fmt.starts_with("ya")
        || pix_fmt.starts_with("yuva")
        || pix_fmt.starts_with("gbrap")
        || ["rgba", "bgra", "argb", "abgr"]
            .iter()
            .any(|p| pix_fmt.starts_with(p))
}

fn extension(file: &Path) -> String {
    file.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub struct StripImage;

impl Module for StripImage {
    fn id(&self) -> &'static str {
        "strip-image"
    }

    fn name(&self) -> &'static str {
        "Strip metadata"
    }

    fn description(&self) -> &'static str {
        "Removes location, camera details, editing history and Content Credentials (C2PA) from images without recompressing them, so the picture is identical. Keeps the rotation so photos stay upright."
    }

    fn category(&self) -> Category {
        Category::Image
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn accepts(&self) -> Accepts {
        Accepts::Image
    }

    fn options_schema(&self) -> Vec<Field> {
        vec![Field::Toggle {
            key: "keepColorProfile",
            label: "Keep the colour profile",
            default: true,
            help: Some(
                "Colour profiles aren't personal. Removing one can make wide-gamut phone photos look duller.",
            ),
        }]
    }

    fn suggest(&self, info: &MediaInfo) -> Option<Suggestion> {
        let meta = info.image.as_ref()?;
        let what = removable(meta, info.c2pa == Some(true))?;
        let urgent = meta.gps || meta.c2pa || info.c2pa == Some(true) || meta.ai_marked;
        Some(Suggestion {
            reason: format!("Removes {what} without recompressing. Takes a second."),
            priority: if urgent { 80 } else { 50 },
        })
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let file = input.file().ok_or("This tool works on files, not links.")?;
        image_info(ctx, file)?;
        let keep_icc = options
            .get("keepColorProfile")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let output = ctx.output_next_to(file, "_clean", &extension(file));

        let mut args: Vec<String> = ["-all=", "-JUMBF:all=", "-tagsfromfile", "@", "-Orientation"]
            .map(String::from)
            .to_vec();
        if keep_icc {
            args.push("-ICC_Profile".into());
        }
        args.extend([
            "-o".into(),
            output.display().to_string(),
            file.display().to_string(),
        ]);

        let mut plan = Plan {
            title: format!("Strip metadata: {}", input.short_name()),
            outputs: vec![output],
            ..Default::default()
        };
        exiftool_step(ctx, &mut plan, "Stripping", args)?;
        Ok(plan)
    }
}

pub struct ConvertWebp;

impl Module for ConvertWebp {
    fn id(&self) -> &'static str {
        "webp"
    }

    fn name(&self) -> &'static str {
        "Convert to WebP"
    }

    fn description(&self) -> &'static str {
        "Converts images to WebP, usually a third smaller than JPEG at the same quality. Keeps transparency. Removes metadata and C2PA unless you say otherwise."
    }

    fn category(&self) -> Category {
        Category::Image
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn accepts(&self) -> Accepts {
        Accepts::Image
    }

    fn options_schema(&self) -> Vec<Field> {
        vec![
            Field::Number {
                key: "quality",
                label: "Quality",
                min: 1.0,
                max: 100.0,
                step: 1.0,
                unit: "",
                default: 85.0,
                help: Some("85 looks the same as the original for almost every image."),
                show_when: Some(ShowWhen {
                    key: "lossless",
                    value: "false",
                }),
                slider: true,
            },
            Field::Toggle {
                key: "lossless",
                label: "Lossless",
                default: false,
                help: Some(
                    "Pixel-perfect. Best for screenshots and graphics; photos come out bigger.",
                ),
            },
            Field::Toggle {
                key: "strip",
                label: "Remove metadata and C2PA",
                default: true,
                help: Some(
                    "Location, camera details and Content Credentials. The colour profile is kept.",
                ),
            },
        ]
    }

    fn suggest(&self, info: &MediaInfo) -> Option<Suggestion> {
        let codec = info.streams.first()?.codec();
        (info.image.is_some() && matches!(codec, "png" | "mjpeg" | "tiff" | "bmp")).then(|| Suggestion {
            reason: "A smaller file, usually a third smaller than JPEG at the same quality, with the metadata removed.".into(),
            priority: 40,
        })
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let file = input.file().ok_or("This tool works on files, not links.")?;
        let (info, meta) = image_info(ctx, file)?;
        let lossless = options
            .get("lossless")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let strip = options
            .get("strip")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let quality = options
            .get("quality")
            .and_then(|v| v.as_f64())
            .unwrap_or(85.0)
            .clamp(1.0, 100.0)
            .round();

        let suffix = if extension(file) == "webp" {
            "_converted"
        } else {
            ""
        };
        let output = ctx.output_next_to(file, suffix, "webp");
        // ffmpeg applies the EXIF orientation itself, so the pixels come out
        // upright, and writes no metadata of its own.
        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-nostdin".into(),
            "-n".into(),
            "-i".into(),
            file.display().to_string(),
            "-frames:v".into(),
            "1".into(),
            "-map_metadata".into(),
            "-1".into(),
            "-c:v".into(),
            "libwebp".into(),
            "-compression_level".into(),
            "6".into(),
        ];
        if lossless {
            args.extend(["-lossless", "1"].map(String::from));
        } else {
            args.extend(["-quality".into(), quality.to_string()]);
            // Left alone, ffmpeg picks plain 4:2:0 for lossy WebP and the
            // transparency is lost; ask for the format that carries it.
            let pix = info
                .streams
                .first()
                .and_then(|s| s.pix_fmt.as_deref())
                .unwrap_or("");
            if has_alpha(pix) {
                args.extend(["-pix_fmt", "yuva420p"].map(String::from));
            }
        }
        args.push(output.display().to_string());

        let mut plan = Plan {
            title: format!("Convert to WebP: {}", input.short_name()),
            steps: vec![Step {
                label: "Converting".into(),
                tool: Tool::Ffmpeg,
                args,
                progress: ProgressKind::None,
                redact: vec![],
            }],
            outputs: vec![output.clone()],
            ..Default::default()
        };

        // Copy back what should survive: the colour profile, or (when keeping
        // metadata) everything except the orientation, since the pixels are
        // already upright, and the C2PA manifest, which no longer matches them.
        let source = file.display().to_string();
        let target = output.display().to_string();
        if !strip {
            let args = [
                "-tagsfromfile",
                &source,
                "-all:all",
                "--Orientation",
                "--JUMBF:all",
                "-overwrite_original",
                &target,
            ];
            exiftool_step(
                ctx,
                &mut plan,
                "Copying metadata",
                args.map(String::from).to_vec(),
            )?;
        } else if meta.icc {
            let args = [
                "-tagsfromfile",
                &source,
                "-ICC_Profile",
                "-overwrite_original",
                &target,
            ];
            exiftool_step(
                ctx,
                &mut plan,
                "Keeping the colour profile",
                args.map(String::from).to_vec(),
            )?;
        }
        Ok(plan)
    }
}

/// Formats Premiere can't import, and how they're usually called.
fn premiere_cant_open(codec: &str) -> Option<&'static str> {
    Some(match codec {
        "webp" => "WebP",
        "av1" => "AVIF",
        "hevc" => "HEIC",
        "svg" => "SVG",
        "jpegxl" => "JPEG XL",
        "qoi" => "QOI",
        _ => return None,
    })
}

/// Vectors are drawn with their longest side this long, so logos stay sharp at 4K.
const SVG_LONGEST: u32 = 3840;

/// ffmpeg options drawing an SVG at 4K in its own proportions. Both sides are
/// given: with only one, ffmpeg keeps the other at the SVG's nominal size.
pub fn svg_size_args(info: &MediaInfo) -> Vec<String> {
    let first = info.streams.first();
    let (w, h) = (
        first.and_then(|s| s.width).unwrap_or(1).max(1) as u64,
        first.and_then(|s| s.height).unwrap_or(1).max(1) as u64,
    );
    let long = SVG_LONGEST as u64;
    let (width, height) = if w >= h {
        (long, (long * h / w).max(1))
    } else {
        ((long * w / h).max(1), long)
    };
    vec![
        "-width".into(),
        width.to_string(),
        "-height".into(),
        height.to_string(),
    ]
}

pub struct PremiereImage;

impl Module for PremiereImage {
    fn id(&self) -> &'static str {
        "png"
    }

    fn name(&self) -> &'static str {
        "Make Premiere-ready (PNG)"
    }

    fn description(&self) -> &'static str {
        "Converts WebP, AVIF, HEIC, SVG and JPEG XL images, which Premiere can't import, to PNG. Lossless, keeps transparency, and turns phone photos upright."
    }

    fn category(&self) -> Category {
        Category::Image
    }

    fn lane(&self) -> Lane {
        Lane::Process
    }

    fn accepts(&self) -> Accepts {
        Accepts::Image
    }

    fn options_schema(&self) -> Vec<Field> {
        Vec::new()
    }

    fn suggest(&self, info: &MediaInfo) -> Option<Suggestion> {
        let name = premiere_cant_open(info.streams.first()?.codec())?;
        info.is_image().then(|| Suggestion {
            reason: format!(
                "Premiere can't import {name} images. Saves a PNG it can, losing nothing."
            ),
            priority: 90,
        })
    }

    fn plan(
        &self,
        ctx: &PlanContext,
        input: &JobInput,
        _options: &serde_json::Value,
    ) -> Result<Plan, String> {
        let file = input.file().ok_or("This tool works on files, not links.")?;
        let (info, meta) = image_info(ctx, file)?;
        let codec = info.streams.first().map(|s| s.codec()).unwrap_or("unknown");
        let suffix = if extension(file) == "png" {
            "_converted"
        } else {
            ""
        };
        let output = ctx.output_next_to(file, suffix, "png");

        let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-n"]
            .map(String::from)
            .to_vec();
        if codec == "svg" {
            args.extend(svg_size_args(&info));
        }
        // ffmpeg turns the picture upright and puts a HEIC's tiles together itself.
        args.extend([
            "-i".into(),
            file.display().to_string(),
            "-frames:v".into(),
            "1".into(),
            "-map_metadata".into(),
            "-1".into(),
            "-update".into(),
            "1".into(),
            output.display().to_string(),
        ]);

        let mut plan = Plan {
            title: format!("Make Premiere-ready: {}", input.short_name()),
            steps: vec![Step {
                label: "Converting".into(),
                tool: Tool::Ffmpeg,
                args,
                progress: ProgressKind::None,
                redact: vec![],
            }],
            outputs: vec![output.clone()],
            ..Default::default()
        };
        if meta.icc {
            let (source, target) = (file.display().to_string(), output.display().to_string());
            let args = [
                "-tagsfromfile",
                &source,
                "-ICC_Profile",
                "-overwrite_original",
                &target,
            ];
            exiftool_step(
                ctx,
                &mut plan,
                "Keeping the colour profile",
                args.map(String::from).to_vec(),
            )?;
        }
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_what_a_strip_removes() {
        let meta = ImageMeta {
            gps: true,
            camera: Some("Apple iPhone 15 Pro".into()),
            fields: 9,
            ..Default::default()
        };
        assert_eq!(
            removable(&meta, true).unwrap(),
            "the location, camera details, and Content Credentials (C2PA)"
        );
        let plain = ImageMeta {
            fields: 3,
            ..Default::default()
        };
        assert_eq!(removable(&plain, false).unwrap(), "3 metadata fields");
        assert!(removable(&ImageMeta::default(), false).is_none());
    }

    #[test]
    fn spots_transparency() {
        assert!(
            has_alpha("rgba") && has_alpha("rgba64be") && has_alpha("ya8") && has_alpha("pal8")
        );
        assert!(!has_alpha("rgb24") && !has_alpha("yuvj420p") && !has_alpha("gray"));
    }

    #[test]
    fn svgs_keep_their_shape() {
        let svg = |w, h| MediaInfo {
            streams: vec![crate::media::Stream {
                width: Some(w),
                height: Some(h),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(
            svg_size_args(&svg(100, 100)).join(" "),
            "-width 3840 -height 3840"
        );
        assert_eq!(
            svg_size_args(&svg(400, 100)).join(" "),
            "-width 3840 -height 960"
        );
        assert_eq!(
            svg_size_args(&svg(50, 200)).join(" "),
            "-width 960 -height 3840"
        );
    }

    #[test]
    fn names_what_premiere_cant_open() {
        assert_eq!(premiere_cant_open("av1"), Some("AVIF"));
        assert_eq!(premiere_cant_open("hevc"), Some("HEIC"));
        assert_eq!(premiere_cant_open("png"), None);
        assert_eq!(premiere_cant_open("mjpeg"), None);
    }
}
