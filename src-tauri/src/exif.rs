//! Running exiftool, and what it says about an image.
//!
//! On Windows, exiftool reads its command line in the system code page, so a
//! file name with characters outside it (emoji, Japanese…) arrives mangled.
//! Every run therefore passes its arguments in a UTF-8 argument file, with
//! `-charset filename=utf8` so exiftool reads the names back correctly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::tools::{command, run};

/// Writes `args` one per line to a new file in `dir` and returns the arguments
/// that make exiftool read it. Delete the file when the run is over.
pub fn argfile(dir: &Path, args: &[String]) -> Result<(Vec<String>, PathBuf), String> {
    if args.iter().any(|a| a.contains(['\n', '\r'])) {
        return Err("A file name contains a line break, which exiftool can't handle.".into());
    }
    let path = dir.join(format!("exiftool-{}.args", uuid::Uuid::new_v4()));
    std::fs::write(&path, args.join("\n") + "\n")
        .map_err(|e| format!("Couldn't prepare exiftool: {e}"))?;
    let launch = vec![
        "-charset".into(),
        "filename=utf8".into(),
        "-@".into(),
        path.display().to_string(),
    ];
    Ok((launch, path))
}

/// What exiftool found in an image, boiled down to what matters to a person.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMeta {
    /// GPS coordinates are recorded.
    pub gps: bool,
    /// e.g. "Apple iPhone 15 Pro"
    pub camera: Option<String>,
    /// e.g. "Adobe Photoshop 26.0"
    pub software: Option<String>,
    /// IPTC marks it as made by generative AI.
    pub ai_marked: bool,
    /// Metadata fields beyond the basic image structure.
    pub fields: usize,
    /// EXIF orientation (1–8) if set.
    pub orientation: Option<u8>,
    /// Carries a colour profile.
    pub icc: bool,
    /// A C2PA (JUMBF) manifest.
    pub c2pa: bool,
}

/// Groups that describe the file itself, not anything about who or how.
/// ("Adobe" is the JPEG colour-transform marker, not Adobe's editing history.)
const STRUCTURE_GROUPS: &[&str] = &[
    "Adobe",
    "SourceFile",
    "ExifTool",
    "System",
    "File",
    "Composite",
    "JFIF",
    "RIFF",
    "APP14",
];
/// Structural tags exiftool reports under content groups. Orientation is shown
/// on its own and kept by a strip on purpose.
const STRUCTURE_TAGS: &[&str] = &[
    "Orientation",
    "YCbCrPositioning",
    "XResolution",
    "YResolution",
    "ResolutionUnit",
    "ImageWidth",
    "ImageHeight",
    "BitDepth",
    "ColorType",
    "Compression",
    "Filter",
    "Interlace",
    "PixelsPerUnitX",
    "PixelsPerUnitY",
    "PixelUnits",
    "Gamma",
    "SRGBRendering",
    "BackgroundColor",
    "Palette",
    "Transparency",
    "VP8Version",
    "HorizontalScale",
    "VerticalScale",
    "WebP_Flags",
    "AlphaPreprocessing",
    "AlphaFiltering",
    "AlphaCompression",
];

pub fn image_meta(exiftool: &Path, temp_dir: &Path, file: &Path) -> Result<ImageMeta, String> {
    let (launch, argfile) = argfile(
        temp_dir,
        &["-j", "-n", "-G1", "-a", "-q", "-q"]
            .map(String::from)
            .into_iter()
            .chain([file.display().to_string()])
            .collect::<Vec<_>>(),
    )?;
    let out = run(
        command(exiftool).args(&launch),
        std::time::Duration::from_secs(60),
    );
    let _ = std::fs::remove_file(&argfile);
    let out = out.map_err(|e| format!("Couldn't start exiftool: {e}"))?;
    let json: Vec<BTreeMap<String, serde_json::Value>> = serde_json::from_slice(&out.stdout)
        .map_err(|_| "exiftool couldn't read this image.".to_string())?;
    Ok(summarize(json.into_iter().next().unwrap_or_default()))
}

fn summarize(tags: BTreeMap<String, serde_json::Value>) -> ImageMeta {
    let text = |key: &str| {
        tags.get(key).and_then(|v| match v {
            serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    };
    let group = |key: &str| key.split_once(':').map_or(key, |(g, _)| g).to_string();

    let camera = match (text("IFD0:Make"), text("IFD0:Model")) {
        (Some(make), Some(model)) if model.starts_with(&make) => Some(model),
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (make, model) => make.or(model),
    };
    let software = text("IFD0:Software")
        .or_else(|| text("XMP-xmp:CreatorTool"))
        .or_else(|| text("PNG:Software"));
    let fields = tags
        .keys()
        .filter(|k| {
            let g = group(k);
            let tag = k.split_once(':').map_or(k.as_str(), |(_, t)| t);
            !STRUCTURE_GROUPS.contains(&g.as_str())
                && !g.starts_with("ICC")
                && !STRUCTURE_TAGS.contains(&tag)
        })
        .count();

    ImageMeta {
        gps: tags.keys().any(|k| k.ends_with(":GPSLatitude")),
        camera,
        software,
        ai_marked: tags.iter().any(|(k, v)| {
            k.ends_with(":DigitalSourceType")
                && v.as_str()
                    .is_some_and(|s| s.contains("trainedAlgorithmicMedia"))
        }),
        fields,
        orientation: tags
            .get("IFD0:Orientation")
            .and_then(|v| v.as_u64())
            .and_then(|o| u8::try_from(o).ok()),
        icc: tags.keys().any(|k| group(k).starts_with("ICC")),
        c2pa: tags.keys().any(|k| group(k) == "JUMBF"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_a_phone_photo() {
        let json = serde_json::json!({
            "SourceFile": "x.jpg", "ExifTool:ExifToolVersion": 13.59, "File:FileSize": 1000,
            "IFD0:Make": "Apple", "IFD0:Model": "iPhone 15 Pro", "IFD0:Orientation": 6,
            "IFD0:Software": "Adobe Photoshop 26.0", "GPS:GPSLatitude": 51.5, "GPS:GPSLatitudeRef": "N",
            "ICC-header:ProfileVersion": 528, "Composite:ImageSize": "640 480",
            "XMP-iptcExt:DigitalSourceType": "http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia",
            "JUMBF:JUMDLabel": "c2pa"
        });
        let tags: BTreeMap<String, serde_json::Value> = serde_json::from_value(json).unwrap();
        let m = summarize(tags);
        assert_eq!(m.camera.as_deref(), Some("Apple iPhone 15 Pro"));
        assert_eq!(m.software.as_deref(), Some("Adobe Photoshop 26.0"));
        assert!(m.gps && m.icc && m.c2pa && m.ai_marked);
        assert_eq!(m.orientation, Some(6));
        // Make, Model, Software, 2 GPS, DigitalSourceType, JUMDLabel (not Orientation)
        assert_eq!(m.fields, 7);
    }
}
