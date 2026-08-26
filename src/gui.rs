use crate::cli::Codec;
use eframe::egui;

use crate::pipeline::{self, Event, Job};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

const MODELS: [&str; 4] = [
    "realesr-animevideov3",
    "realesrgan-x4plus",
    "realesrgan-x4plus-anime",
    "realesrnet-x4plus",
];
const CODECS: [(Codec, &str); 3] = [
    (Codec::Hevc, "HEVC / H.265 (smaller files)"),
    (Codec::H264, "H.264 (maximum compatibility)"),
    (Codec::Av1, "AV1 (modern, slowest)"),
];

enum Msg {
    Event(Event),
    Finished(Result<String, String>),
}

#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Ok(String),
    Failed(String),
    Cancelled,
}

pub struct PixelLiftApp {
    input: String,
    output: String,
    scale: usize,
    model: usize,
    codec: usize,
    quality: f32,
    fps: String,
    esrgan: String,
    keep_frames: bool,
    resume: bool,

    running: bool,
    rx: Option<Receiver<Msg>>,
    worker: Option<JoinHandle<()>>,
    cancel: Option<Arc<AtomicBool>>,
    started: Option<Instant>,

    stage: String,
    progress: Option<(usize, usize)>,
    log: Vec<String>,
    outcome: Option<Outcome>,
}

impl Default for PixelLiftApp {
    fn default() -> Self {
        Self {
            input: String::new(),
            output: String::new(),
            scale: 2,
            model: 0,
            codec: 0,
            quality: 60.0,
            fps: String::new(),
            esrgan: autodetect_esrgan()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            keep_frames: false,
            resume: false,
            running: false,
            rx: None,
            worker: None,
            cancel: None,
            started: None,
            stage: String::new(),
            progress: None,
            log: vec!["Welcome to PixelLift. Drop a video here or pick one below.".into()],
            outcome: None,
        }
    }
}

fn autodetect_esrgan() -> Option<PathBuf> {
    // 1) next to the executable (packaged layout: <app>/sidecars/realesrgan/…)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(found) = scan_sidecars(&dir.join("sidecars"), 0) {
                return Some(found);
            }
        }
    }
    // 2) current working directory (repo / dev layout: ./sidecars/…)
    let base = std::env::current_dir().ok()?.join("sidecars");
    scan_sidecars(&base, 0)
}

fn scan_sidecars(dir: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 4 {
        return None;
    }
    for e in std::fs::read_dir(dir).ok()? {
        let p = e.ok()?.path();
        if p.is_dir() {
            if let Some(found) = scan_sidecars(&p, depth + 1) {
                return Some(found);
            }
        } else if p
            .file_name()
            .map(|n| n.to_string_lossy().starts_with("realesrgan-ncnn-vulkan"))
            .unwrap_or(false)
        {
            return Some(p);
        }
    }
    None
}

fn crf_for(codec: &Codec, quality: f32) -> u32 {
    let q = quality.clamp(0.0, 100.0);
    match codec {
        Codec::Hevc => (28.0 - q * 0.12) as u32,
        Codec::H264 => (28.0 - q * 0.14) as u32,
        Codec::Av1 => (50.0 - q * 0.30) as u32,
    }
}

impl PixelLiftApp {
    fn log(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        if self.log.len() > 500 {
            self.log.remove(0);
        }
    }

    fn start(&mut self) {
        if self.input.trim().is_empty() {
            self.log("Pick an input video first.");
            return;
        }
        let output = if self.output.trim().is_empty() {
            let stem = Path::new(self.input.trim())
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "output".to_string());
            format!("{}_upscaled.mp4", stem)
        } else {
            self.output.trim().to_string()
        };
        let esrgan = if self.esrgan.trim().is_empty() {
            PathBuf::from("realesrgan-ncnn-vulkan")
        } else {
            PathBuf::from(self.esrgan.trim())
        };
        let codec = CODECS[self.codec].0.clone();
        let job = Job {
            input: PathBuf::from(self.input.trim()),
            output: PathBuf::from(&output),
            scale: self.scale as u32,
            model: MODELS[self.model].to_string(),
            codec: codec.clone(),
            crf: crf_for(&codec, self.quality),
            fps: {
                let f = self.fps.trim();
                if f.is_empty() {
                    None
                } else {
                    Some(f.to_string())
                }
            },
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            esrgan,
            workdir: None,
            keep_frames: self.keep_frames,
            resume: self.resume,
        };

