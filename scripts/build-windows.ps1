# Phoebe Assistant - Windows x64 one-shot build script.
#
# Run from the repository root (or anywhere; it locates the root itself):
#   powershell -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1
#
# It checks the toolchain and GPT-SoVITS inputs, then runs the full release
# build (frontend + sidecar bundle + Windows voice runtime + NSIS installer).
# See docs/windows-release.md for the full checklist.

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path $PSScriptRoot -Parent
Set-Location $ProjectRoot

function Fail($message) {
    Write-Host ""
    Write-Host "[FAIL] $message" -ForegroundColor Red
    Write-Host "See docs/windows-release.md for detailed instructions." -ForegroundColor Yellow
    exit 1
}

Write-Host "=== Phoebe Assistant Windows build ===" -ForegroundColor Cyan

# 1. Toolchain checks
try { $nodeVersion = node -v } catch { Fail "Node.js not found. Install Node.js 22+ from https://nodejs.org/" }
Write-Host "[OK] Node $nodeVersion"

try { $rustVersion = cargo -V } catch { Fail "Rust not found. Install rustup (MSVC) from https://rustup.rs/" }
Write-Host "[OK] $rustVersion"

try { $condaVersion = conda --version } catch { Fail "conda not found. Install miniconda from https://docs.conda.io/en/latest/miniconda.html" }
Write-Host "[OK] $condaVersion"

# 2. GPT-SoVITS paths (env vars override the defaults)
if ($env:PHOEBE_GPTSOVITS_ROOT) {
    $gptRoot = $env:PHOEBE_GPTSOVITS_ROOT
} else {
    $gptRoot = Join-Path (Split-Path $ProjectRoot -Parent) "GPT-SoVITS"
}
if ($env:PHOEBE_GPTSOVITS_ENV) {
    $gptEnv = $env:PHOEBE_GPTSOVITS_ENV
} else {
    $gptEnv = Join-Path $env:USERPROFILE "miniconda3\envs\GPTSoVits"
}

$requiredWeights = @(
    (Join-Path $gptRoot "GPT_weights_v2\phoebe_zh_v2-e15.ckpt"),
    (Join-Path $gptRoot "SoVITS_weights_v2\phoebe_zh_v2_e8_s912.pth"),
    (Join-Path $gptRoot "GPT_SoVITS\pretrained_models\chinese-roberta-wwm-ext-large"),
    (Join-Path $gptRoot "GPT_SoVITS\pretrained_models\chinese-hubert-base"),
    (Join-Path $gptRoot "GPT_SoVITS\pretrained_models\fast_langdetect")
)
$missing = $requiredWeights | Where-Object { -not (Test-Path $_) }
if ($missing) {
    $list = ($missing -join "`n  ")
    Fail "GPT-SoVITS inputs are incomplete at '$gptRoot'. Missing:`n  $list`nCopy them from the macOS machine (see docs/windows-release.md section 2.1)."
}
Write-Host "[OK] GPT-SoVITS weights at $gptRoot"

if (-not (Test-Path (Join-Path $gptEnv "conda-meta"))) {
    Fail "conda environment not found at '$gptEnv'. Create the GPTSoVits env first (docs/windows-release.md section 2.2)."
}
Write-Host "[OK] conda env at $gptEnv"

# 3. Export paths for the release scripts
$env:PHOEBE_GPTSOVITS_ROOT = $gptRoot
$env:PHOEBE_GPTSOVITS_ENV = $gptEnv

Write-Host ""
Write-Host "=== npm ci ===" -ForegroundColor Cyan
npm ci

Write-Host ""
Write-Host "=== Building installer (voice runtime generation can take 10+ minutes on first run) ===" -ForegroundColor Cyan
npm run tauri:build -w @phoebe/desktop

Write-Host ""
Write-Host "=== Done ===" -ForegroundColor Green
Write-Host "Installer: target\release\bundle\nsis\*-setup.exe"
