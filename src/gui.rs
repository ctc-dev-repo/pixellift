use anyhow::Result;

/// Milestone 2 will replace this with the egui interface.
/// The engine is fully usable from the CLI today (see `pixellift --help`).
pub fn run() -> Result<()> {
    eprintln!("PixelLift GUI is coming in milestone 2.");
    eprintln!("The upscaling engine is ready today — use the CLI:");
    eprintln!("  pixellift --input video.mp4 --scale 4 --codec hevc");
    Ok(())
}
