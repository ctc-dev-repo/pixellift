#!/usr/bin/env bash
# Fetches the sidecar binaries PixelLift orchestrates:
#   - realesrgan-ncnn-vulkan (+ models) from xinntao/Real-ESRGAN releases
#   - ffmpeg/ffprobe: install via your package manager (brew/apt/winget)
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SIDECARS="$HERE/sidecars"
mkdir -p "$SIDECARS"

OS="$(uname -s)"
case "$OS" in
  Darwin) PKG="macos";;
  Linux)  PKG="ubuntu";;
  *)      echo "On Windows: download realesrgan-ncnn-vulkan-v0.2.0-windows.zip manually."; exit 1;;
esac

BASE="https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0"
ZIP="realesrgan-ncnn-vulkan-20220424-$PKG.zip"

echo "Downloading $ZIP ..."
curl -sL -o "$SIDECARS/$ZIP" "$BASE/$ZIP"
mkdir -p "$SIDECARS/realesrgan"
unzip -q -o "$SIDECARS/$ZIP" "realesrgan-ncnn-vulkan*" "models/*" -d "$SIDECARS/realesrgan/"
rm "$SIDECARS/$ZIP"

BIN_DIR="$SIDECARS/realesrgan"
echo "Sidecar installed: $BIN_DIR"
echo
echo "Run PixelLift with:"
echo "  pixellift --input video.mp4 --esrgan \"$BIN_DIR/realesrgan-ncnn-vulkan\""
echo
echo "ffmpeg/ffprobe: install with 'brew install ffmpeg' (macOS),"
echo "'sudo apt install ffmpeg' (Ubuntu/Debian) or download from ffmpeg.org (Windows)."
