#!/usr/bin/env python3
"""Build a platform-local, relocatable GPT-SoVITS runtime for the Tauri bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import tarfile
import tempfile


RUNTIME_FORMAT = 1
GPT_WEIGHT = Path("GPT_weights_v2/phoebe_zh_v2-e15.ckpt")
SOVITS_WEIGHT = Path("SoVITS_weights_v2/phoebe_zh_v2_e8_s912.pth")
BASE_MODELS = (
    Path("GPT_SoVITS/pretrained_models/chinese-roberta-wwm-ext-large"),
    Path("GPT_SoVITS/pretrained_models/chinese-hubert-base"),
    Path("GPT_SoVITS/pretrained_models/fast_langdetect"),
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(8 * 1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def archive_filter(info: tarfile.TarInfo) -> tarfile.TarInfo | None:
    parts = PurePosixPath(info.name).parts
    if any(part in {".git", "__pycache__", ".mypy_cache", ".pytest_cache"} for part in parts):
        return None
    if info.name.endswith((".pyc", ".pyo", ".DS_Store")):
        return None
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    info.mtime = 0
    return info


def add_path(archive: tarfile.TarFile, source: Path, target: PurePosixPath) -> None:
    archive.add(source, arcname=str(target), recursive=True, filter=archive_filter)


def make_project_archive(source: Path, destination: Path) -> None:
    with tarfile.open(destination, "w:gz", compresslevel=6) as archive:
        for name in ("api_v2.py", "config.py", "LICENSE"):
            add_path(archive, source / name, PurePosixPath("gpt-sovits") / name)
        add_path(archive, source / "tools", PurePosixPath("gpt-sovits/tools"))

        # Add inference sources but omit the multi-version pretrained model collection.
        for child in sorted((source / "GPT_SoVITS").iterdir()):
            if child.name in {"pretrained_models", "__pycache__"}:
                continue
            add_path(archive, child, PurePosixPath("gpt-sovits/GPT_SoVITS") / child.name)

        for model in BASE_MODELS:
            add_path(archive, source / model, PurePosixPath("gpt-sovits") / model.as_posix())
        add_path(archive, source / GPT_WEIGHT, PurePosixPath("gpt-sovits") / GPT_WEIGHT.as_posix())
        add_path(archive, source / SOVITS_WEIGHT, PurePosixPath("gpt-sovits") / SOVITS_WEIGHT.as_posix())

        config = (
            "custom:\n"
            "  bert_base_path: GPT_SoVITS/pretrained_models/chinese-roberta-wwm-ext-large\n"
            "  cnhuhbert_base_path: GPT_SoVITS/pretrained_models/chinese-hubert-base\n"
            "  device: cpu\n"
            "  is_half: false\n"
            f"  t2s_weights_path: {GPT_WEIGHT.as_posix()}\n"
            "  version: v2\n"
            f"  vits_weights_path: {SOVITS_WEIGHT.as_posix()}\n"
        ).encode()
        info = tarfile.TarInfo("gpt-sovits/phoebe_tts.yaml")
        info.size = len(config)
        info.mode = 0o644
        info.mtime = 0
        import io
        archive.addfile(info, io.BytesIO(config))


def normalized_platform() -> tuple[str, str]:
    system = platform.system().lower()
    target = {"darwin": "macos", "windows": "windows"}.get(system)
    if target is None:
        raise SystemExit(f"Unsupported release platform: {system}")
    machine = platform.machine().lower()
    arch = {"arm64": "aarch64", "amd64": "x86_64", "x86_64": "x86_64"}.get(machine)
    if arch is None:
        raise SystemExit(f"Unsupported release architecture: {machine}")
    return target, arch


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--environment", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    source = args.source.resolve()
    environment = args.environment.resolve()
    output = args.output.resolve()
    required = [source / "api_v2.py", source / "LICENSE", source / GPT_WEIGHT,
                source / SOVITS_WEIGHT, *(source / item for item in BASE_MODELS)]
    missing = [str(path) for path in required if not path.exists()]
    if missing:
        raise SystemExit("GPT-SoVITS release inputs are incomplete:\n" + "\n".join(missing))
    if not (environment / "conda-meta").is_dir():
        raise SystemExit(f"Expected a conda environment, got: {environment}")

    try:
        import conda_pack
    except ImportError as error:
        raise SystemExit(
            "conda-pack is required in the selected GPT-SoVITS environment. "
            "Install it with: python -m pip install conda-pack"
        ) from error

    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="phoebe-voice-pack-", dir=output.parent) as temporary:
        temporary_path = Path(temporary)
        python_archive = temporary_path / "python-env.tar.gz"
        project_archive = temporary_path / "gpt-sovits.tar.gz"
        conda_pack.pack(prefix=str(environment), output=str(python_archive), format="tgz", force=True)
        make_project_archive(source, project_archive)

        target_platform, architecture = normalized_platform()
        python_hash = sha256(python_archive)
        project_hash = sha256(project_archive)
        runtime_id = f"v{RUNTIME_FORMAT}-{target_platform}-{architecture}-{python_hash[:12]}-{project_hash[:12]}"
        manifest = {
            "format": RUNTIME_FORMAT,
            "runtime_id": runtime_id,
            "platform": target_platform,
            "architecture": architecture,
            "python_archive": {"file": python_archive.name, "sha256": python_hash},
            "project_archive": {"file": project_archive.name, "sha256": project_hash},
            "api_script": "gpt-sovits/api_v2.py",
            "config": "gpt-sovits/phoebe_tts.yaml",
            "gpt_weight": GPT_WEIGHT.as_posix(),
            "sovits_weight": SOVITS_WEIGHT.as_posix(),
            "upstream_license": "GPT-SoVITS-LICENSE",
        }
        manifest_path = temporary_path / "manifest.json"
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
        shutil.copy2(source / "LICENSE", temporary_path / "GPT-SoVITS-LICENSE")

        for path in output.iterdir():
            if path.is_file():
                path.unlink()
        for path in temporary_path.iterdir():
            shutil.move(str(path), output / path.name)

    print(json.dumps({"runtime_id": runtime_id, "output": str(output)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
