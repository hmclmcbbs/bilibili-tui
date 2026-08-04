use serde::Deserialize;

/// A subtitle track listed by the player API.
#[derive(Debug, Clone, Deserialize)]
pub struct SubtitleInfo {
    pub lan: String,
    #[serde(rename = "lan_doc")]
    pub lan_doc: String,
    #[serde(rename = "subtitle_url")]
    pub subtitle_url: String,
}

/// A single subtitle cue fetched from the subtitle JSON.
#[derive(Debug, Clone, Deserialize)]
pub struct SubtitleCue {
    pub from: f64,
    pub to: f64,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct SubtitlePayload {
    body: Vec<SubtitleCue>,
}

impl SubtitleCue {
    /// Render this cue as one SRT block (index, timestamps, content).
    pub fn to_srt(&self, index: usize) -> String {
        format!(
            "{}\n{} --> {}\n{}\n\n",
            index,
            srt_timestamp(self.from),
            srt_timestamp(self.to),
            self.content
        )
    }
}

/// Parse the subtitle JSON body into cues.
pub fn parse_subtitle_body(json: &str) -> Result<Vec<SubtitleCue>, serde_json::Error> {
    let payload: SubtitlePayload = serde_json::from_str(json)?;
    Ok(payload.body)
}

/// Format a seconds value as SRT timestamp `HH:MM:SS,mmm`.
fn srt_timestamp(seconds: f64) -> String {
    let total_ms = (seconds.max(0.0) * 1000.0).round() as i64;
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms % 3_600_000) / 60_000;
    let secs = (total_ms % 60_000) / 1000;
    let millis = total_ms % 1000;
    format!("{hours:02}:{minutes:02}:{secs:02},{millis:03}")
}

/// Render a full SRT document from cues.
pub fn render_srt(cues: &[SubtitleCue]) -> String {
    let mut out = String::new();
    for (i, cue) in cues.iter().enumerate() {
        out.push_str(&cue.to_srt(i + 1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_subtitle_body() {
        let json = r#"{"body":[{"from":1.0,"to":3.5,"content":"你好"},{"from":4.0,"to":6.25,"content":"世界"}]}"#;
        let cues = parse_subtitle_body(json).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].content, "你好");
        assert_eq!(cues[1].to, 6.25);
    }

    #[test]
    fn renders_srt_timestamp() {
        assert_eq!(srt_timestamp(1.0), "00:00:01,000");
        assert_eq!(srt_timestamp(61.5), "00:01:01,500");
        assert_eq!(srt_timestamp(3661.25), "01:01:01,250");
    }

    #[test]
    fn renders_srt_document() {
        let cues = vec![
            SubtitleCue {
                from: 1.0,
                to: 3.5,
                content: "你好".to_string(),
            },
            SubtitleCue {
                from: 4.0,
                to: 6.0,
                content: "世界".to_string(),
            },
        ];
        let srt = render_srt(&cues);
        assert!(srt.contains("00:00:01,000 --> 00:00:03,500"));
        assert!(srt.contains("00:00:04,000 --> 00:00:06,000"));
        assert!(srt.contains("你好"));
    }
}
