use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    /// Rational frame rate as ffmpeg accepts it, e.g. "30000/1001"
    pub fps: String,
    pub has_audio: bool,
    pub duration: f64,
}

pub fn probe(ffprobe: &Path, input: &Path) -> Result<VideoInfo> {
    let out = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .arg(input)
        .output()
        .with_context(|| format!("running ffprobe on {}", input.display()))?;

    if !out.status.success() {
        anyhow::bail!(
            "ffprobe failed for {} (is this a valid video file?)",
            input.display()
        );
    }

    let json: Value = serde_json::from_slice(&out.stdout).context("parsing ffprobe output")?;
    let streams = json["streams"]
        .as_array()
        .context("ffprobe returned no streams")?;

    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .context("no video stream found")?;

    let width = video["width"].as_u64().context("missing width")? as u32;
    let height = video["height"].as_u64().context("missing height")? as u32;
    let fps = video["r_frame_rate"]
        .as_str()
        .unwrap_or("25/1")
        .to_string();
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);
    let has_audio = streams
        .iter()
        .any(|s| s["codec_type"] == "audio");

    Ok(VideoInfo {
        width,
        height,
        fps,
        has_audio,
        duration,
    })
}
