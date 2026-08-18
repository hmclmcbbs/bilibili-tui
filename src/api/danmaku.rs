use anyhow::{Result, anyhow};
use quick_xml::Reader;
use quick_xml::escape::unescape;
use quick_xml::events::Event;

#[derive(Debug, Clone, PartialEq)]
pub struct VideoDanmaku {
    pub time: f64,
    pub text: String,
    pub color: u32,
    /// Bilibili danmaku mode. 1/4/5/6 are rolling/top/bottom text,
    /// 7/8 are positioned (advanced) danmaku carrying a coordinate array.
    pub mode: i32,
    /// Normalized x coordinate (0.0-1.0) for positioned danmaku.
    pub x: Option<f64>,
    /// Normalized y coordinate (0.0-1.0) for positioned danmaku.
    pub y: Option<f64>,
    /// Normalized end x coordinate for moving positioned danmaku.
    pub x2: Option<f64>,
    /// Normalized end y coordinate for moving positioned danmaku.
    pub y2: Option<f64>,
    /// Rotation angle in degrees for positioned danmaku.
    pub rotation: Option<f64>,
    /// Font size hint for positioned danmaku, in the same units as the
    /// p-attribute font size (25 = default). Comes from the XML p[2] field,
    /// or the legacy BAS payload field 3 when the payload is per-mille.
    pub size: Option<f64>,
    /// Display duration in milliseconds for positioned danmaku.
    pub duration_ms: Option<i64>,
    /// Font family hint for positioned danmaku.
    pub font_family: Option<String>,
    /// BAS opacity (0.0 = fully transparent, 1.0 = opaque) for positioned
    /// danmaku. `None` means "use the global renderer opacity".
    pub alpha: Option<f64>,
    /// End value of the BAS opacity fade (e.g. "0.25-0" fades from 0.25 to
    /// 0.0). `Some(alpha)` is the value at the start of the display window.
    /// When absent, the alpha stays constant at `alpha`.
    pub alpha_to: Option<f64>,
    /// BAS border flag (`false` = no outline) for positioned danmaku.
    /// `None` means "use the global stroke width".
    pub border: Option<bool>,
}

/// Parsed BAS payload for a positioned (advanced) danmaku.
struct PositionedPayload {
    x: f64,
    y: f64,
    x2: f64,
    y2: f64,
    rotation: f64,
    size: f64,
    duration_ms: i64,
    text: String,
    font: String,
    alpha: Option<f64>,
    alpha_to: Option<f64>,
    border: Option<bool>,
}

