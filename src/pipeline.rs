use crate::cli::{Cli, Codec};
use crate::probe;
use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const CHUNK_SIZE: usize = 48;

#[derive(Debug, Clone)]
pub enum Event<'a> {
    Stage(&'a str),
    Progress { done: usize, total: usize },
    Info(String),
}

pub struct Job {
    pub input: PathBuf,
    pub output: PathBuf,
    pub scale: u32,
    pub model: String,
    pub codec: Codec,
    pub crf: u32,
    pub fps: Option<String>,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub esrgan: PathBuf,
    pub workdir: Option<PathBuf>,
    pub keep_frames: bool,
    pub resume: bool,
}

pub fn job_from_cli(cli: &Cli) -> Job {
    let output = cli.output.clone().unwrap_or_else(|| {
        let stem = cli
            .input
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "output".to_string());
        PathBuf::from(format!("{}_upscaled.mp4", stem))
    });

    // x4plus-family models are hard-locked to 4x by their weights.
    let mut scale = cli.scale;
    if cli.model.contains("x4plus") && scale != 4 {
        scale = 4;
    }

    Job {
        input: cli.input.clone().expect("input required for upscale"),
        output,
        scale,
        model: cli.model.clone(),
        codec: cli.codec.clone(),
        crf: cli.crf.unwrap_or_else(|| cli.codec.default_crf()),
        fps: cli.fps.clone(),
        ffmpeg: cli.ffmpeg.clone().unwrap_or_else(|| PathBuf::from("ffmpeg")),
        ffprobe: cli.ffprobe.clone().unwrap_or_else(|| PathBuf::from("ffprobe")),
        esrgan: cli
            .esrgan
            .clone()
            .unwrap_or_else(|| PathBuf::from("realesrgan-ncnn-vulkan")),
        workdir: cli.workdir.clone(),
        keep_frames: cli.keep_frames,
        resume: cli.resume,
    }
}

fn is_cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::SeqCst)
}

fn run_child(cmd: &mut Command, cancel: &Arc<AtomicBool>, what: &str) -> Result<()> {
    use std::io::Read;
    cmd.stdout(Stdio::null());
    let err_path = std::env::temp_dir().join(format!(
        "pixellift-{}-{}.log",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    let err_file = std::fs::File::create(&err_path)?;
    cmd.stderr(Stdio::from(err_file));
    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawning {} (is it installed and on PATH?)", what))?;
    let status;
    loop {
        match child.try_wait()? {
            Some(s) => {
                status = s;
                break;
            }
            None => {
                if is_cancelled(cancel) {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!("cancelled by user");
                }
                std::thread::sleep(Duration::from_millis(150));
            }
        }
    }
    if !status.success() {
        let mut tail = String::new();
        if let Ok(mut f) = std::fs::File::open(&err_path) {
            let _ = f.read_to_string(&mut tail);
        }
        let tail: String = tail
            .lines()
            .filter(|l| !l.trim().is_empty())
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n  ");
        let _ = std::fs::remove_file(&err_path);
        bail!(
            "{} failed with {}\n  last output:\n  {}",
            what,
            status,
            tail
        );
    }
    let _ = std::fs::remove_file(&err_path);
    Ok(())
}

fn list_frames(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut frames: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "png").unwrap_or(false))
        .collect();
    frames.sort();
    Ok(frames)
}

fn clear_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_file() {
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}

pub fn run(job: &Job, cancel: &Arc<AtomicBool>, mut on_event: impl FnMut(Event)) -> Result<()> {
    let models_dir = job
        .esrgan
        .parent()
        .map(|p| p.join("models"))
        .unwrap_or_else(|| PathBuf::from("models"));

    // ---- probe ----
    on_event(Event::Stage("probe"));
    let info = probe::probe(&job.ffprobe, &job.input)?;
    on_event(Event::Info(format!(
        "source: {}x{} @ {} fps, {} s, audio: {}",
        info.width,
        info.height,
        info.fps,
        info.duration as u64,
        if info.has_audio { "yes" } else { "no" }
    )));

    // ---- workdir ----
    let auto_workdir = job.workdir.is_none();
    let workdir = job.workdir.clone().unwrap_or_else(|| {
        let stem = job
            .output
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "pixellift".to_string());
        let parent = job.output.parent().unwrap_or(Path::new(".")).to_path_buf();
        parent.join(format!("{}.frames", stem))
    });
    let fin = workdir.join("in");
    let fout = workdir.join("out");
    let chunk_in = workdir.join("chunk_in");
    let chunk_out = workdir.join("chunk_out");
    std::fs::create_dir_all(&fin)?;
    std::fs::create_dir_all(&fout)?;
    std::fs::create_dir_all(&chunk_in)?;
    std::fs::create_dir_all(&chunk_out)?;

    let result = run_inner(
        job,
        cancel,
        &mut on_event,
        &info,
        &fin,
        &fout,
        &chunk_in,
        &chunk_out,
        &models_dir,
    );

    if auto_workdir && !job.keep_frames {
        let _ = std::fs::remove_dir_all(&workdir);
    }
    result
}