        let (tx, rx): (Sender<Msg>, Receiver<Msg>) = channel();
        let tx_events = tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel2 = cancel.clone();
        self.log(format!(
            "Starting: {} → {} ({}x, {}, CRF {})",
            job.input.display(),
            job.output.display(),
            job.scale,
            MODELS[self.model],
            job.crf
        ));
        let out_display = output.clone();
        let worker = std::thread::spawn(move || {
            let res = pipeline::run(&job, &cancel2, move |ev| {
                let _ = tx_events.send(Msg::Event(ev));
            });
            let _ = tx.send(Msg::Finished(match res {
                Ok(()) => Ok(out_display),
                Err(e) => Err(format!("{e:#}")),
            }));
        });

        self.running = true;
        self.outcome = None;
        self.stage = "starting".into();
        self.progress = None;
        self.started = Some(Instant::now());
        self.rx = Some(rx);
        self.worker = Some(worker);
        self.cancel = Some(cancel);
    }

    fn cancel(&mut self) {
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::SeqCst);
            self.log("Cancel requested — finishing the current step…");
        }
    }

    fn poll(&mut self) {
        let Some(mut rx) = self.rx.take() else { return };
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Event(Event::Stage(s)) => {
                    self.stage = s.to_string();
                    self.progress = None;
                    self.log(format!("[{}]", s));
                }
                Msg::Event(Event::Progress { done, total }) => {
                    self.progress = Some((done, total));
                }
                Msg::Event(Event::Info(s)) => self.log(format!("  {}", s)),
                Msg::Finished(res) => {
                    self.running = false;
                    match &res {
                        Ok(out) => {
                            self.outcome = Some(Outcome::Ok(out.clone()));
                            self.log(format!("✔ Done -> {}", out));
                        }
                        Err(e) if e.contains("cancelled") => {
                            self.outcome = Some(Outcome::Cancelled);
                            self.log("Cancelled. Use --resume (checkbox) to continue later.");
                        }
                        Err(e) => {
                            self.outcome = Some(Outcome::Failed(e.clone()));
                            self.log(format!("✖ Failed: {}", e));
                        }
                    }
                    self.worker = None;
                    return; // receiver consumed; run is over
                }
            }
        }
        self.rx = Some(rx);
    }
}

