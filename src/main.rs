mod cli;
mod gui;
mod pipeline;
mod probe;

use anyhow::Result;
use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() {
    if let Err(e) = try_main() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    let cli = cli::Cli::parse();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let c = cancel.clone();
        ctrlc::set_handler(move || {
            c.store(true, Ordering::SeqCst);
            eprintln!("\ncancelling after the current step…");
        })?;
    }

    // No input file (or explicit --gui) launches the graphical interface.
    if cli.gui || cli.input.is_none() {
        return gui::run();
    }

    let job = pipeline::job_from_cli(&cli);
    let started = std::time::Instant::now();
    let mut last_stage = String::new();

    pipeline::run(&job, &cancel, |ev| match ev {
        pipeline::Event::Stage(s) => {
            eprintln!();
            eprintln!("[{}]", s);
            last_stage = s.to_string();
        }
        pipeline::Event::Progress { done, total } => {
            eprint!("\r  {}/{} frames", done, total);
        }
        pipeline::Event::Info(msg) => eprintln!("  {}", msg),
    })?;

    eprintln!();
    println!(
        "Done in {:.1}s -> {}",
        started.elapsed().as_secs_f32(),
        job.output.display()
    );
    let _ = last_stage;
    Ok(())
}
