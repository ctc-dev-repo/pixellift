use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum Codec {
    Hevc,
    H264,
    Av1,
}

impl Codec {
    pub fn default_crf(&self) -> u32 {
        match self {
            Codec::Hevc => 20,
            Codec::H264 => 18,
            Codec::Av1 => 32,
        }
    }

    pub fn encoder_args(&self, crf: u32) -> Vec<String> {
        match self {
            Codec::Hevc => vec!["-c:v", "libx265", "-crf", &crf.to_string(), "-tag:v", "hvc1"]
                .into_iter()
                .map(String::from)
                .collect(),
            Codec::H264 => vec!["-c:v", "libx264", "-crf", &crf.to_string()]
                .into_iter()
                .map(String::from)
                .collect(),
            Codec::Av1 => vec!["-c:v", "libsvtav1", "-crf", &crf.to_string()]
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "pixellift",
    version,
    about = "AI video upscaler — SD to HD to UHD, powered by Real-ESRGAN (Vulkan)"
)]
pub struct Cli {
    /// Input video file
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Output video file (default: <input>_upscaled.mp4)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Upscale factor: 2, 3 or 4
    #[arg(short, long, default_value_t = 2)]
    pub scale: u32,

    /// Real-ESRGAN model (realesr-animevideov3 supports 2/3/4x; x4plus variants are 4x only)
    #[arg(short, long, default_value = "realesr-animevideov3")]
    pub model: String,

    /// Output codec
    #[arg(short, long, value_enum, default_value_t = Codec::Hevc)]
    pub codec: Codec,

    /// Quality: CRF value (defaults: hevc 20, h264 18, av1 32 — lower is better)
    #[arg(long)]
    pub crf: Option<u32>,

    /// Output frame rate (default: source frame rate)
    #[arg(long)]
    pub fps: Option<String>,

    /// Path to ffmpeg (default: search PATH)
    #[arg(long)]
    pub ffmpeg: Option<PathBuf>,

    /// Path to ffprobe (default: search PATH)
    #[arg(long)]
    pub ffprobe: Option<PathBuf>,

    /// Path to realesrgan-ncnn-vulkan (default: search PATH)
    #[arg(short, long)]
    pub esrgan: Option<PathBuf>,

    /// Working directory for frames (default: <output>.frames)
    #[arg(long)]
    pub workdir: Option<PathBuf>,

    /// Keep extracted/upscaled frames after completion
    #[arg(long)]
    pub keep_frames: bool,

    /// Reuse frames already in the workdir (resume an interrupted run)
    #[arg(long)]
    pub resume: bool,

    /// Launch the graphical interface
    #[arg(long)]
    pub gui: bool,
}
