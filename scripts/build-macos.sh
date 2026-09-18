#!/usr/bin/env bash
# Phoebe Assistant - macOS (Apple Silicon) one-shot build script.
#
# Usage from the repository root:
#   ./scripts/build-macos.sh
#
# Checks the toolchain and GPT-SoVITS inputs, then builds the .app and DMG.
# See docs/phoebe-voice.md for the voice runtime details.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() {
  echo ""
  echo "[FAIL] $1" >&2
  exit 1
}

echo "=== Phoebe Assistant macOS build ==="

command -v node >/dev/null 2>&1 || fail "Node.js not found. Install Node.js 22+ from https://nodejs.org/"
echo "[OK] Node $(node -v)"

command -v cargo >/dev/null 2>&1 || fail "Rust not found. Install from https://rustup.rs/"
echo "[OK] $(cargo -V)"

command -v conda >/dev/null 2>&1 || fail "conda not found. Install miniconda."
echo "[OK] $(conda --version)"

GPT_ROOT="${PHOEBE_GPTSOVITS_ROOT:-$(dirname "$ROOT")/GPT-SoVITS}"
GPT_ENV="${PHOEBE_GPTSOVITS_ENV:-$HOME/miniconda3/envs/GPTSoVits}"

for w in \
  "GPT_weights_v2/phoebe_zh_v2-e15.ckpt" \
  "SoVITS_weights_v2/phoebe_zh_v2_e8_s912.pth" \
  "GPT_SoVITS/pretrained_models/chinese-roberta-wwm-ext-large" \
  "GPT_SoVITS/pretrained_models/chinese-hubert-base" \
  "GPT_SoVITS/pretrained_models/fast_langdetect"; do
  [ -e "$GPT_ROOT/$w" ] || fail "Missing GPT-SoVITS input: $GPT_ROOT/$w"
done
echo "[OK] GPT-SoVITS weights at $GPT_ROOT"

[ -d "$GPT_ENV/conda-meta" ] || fail "conda environment not found at $GPT_ENV"
echo "[OK] conda env at $GPT_ENV"

export PHOEBE_GPTSOVITS_ROOT="$GPT_ROOT"
export PHOEBE_GPTSOVITS_ENV="$GPT_ENV"

echo ""
echo "=== npm ci ==="
npm ci

echo ""
echo "=== Building .app + DMG ==="
npm run tauri:build -w @phoebe/desktop

echo ""
echo "=== Done ==="
echo "App: target/release/bundle/macos/Phoebe Assistant.app"
echo "DMG: target/release/bundle/dmg/"
