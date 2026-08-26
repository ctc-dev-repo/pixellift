# PixelLift

**AI video upscaler — SD to HD to UHD.** Powered by Real-ESRGAN running on
your GPU via Vulkan (NVIDIA, AMD, and Intel), orchestrated by a single native
Rust binary. CLI today, GUI in milestone 2.

## Status: Milestone 1 — CLI engine ✅

The full pipeline works end-to-end and is verified:
`probe → frame extract → AI upscale (chunked, resumable) → encode (audio preserved)`.

| Milestone | Scope | Status |
|---|---|---|
| 1 | CLI engine: probe/extract/upscale/encode, progress, cancel, resume | ✅ done |
| 2 | egui GUI: drag & drop, presets, progress, preview | planned |
| 3 | Packaging: bundled sidecars, Windows/Linux/macOS builds via CI | planned |
| 4 | Extras: RIFE frame interpolation, batch queues, HDR tone-mapping | planned |

## Build & run

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