impl eframe::App for PixelLiftApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();

        // drag & drop
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(f) = dropped
            .iter()
            .find_map(|df| df.path.clone())
            .filter(|_| !self.running)
        {
            self.input = f.display().to_string();
            self.log(format!("Dropped: {}", self.input));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::default().inner_margin(14.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("PixelLift");
                    ui.weak("AI video upscaler — SD → HD → UHD");
                });
                ui.add_space(6.0);
                ui.separator();

                egui::Grid::new("files")
                    .num_columns(3)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Input video");
                    ui.add_enabled(
                        !self.running,
                        egui::TextEdit::singleline(&mut self.input)
                            .hint_text("drop a file here, or browse…")
                            .desired_width(ui.available_width() - 90.0),
                    );
                        if ui
                            .add_enabled(!self.running, egui::Button::new("Browse…"))
                            .clicked()
                        {
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter("Videos", &["mp4", "mkv", "mov", "avi", "webm", "m4v"])
                                .pick_file()
                            {
                                self.input = p.display().to_string();
                            }
                        }
                        ui.end_row();

                        ui.label("Output video");
                        ui.add_sized(
                            [ui.available_width() - 90.0, 20.0],
                            egui::TextEdit::singleline(&mut self.output)
                                .hint_text("auto: <input>_upscaled.mp4"),
                        );
                        if ui
                            .add_enabled(!self.running, egui::Button::new("Browse…"))
                            .clicked()
                        {
                            if let Some(p) = rfd::FileDialog::new()
                                .set_file_name("upscaled.mp4")
                                .save_file()
                            {
                                self.output = p.display().to_string();
                            }
                        }
                        ui.end_row();
                    });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                egui::Grid::new("settings")
                    .num_columns(4)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Upscale");
                        egui::ComboBox::from_id_source("scale")
                            .selected_text(format!("{}×", self.scale))
                            .show_ui(ui, |ui| {
                                for s in [2usize, 3, 4] {
                                    ui.selectable_value(&mut self.scale, s, format!("{}×", s));
                                }
                            });
                        ui.label("Model");
                        egui::ComboBox::from_id_source("model")
                            .selected_text(MODELS[self.model])
                            .show_ui(ui, |ui| {
                                for (i, m) in MODELS.iter().enumerate() {
                                    ui.selectable_value(&mut self.model, i, *m);
                                }
                            });
                        ui.end_row();

                        ui.label("Codec");
                        egui::ComboBox::from_id_source("codec")
                            .selected_text(CODECS[self.codec].1)
                            .show_ui(ui, |ui| {
                                for (i, (_, label)) in CODECS.iter().enumerate() {
                                    ui.selectable_value(&mut self.codec, i, *label);
                                }
                            });
                        ui.label("Quality");
                        ui.add(
                            egui::Slider::new(&mut self.quality, 0.0..=100.0)
                                .text("higher = better + slower"),
                        );
                        ui.end_row();

                        ui.label("Frame rate");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.fps)
                                .hint_text("source fps")
                                .desired_width(90.0),
                        );
                        ui.label("Sidecar");
                        ui.add_sized(
                            [ui.available_width() - 20.0, 20.0],
                            egui::TextEdit::singleline(&mut self.esrgan)
                                .hint_text("auto-detected"),
                        );
                        ui.end_row();
                    });

                ui.checkbox(&mut self.resume, "Resume an interrupted run");
                ui.checkbox(&mut self.keep_frames, "Keep extracted frames");

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    let button = ui.add_sized(
                        [160.0, 36.0],
                        egui::Button::new(
                            egui::RichText::new(if self.running { "⏹ Cancel" } else { "✨ Upscale" })
                                .size(18.0),
                        ),
                    );
                    if self.running {
                        if button.clicked() {
                            self.cancel();
                        }
                    } else if button.clicked() {
                        self.start();
                    }

                    if self.running {
                        let elapsed = self
                            .started
                            .map(|s| s.elapsed().as_secs_f32())
                            .unwrap_or(0.0);
                        ui.weak(format!("running… {:.0}s", elapsed));
                        if ui.button("clear log").clicked() {
                            self.log.clear();
                        }
                    }
                });

                ui.add_space(8.0);
                ui.separator();

                match (&self.running, &self.outcome) {
                    (true, _) => {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(18.0));
                            ui.weak(format!("stage: {}", self.stage));
                        });
                        match self.progress {
                            Some((done, total)) if total > 0 => {
                                ui.add(egui::ProgressBar::new(done as f32 / total as f32)
                                    .text(format!("{} / {} frames", done, total)));
                            }
                            _ => {
                                ui.weak("working…");
                            }
                        }
                    }
                    (false, Some(Outcome::Ok(out))) => {
                        ui.colored_label(egui::Color32::from_rgb(80, 220, 120),
                            format!("✔ Finished -> {}", out));
                    }
                    (false, Some(Outcome::Cancelled)) => {
                        ui.colored_label(egui::Color32::YELLOW,
                            "Cancelled. Tick “Resume” and start again to continue.");
                    }
                    (false, Some(Outcome::Failed(e))) => {
                        ui.colored_label(egui::Color32::from_rgb(255, 110, 110),
                            format!("✖ Failed: {}", e));
                    }
                    _ => {}
                }

                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        for line in &self.log {
                            ui.monospace(line);
                        }
                    });
            });

        if self.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
}

pub fn run() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 680.0])
            .with_min_inner_size([640.0, 540.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "PixelLift — AI Video Upscaler",
        options,
        Box::new(|_cc| Box::new(PixelLiftApp::default())),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {e}"))
}