/// Parse the JSON-array payload used by positioned (advanced) danmaku.
/// Example: `[0.32,0.11,"1-1",1.5,"真/n是/n毫/n无/n道/n理",0,0,0.32,0.11,500,0,true,"黑体",1]`
/// Fields (Bilibili BAS format):
///   0: x (0-1 or 0-1000)    1: y (0-1 or 0-1000)
///   2: alpha ("1-1" = from-to, modern) or id (legacy)
///   3: duration seconds (modern) or font-size hint (legacy)
///   4: text (`/n` = newline)
///   5: rotate-z degrees (modern) or color (legacy)
///   6: rotate-y degrees (modern) or rotation (legacy)
///   7: end x    8: end y    9: duration (ms)
///   10: delay    11: border    12: font family    13: unused
fn parse_positioned_payload(
    payload: &str,
    p_font_size: f64,
) -> Option<PositionedPayload> {
    let payload = payload.trim();
    if !payload.starts_with('[') || !payload.ends_with(']') {
        return None;
    }
    let inner = &payload[1..payload.len() - 1];
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                in_string = !in_string;
            }
            ',' if !in_string => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                if in_string || !ch.is_whitespace() {
                    current.push(ch);
                }
            }
        }
    }
    if !current.trim().is_empty() {
        fields.push(current.trim().to_string());
    }
    if fields.len() < 10 {
        return None;
    }
    // Coordinates are either normalized 0-1 decimals (new format) or 0-1000
    // per-mille integers (older Flash-era payloads, e.g. x=1 means 0.1%).
    // If ANY coordinate exceeds 2, the whole payload uses per-mille: divide
    // every coordinate by 1000. This handles legacy x=1/y=2 style values that
    // would otherwise be interpreted as 100%/200% and rendered off-screen.
    let x_raw: f64 = fields[0].parse().ok()?;
    let y_raw: f64 = fields[1].parse().ok()?;
    let x2_raw: f64 = fields
        .get(7)
        .and_then(|value| value.parse().ok())
        .unwrap_or(x_raw);
    let y2_raw: f64 = fields
        .get(8)
        .and_then(|value| value.parse().ok())
        .unwrap_or(y_raw);
    let per_mille = [x_raw, y_raw, x2_raw, y2_raw]
        .iter()
        .any(|value| *value > 2.0);
    let normalize = |value: f64| if per_mille { value / 1000.0 } else { value };
    let x = normalize(x_raw);
    let y = normalize(y_raw);
    let x2 = normalize(x2_raw);
    let y2 = normalize(y2_raw);
    // Field semantics differ across eras. Legacy Flash payloads (per-mille
    // coordinates, detected above) put the color in field 5 and the rotation
    // in field 6. Modern BAS payloads put rotate-z in field 5, rotate-y in
    // field 6, and the color only in the p attribute. The p-attribute color
    // is authoritative in both cases, so only the rotation slot changes.
    let rotation: f64 = if per_mille {
        fields.get(6).and_then(|value| value.parse().ok()).unwrap_or(0.0)
    } else {
        fields.get(5).and_then(|value| value.parse().ok()).unwrap_or(0.0)
    };
    // Field 9 is the display duration in milliseconds. Some payloads leave
    // it at 0 and carry the duration in field 3 (seconds, e.g. 0.3 = 300 ms),
    // which is also where the modern format keeps it. The official player
    // falls back to field 3 when field 9 is zero; without the fallback those
    // comments rendered for only 0.1 s (the Lua minimum) and blinked out.
    let duration: i64 = if per_mille {
        fields[9].parse().unwrap_or(5000)
    } else {
        // Both field 3 (seconds) and field 9 (milliseconds) can carry the
        // display duration in modern BAS; they usually agree (e.g. 2.28s vs
        // 2280ms). But some payloads set field 9 to a wrong/short value while
        // field 3 holds the true duration (cherry pop "第一名": field 3 = 1.85s
        // but field 9 = 650ms). Taking field 9 alone truncates the comment to
        // a blink. Use the LARGER of the two so a malformed field 9 cannot
        // shorten the on-screen lifetime below the official value.
        let from_f9 = fields[9]
            .parse::<i64>()
            .ok()
            .filter(|&ms| ms > 0);
        let from_f3 = fields
            .get(3)
            .and_then(|value| value.parse::<f64>().ok())
            .map(|seconds| (seconds * 1000.0) as i64)
            .filter(|&ms| ms > 0);
        match (from_f3, from_f9) {
            (Some(a), Some(b)) => a.max(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => 5000,
        }
    };
    // Modern payloads carry the duration seconds in field 3 (matching field
    // 9 ms, e.g. 2.35 vs 2350), so field 3 is never a font size there. The
    // font size for modern payloads lives in the p attribute (p[2], e.g. 71);
    // legacy per-mille payloads carry it in field 3 (e.g. 10).
    let size: f64 = if per_mille {
        fields[3].parse().ok().filter(|size: &f64| *size > 0.0).unwrap_or(0.0)
    } else {
        p_font_size
    };
    // Modern BAS carries the opacity as "from-to" (e.g. "1-1" stays opaque,
    // "0.25-0" fades out to transparent, "0.5-0.5" stays half opaque).
    // Legacy payloads use field 2 as the id string, so only parse alpha when
    // it looks like a number (not a legacy id like "1-1" in old payloads...
    // but "1-1" is also the most common modern alpha!). Distinguish by
    // per-mille: legacy ids look like "1-1" too, so only trust field 2 as
    // alpha in modern payloads.
    let (alpha, alpha_to): (Option<f64>, Option<f64>) = if per_mille {
        (None, None)
    } else {
        fields.get(2).and_then(|value| {
            let mut parts = value.split('-');
            let from = parts.next()?.trim().parse::<f64>().ok()?;
            let to = parts
                .next()
                .map(|part| part.trim().parse::<f64>().unwrap_or(from));
            Some((Some(from), to))
        }).unwrap_or((None, None))
    };
    // Field 11 is the border flag (0/1 or false/true) in modern BAS.
    let border: Option<bool> = fields.get(11).and_then(|value| {
        let value = value.trim().to_ascii_lowercase();
        if value == "true" {
            Some(true)
        } else if value == "false" {
            Some(false)
        } else {
            match value.parse::<i64>() {
                Ok(0) => Some(false),
                Ok(_) => Some(true),
                Err(_) => None,
            }
        }
    });
    // Keep `/n` as the newline marker. The Lua renderer escapes the text for
    // ASS first and converts `/n` to `\N` afterwards, so a backslash never
    // survives into the ASS line (escaping `\N` would print a literal `\N`).
    let text = fields[4]
        .replace("\\n", "/n")
        .replace('\n', "/n");
    let font = fields
        .get(12)
        .map(|value| value.replace(['\\', '"'], "").trim().to_string())
        .unwrap_or_default();
    Some(PositionedPayload {
        x,
        y,
        x2,
        y2,
        rotation,
        size,
        duration_ms: duration,
        text,
        font,
        alpha,
        alpha_to,
        border,
    })
}

