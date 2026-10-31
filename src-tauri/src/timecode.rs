//! Timestamps as people type them: "75", "1:15", "1:02:10.5", and ranges
//! like "1:02:10-1:03:45 the clutch".

/// "75" → 75, "1:15" → 75, "1:02:10.5" → 3730.5. Minutes and seconds after the
/// first field must be under 60.
pub fn parse_time(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let mut total = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let v: f64 = part
            .parse()
            .ok()
            .filter(|v: &f64| *v >= 0.0 && v.is_finite())?;
        if i > 0 && v >= 60.0 {
            return None;
        }
        total = total * 60.0 + v;
    }
    Some(total)
}

/// One range: start and end in seconds, plus whatever followed it on the line.
#[derive(Clone, Debug, PartialEq)]
pub struct Range {
    pub start: f64,
    pub end: f64,
    pub label: Option<String>,
}

/// "1:02:10-1:03:45 the clutch" → 3730..3825, label "the clutch".
pub fn parse_range(line: &str) -> Result<Range, String> {
    let example = "Use start-end, like 1:02:10-1:03:45.";
    let (a, b) = line
        .trim()
        .split_once('-')
        .ok_or_else(|| format!("\"{}\" isn't a time range. {example}", line.trim()))?;
    let b = b.trim_start();
    let (end_token, label) = match b.split_once(char::is_whitespace) {
        Some((end, rest)) => (end, Some(rest.trim().to_string()).filter(|l| !l.is_empty())),
        None => (b, None),
    };
    let start = parse_time(a).ok_or_else(|| format!("\"{}\" isn't a time. {example}", a.trim()))?;
    let end =
        parse_time(end_token).ok_or_else(|| format!("\"{end_token}\" isn't a time. {example}"))?;
    if end <= start {
        return Err(format!(
            "The end ({}) must be after the start ({}).",
            end_token,
            a.trim()
        ));
    }
    Ok(Range { start, end, label })
}

/// Parses every non-empty line, naming the line that's wrong.
pub fn parse_ranges(lines: &[String]) -> Result<Vec<Range>, String> {
    const MAX_RANGES: usize = 100;
    if lines.iter().filter(|l| !l.trim().is_empty()).count() > MAX_RANGES {
        return Err(format!("Use at most {MAX_RANGES} ranges at a time."));
    }
    lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| parse_range(l).map_err(|e| format!("Line {}: {e}", i + 1)))
        .collect()
}

/// Seconds for ffmpeg and yt-dlp, without trailing zeros: 760 → "760", 3730.5 → "3730.5".
pub fn secs(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// For file names: 3730 → "01-02-10", 75.5 → "00-01-15".
pub fn file_stamp(v: f64) -> String {
    let s = v.floor() as u64;
    format!("{:02}-{:02}-{:02}", s / 3600, s % 3600 / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_and_ranges() {
        assert_eq!(parse_time("75"), Some(75.0));
        assert_eq!(parse_time("1:15"), Some(75.0));
        assert_eq!(parse_time("1:02:10.5"), Some(3730.5));
        assert_eq!(parse_time("1:75"), None);
        assert_eq!(parse_time("1::2"), None);
        assert_eq!(parse_time("-5"), None);
        let r = parse_range("1:02:10-1:03:45 the clutch").unwrap();
        assert_eq!(
            (r.start, r.end, r.label.as_deref()),
            (3730.0, 3825.0, Some("the clutch"))
        );
        assert_eq!(parse_range(" 12:40 - 15:05 ").unwrap().label, None);
        assert!(
            parse_range("15:05-12:40")
                .unwrap_err()
                .contains("must be after")
        );
        assert!(
            parse_range("intro")
                .unwrap_err()
                .contains("isn't a time range")
        );
        assert!(
            parse_ranges(&["1-2".into(), "".into(), "bad".into()])
                .unwrap_err()
                .starts_with("Line 2")
        );
        assert_eq!(secs(3730.5), "3730.5");
        assert_eq!(secs(760.0), "760");
        assert_eq!(file_stamp(3730.9), "01-02-10");
    }
}
