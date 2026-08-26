# PixelLift

**AI video upscaler — SD to HD to UHD.** Powered by Real-ESRGAN running on
your GPU via Vulkan (NVIDIA, AMD, and Intel), orchestrated by a single native
Rust binary. CLI today, GUI in milestone 2.

## Status: Milestones 1–3 complete ✅

| Milestone | Scope | Status |
|---|---|---|
| 1 | CLI engine: probe/extract/upscale/encode, progress, cancel, resume | ✅ done |
| 2 | egui GUI: drag & drop, presets, progress, cancel, log | ✅ done |
| 3 | Packaging: Windows/Linux/macOS release zips with bundled sidecars | ✅ done |
| 4 | Extras: RIFE frame interpolation, batch queues, HDR tone-mapping | planned |

## Download (no build required)

Grab a per-OS zip from **Releases** — each bundles PixelLift, FFmpeg, and the
Real-ESRGAN sidecar + models. Unzip, then:

- **Windows:** double-click `pixellift.exe`
- **macOS:** right-click the binary → Open (first run, Gatekeeper), or run
  `./pixellift` in Terminal from the folder
- **Linux:** `./pixellift` (needs Vulkan drivers — anything that games, works)

No input file given opens the GUI; pass `--input` for CLI mode. The GUI
auto-detects the bundled sidecar in `sidecars/realesrgan/`.

## Build from source

```bash
./fetch-sidecars.sh          # downloads Real-ESRGAN sidecar + models
brew install ffmpeg          # macOS (apt install ffmpeg / winget on Windows)
cargo build --release

# Upscale a video 2x (HEVC output, original audio preserved):
./target/release/pixellift --input clip.mp4 --scale 2

# 4x to UHD, H.264 instead:
./target/release/pixellift -i clip.mp4 -s 4 --codec h264 -o clip_4k.mp4
```

If sidecars aren't on your `PATH`, point at them:
`--esrgan sidecars/esrgan/realesrgan-ncnn-vulkan` (macOS example after
running `fetch-sidecars.sh`).

## Options that matter

| Flag | Meaning |
|---|---|
| `--scale 2/3/4` | Upscale factor (animevideov3 model supports all; x4plus models are 4x-only) |
| `--model` | `realesr-animevideov3` (fast, default) · `realesrgan-x4plus` (photoreal) · `realesrgan-x4plus-anime` |
| `--codec hevc/h264/av1` | Output codec (audio is copied untouched) |
| `--crf` | Quality knob (lower = better/bigger) |
| `--resume` | Reuse frames from an interrupted run |
| `--keep-frames` | Keep the PNG frame cache |

Press **Ctrl-C** anytime — the current step finishes, frames are kept, and
`--resume` continues where you left off.

## Architecture

```
ffprobe ──► ffmpeg (extract frames, CFR)
        ──► realesrgan-ncnn-vulkan (Vulkan AI upscale, 48-frame chunks)
        ──► ffmpeg (encode H.264/HEVC/AV1 + mux original audio)
```

The AI inference is delegated to `real-esrgan-ncnn-vulkan`
([BSD-3-Clause](https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan)) so
PixelLift itself stays a small native binary with zero Python and no CUDA
requirement — Vulkan runs on every GPU vendor. Video container handling is
FFmpeg (LGPL build).

## Known sharp edges (M1)

- `realesrgan-ncnn-vulkan` silently skips **symlinked** frames — PixelLift
  uses hard links (with copy fallback) to work around it
- x4plus-family models output 4x regardless of `--scale`; PixelLift clamps
  for you

## License

Apache-2.0 for PixelLift's code. Sidecar binaries and models carry their own
licenses (BSD-3-Clause / model licenses — see About in the GUI, milestone 3).