fn run_inner(
    job: &Job,
    cancel: &Arc<AtomicBool>,
    on_event: &mut dyn FnMut(Event),
    info: &probe::VideoInfo,
    fin: &Path,
    fout: &Path,
    chunk_in: &Path,
    chunk_out: &Path,
    models_dir: &Path,
) -> Result<()> {
    let fps_str = job.fps.clone().unwrap_or_else(|| info.fps.clone());

    // ---- extract frames (CFR) ----
    let existing = list_frames(fin)?;
    if job.resume && !existing.is_empty() {
        on_event(Event::Info(format!(
            "resume: reusing {} extracted frames",
            existing.len()
        )));
    } else {
        on_event(Event::Stage("extract"));
        clear_dir(fin)?;
        let mut cmd = Command::new(&job.ffmpeg);
        cmd.arg("-y")
            .arg("-i")
            .arg(&job.input)
            .args(["-vf", &format!("fps={}", fps_str)])
            .args(["-start_number", "0"])
            .arg(fin.join("%06d.png"));
        run_child(&mut cmd, cancel, "frame extraction (ffmpeg)")?;
    }

    let frames = list_frames(fin)?;
    if frames.is_empty() {
        bail!("no frames were extracted — check the input file and fps setting");
    }
    on_event(Event::Info(format!("{} frames to upscale", frames.len())));

    // ---- upscale in chunks ----
    on_event(Event::Stage("upscale"));
    let total = frames.len();
    let mut done = 0usize;
    for chunk in frames.chunks(CHUNK_SIZE) {
        if is_cancelled(cancel) {
            bail!("cancelled by user");
        }
        let outs: Vec<PathBuf> = chunk
            .iter()
            .map(|f| fout.join(f.file_name().unwrap()))
            .collect();
        let already = outs.iter().filter(|p| p.exists()).count();
        if job.resume && already == chunk.len() {
            done += chunk.len();
            on_event(Event::Progress { done, total });
            continue;
        }

        clear_dir(chunk_in)?;
        clear_dir(chunk_out)?;
        for f in chunk {
            let target = chunk_in.join(f.file_name().unwrap());
            // NOTE: realesrgan-ncnn-vulkan's directory scanner silently skips
            // symlinks, so we use hard links (falling back to a copy).
            if std::fs::hard_link(f, &target).is_err() {
                std::fs::copy(f, &target)?;
            }
        }

        let mut cmd = Command::new(&job.esrgan);
        cmd.arg("-i")
            .arg(chunk_in)
            .arg("-o")
            .arg(chunk_out)
            .args(["-s", &job.scale.to_string()])
            .args(["-n", &job.model])
            .args(["-m", &models_dir.to_string_lossy().to_string()]);
        run_child(&mut cmd, cancel, "AI upscale (realesrgan-ncnn-vulkan)")?;

        for e in std::fs::read_dir(chunk_out)? {
            let p = e?.path();
            std::fs::copy(&p, fout.join(p.file_name().unwrap()))?;
        }
        done += chunk.len();
        on_event(Event::Progress { done, total });
    }

    // ---- encode ----
    on_event(Event::Stage("encode"));
    let mut cmd = Command::new(&job.ffmpeg);
    cmd.arg("-y")
        .args(["-framerate", &fps_str])
        .arg("-i")
        .arg(fout.join("%06d.png"));
    if info.has_audio {
        cmd.arg("-i").arg(&job.input);
    }
    cmd.arg("-map").arg("0:v:0");
    if info.has_audio {
        cmd.args(["-map", "1:a:0", "-c:a", "copy", "-shortest"]);
    }
    for a in job.codec.encoder_args(job.crf) {
        cmd.arg(a);
    }
    cmd.args(["-pix_fmt", "yuv420p", "-movflags", "+faststart"]);
    cmd.arg(&job.output);
    run_child(&mut cmd, cancel, "encoding (ffmpeg)")?;

    Ok(())
}