pub fn parse_xml(xml: &str) -> Result<Vec<VideoDanmaku>> {
    let mut reader = Reader::from_str(xml);
    let mut danmaku = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(event) if event.name().as_ref() == b"d" => {
                let metadata = event
                    .attributes()
                    .flatten()
                    .find(|attribute| attribute.key.as_ref() == b"p")
                    .ok_or_else(|| anyhow!("danmaku entry is missing p metadata"))?
                    .unescape_value()?;
                let mut fields = metadata.split(',');
                let time = fields.next().and_then(|value| value.parse().ok());
                let mode = fields.next().and_then(|value| value.parse::<i32>().ok());
                // p[2] is the font size (25 = default, the same unit the
                // official player uses). Positioned danmaku need it because
                // their BAS payload does not carry a size in modern payloads.
                let p_font_size = fields
                    .next()
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(25.0);
                let color = fields.next().and_then(|value| value.parse().ok());
                let raw_text = reader.read_text(event.name())?;
                let text = unescape(&raw_text)?.into_owned();
                if let (Some(time), Some(color), Some(mode)) = (time, color, mode)
                    && time >= 0.0
                    && !text.is_empty()
                {
                    let mut item = VideoDanmaku {
                        time,
                        text,
                        color,
                        mode,
                        x: None,
                        y: None,
                        x2: None,
                        y2: None,
                        rotation: None,
                        size: None,
                        duration_ms: None,
                        font_family: None,
                        alpha: None,
                        alpha_to: None,
                        border: None,
                    };
                    if matches!(mode, 7 | 8)
                        && let Some(payload) = parse_positioned_payload(&item.text, p_font_size)
                    {
                        item.text = payload.text;
                        item.x = Some(payload.x);
                        item.y = Some(payload.y);
                        item.x2 = Some(payload.x2);
                        item.y2 = Some(payload.y2);
                        item.rotation = Some(payload.rotation);
                        item.size = Some(payload.size);
                        item.duration_ms = Some(payload.duration_ms);
                        item.font_family = Some(payload.font);
                        item.alpha = payload.alpha;
                        item.alpha_to = payload.alpha_to;
                        item.border = payload.border;
                    }
                    danmaku.push(item);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    danmaku.sort_by(|left, right| left.time.total_cmp(&right.time));
    Ok(danmaku)
}

/// Read a protobuf varint from `buf`, advancing `pos`.
fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let byte = *buf.get(*pos)?;
        *pos += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

/// Parse one `DanmakuElem` protobuf message. Only the fields used by the
/// player are decoded: progress (2, ms), mode (3), color (5), content (7).
fn parse_danmaku_elem(buf: &[u8]) -> Option<VideoDanmaku> {
    let mut pos = 0;
    let mut time_ms: Option<i64> = None;
    let mut mode: Option<i32> = None;
    let mut font_size: Option<i64> = None;
    let mut color: Option<u32> = None;
    let mut text: Option<String> = None;
    while pos < buf.len() {
        let tag = read_varint(buf, &mut pos)?;
        let field = (tag >> 3) as u32;
        let wire = (tag & 7) as u32;
        match (field, wire) {
            (2, 0) => time_ms = Some(read_varint(buf, &mut pos)? as i64),
            (3, 0) => mode = Some(read_varint(buf, &mut pos)? as i32),
            (4, 0) => font_size = Some(read_varint(buf, &mut pos)? as i64),
            (5, 0) => color = Some(read_varint(buf, &mut pos)? as u32),
            (7, 2) => {
                let len = read_varint(buf, &mut pos)? as usize;
                let raw = buf.get(pos..pos + len)?;
                pos += len;
                text = Some(String::from_utf8_lossy(raw).into_owned());
            }
            (_, 0) => {
                let _ = read_varint(buf, &mut pos);
            }
            (_, 2) => {
                let len = read_varint(buf, &mut pos)? as usize;
                pos = (pos + len).min(buf.len());
            }
            (_, 5) => pos = (pos + 4).min(buf.len()),
            (_, 1) => pos = (pos + 8).min(buf.len()),
            _ => break,
        }
    }
    let time = time_ms? as f64 / 1000.0;
    let mode = mode?;
    let text = text?;
    if time < 0.0 || text.is_empty() {
        return None;
    }
    let mut item = VideoDanmaku {
        time,
        text,
        color: color.unwrap_or(0xFF_FF_FF),
        mode,
        x: None,
        y: None,
        x2: None,
        y2: None,
        rotation: None,
        size: None,
        duration_ms: None,
        font_family: None,
        alpha: None,
        alpha_to: None,
        border: None,
    };
    if matches!(mode, 7 | 8)
        && let Some(payload) = parse_positioned_payload(
            &item.text,
            font_size.filter(|size| *size > 0).unwrap_or(25) as f64,
        )
    {
        item.text = payload.text;
        item.x = Some(payload.x);
        item.y = Some(payload.y);
        item.x2 = Some(payload.x2);
        item.y2 = Some(payload.y2);
        item.rotation = Some(payload.rotation);
        item.size = Some(payload.size);
        item.duration_ms = Some(payload.duration_ms);
        item.font_family = Some(payload.font);
        item.alpha = payload.alpha;
        item.alpha_to = payload.alpha_to;
        item.border = payload.border;
    }
    Some(item)
}

/// Parse the segmented danmaku protobuf response (`DmSegMobileReply`).
/// Top-level field 1 is a repeated `DanmakuElem`.
pub fn parse_seg_protobuf(data: &[u8]) -> Vec<VideoDanmaku> {
    let mut danmaku = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let tag = match read_varint(data, &mut pos) {
            Some(tag) => tag,
            None => break,
        };
        let field = (tag >> 3) as u32;
        let wire = (tag & 7) as u32;
        match (field, wire) {
            (1, 2) => {
                let len = match read_varint(data, &mut pos) {
                    Some(len) => len as usize,
                    None => break,
                };
                if pos + len > data.len() {
                    break;
                }
                let elem = &data[pos..pos + len];
                pos += len;
                if let Some(item) = parse_danmaku_elem(elem) {
                    danmaku.push(item);
                }
            }
            (_, 0) => {
                let _ = read_varint(data, &mut pos);
            }
            (_, 2) => {
                let len = match read_varint(data, &mut pos) {
                    Some(len) => len as usize,
                    None => break,
                };
                pos = (pos + len).min(data.len());
            }
            (_, 5) => pos = (pos + 4).min(data.len()),
            (_, 1) => pos = (pos + 8).min(data.len()),
            _ => break,
        }
    }
    // seg.so returns comments in insertion order, not playback order (the
    // response can jump backwards by over a minute between entries). Lane
    // allocation in the ASS renderer assumes playback order; without this
    // sort, top/bottom/rolling comments pile onto the same lane and hide
    // each other on screen, making the visible danmaku count collapse.
    danmaku.sort_by(|left, right| left.time.total_cmp(&right.time));
    danmaku
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timed_xml_danmaku() {
        let parsed = parse_xml(r#"<i><d p="1.5,1,25,16711680,0,0,0,0">A&amp;B</d></i>"#).unwrap();
        assert_eq!(
            parsed,
            vec![VideoDanmaku {
                time: 1.5,
                text: "A&B".into(),
                color: 16_711_680,
                mode: 1,
                x: None,
                y: None,
                x2: None,
                y2: None,
                rotation: None,
                size: None,
                duration_ms: None,
                font_family: None,
                alpha: None,
                alpha_to: None,
                border: None,
            }]
        );
    }

    #[test]
    fn normalizes_per_mille_positioned_coordinates() {
        let payload =
            r#"["693","321","1-1","10","text","6","6","87","505","400",0,1,"FangSong",1]"#;
        let parsed = parse_xml(&format!(
            r#"<i><d p="1.0,7,70,16777215,1514624467,1,b82a51a3,4141681672,10">{payload}</d></i>"#
        ))
        .unwrap();
        let item = &parsed[0];
        assert_eq!(item.mode, 7);
        assert!((item.x.unwrap() - 0.693).abs() < 1e-9);
        assert!((item.y.unwrap() - 0.321).abs() < 1e-9);
        assert!((item.x2.unwrap() - 0.087).abs() < 1e-9);
        assert!((item.y2.unwrap() - 0.505).abs() < 1e-9);
        assert!((item.rotation.unwrap() - 6.0).abs() < 1e-9);
        assert_eq!(item.duration_ms, Some(400));
        // Per-mille legacy payloads: field 3 is the font size, field 2 is the
        // id (not alpha), and field 11 is still the border flag.
        assert_eq!(item.size, Some(10.0));
        assert_eq!(item.alpha, None);
        assert_eq!(item.alpha_to, None);
        assert_eq!(item.alpha_to, None);
        assert_eq!(item.border, Some(true));
    }

    #[test]
    fn normalizes_legacy_small_per_mille_coordinates() {
        // Older Flash-era payloads use per-mille integers where x=1, y=2
        // mean 0.1% and 0.2%. The presence of a >2 coordinate (85, 293)
        // marks the whole payload as per-mille, so x=1/y=2 must become
        // 0.001/0.002 instead of being interpreted as 100%/200%.
        let payload = r#"["1","2","1-1","10","夢に僕らで帆を張って\n来るべき日のために夜を越え","0","10","85","293","2000",0,1,"\"Microsoft YaHei\"",1]"#;
        let parsed = parse_xml(&format!(
            r#"<i><d p="280.62600,7,40,104601,1573019033,0,31706c92,24052686515601408,10">{payload}</d></i>"#
        ))
        .unwrap();
        let item = &parsed[0];
        assert_eq!(item.mode, 7);
        assert!((item.x.unwrap() - 0.001).abs() < 1e-9);
        assert!((item.y.unwrap() - 0.002).abs() < 1e-9);
        assert!((item.x2.unwrap() - 0.085).abs() < 1e-9);
        assert!((item.y2.unwrap() - 0.293).abs() < 1e-9);
        // Legacy per-mille: field 6 is the rotation angle (10 here), field 3
        // is the font size (10), and field 2 is an id, not alpha.
        assert!((item.rotation.unwrap() - 10.0).abs() < 1e-9);
        assert_eq!(item.size, Some(10.0));
        assert_eq!(item.alpha, None);
    }

    #[test]
    fn parses_positioned_danmaku_payload() {
        let payload = r#"[0.32,0.11,"1-1",1.5,"真/n是/n毫/n无/n道/n理",0,0,0.32,0.11,500,0,true,"黑体",1]"#;
        let parsed = parse_xml(&format!(
            r#"<i><d p="53.91400,7,70,16777215,1514624467,1,b82a51a3,4141681672,10">{payload}</d></i>"#
        ))
        .unwrap();
        assert_eq!(parsed.len(), 1);
        let item = &parsed[0];
        assert_eq!(item.mode, 7);
        // `/n` stays as the newline marker; the Lua side converts it to ASS
        // `\N` after escaping, so no backslash survives into the ASS text.
        assert_eq!(item.text, "真/n是/n毫/n无/n道/n理");
        assert!((item.x.unwrap() - 0.32).abs() < 1e-9);
        assert!((item.y.unwrap() - 0.11).abs() < 1e-9);
        // Normalized (non-per-mille) payloads use field 3 for duration
        // seconds; the font size comes from the p attribute p[2] (70 here).
        // Field 2 is alpha ("1-1" = fully opaque), field 5 is rotate-z (0),
        // field 11 is the border flag (true).
        assert_eq!(item.size, Some(70.0));
        assert_eq!(item.duration_ms, Some(500));
        assert_eq!(item.alpha, Some(1.0));
        assert_eq!(item.alpha_to, Some(1.0));
        assert_eq!(item.border, Some(true));
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn current_public_danmaku_xml_parses() {
        let parsed = crate::api::client::ApiClient::new()
            .get_video_danmaku(39884818572, 300)
            .await
            .unwrap();
        assert!(!parsed.is_empty());
    }

    #[test]
    fn parses_real_positioned_danmaku_from_disk() {
        // Real data dumped from comment.bilibili.com (raw deflate decoded),
        // committed as a fixture so the test does not depend on /tmp.
        let xml = std::fs::read_to_string("tests/fixtures/cherry.xml")
            .expect("need tests/fixtures/cherry.xml");
        let parsed = parse_xml(&xml).unwrap();
        let mode7: Vec<_> = parsed.iter().filter(|m| m.mode == 7).collect();
        eprintln!("mode7 count: {}", mode7.len());
        let mut failed = 0;
        for (i, item) in mode7.iter().enumerate() {
            match (item.x, item.y) {
                (Some(x), Some(y)) => eprintln!(
                    "[{i}] t={:.2} x={x:.4} y={y:.4} x2={:?} y2={:?} rot={:?} size={:?} dur={:?} font={:?} text={}",
                    item.time, item.x2, item.y2, item.rotation, item.size, item.duration_ms, item.font_family, item.text
                ),
                _ => {
                    failed += 1;
                    eprintln!("[{i}] NO COORDS text={}", item.text);
                }
            }
        }
        eprintln!("mode7 total={} failed={}", mode7.len(), failed);
        assert_eq!(mode7.len(), 3, "fixture should have 3 mode7 danmaku");
        assert_eq!(failed, 0, "every mode7 must resolve to coordinates");
    }

    #[test]
    fn parses_seg_protobuf_with_positioned_danmaku() {
        // Hand-built DmSegMobileReply with two DanmakuElem entries:
        // 1) a mode-1 rolling danmaku, 2) a mode-7 positioned danmaku.
        fn varint(mut value: u64) -> Vec<u8> {
            let mut out = Vec::new();
            loop {
                let byte = (value & 0x7f) as u8;
                value >>= 7;
                if value != 0 {
                    out.push(byte | 0x80);
                } else {
                    out.push(byte);
                    break;
                }
            }
            out
        }
        fn field_varint(field: u32, value: u64) -> Vec<u8> {
            let mut out = varint(((field as u64) << 3) | 0);
            out.extend(varint(value));
            out
        }
        fn field_bytes(field: u32, bytes: &[u8]) -> Vec<u8> {
            let mut out = varint(((field as u64) << 3) | 2);
            out.extend(varint(bytes.len() as u64));
            out.extend(bytes);
            out
        }

        // rolling: progress=1500ms, mode=1, color=16777215, content="hello"
        let mut rolling = Vec::new();
        rolling.extend(field_varint(2, 1500));
        rolling.extend(field_varint(3, 1));
        rolling.extend(field_varint(5, 0xFF_FF_FF));
        rolling.extend(field_bytes(7, b"hello"));

        // positioned: progress=53000ms, mode=7, content=BAS array
        let payload = r#"[0.32,0.11,"1-1",1.5,"真/n是/n毫/n无/n道/n理",0,0,0.32,0.11,500,0,true,"黑体",1]"#;
        let mut positioned = Vec::new();
        positioned.extend(field_varint(2, 53_000));
        positioned.extend(field_varint(3, 7));
        positioned.extend(field_varint(5, 0x00_FF_00));
        positioned.extend(field_bytes(7, payload.as_bytes()));

        let mut reply = Vec::new();
        reply.extend(field_bytes(1, &rolling));
        reply.extend(field_bytes(1, &positioned));

        let parsed = parse_seg_protobuf(&reply);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].time, 1.5);
        assert_eq!(parsed[0].mode, 1);
        assert_eq!(parsed[0].text, "hello");
        assert_eq!(parsed[1].time, 53.0);
        assert_eq!(parsed[1].mode, 7);
        assert_eq!(parsed[1].text, "真/n是/n毫/n无/n道/n理");
        assert!((parsed[1].x.unwrap() - 0.32).abs() < 1e-9);
        assert!((parsed[1].y.unwrap() - 0.11).abs() < 1e-9);
        assert_eq!(parsed[1].duration_ms, Some(500));
    }
}
