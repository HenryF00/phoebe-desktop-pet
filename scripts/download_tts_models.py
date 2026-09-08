#!/usr/bin/env python3
"""Download the upstream v2ProPlus inference/training weights, no audio upload."""
import os
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault('HF_HOME', str(ROOT / 'models/huggingface'))
os.environ.setdefault('HF_HUB_DISABLE_TELEMETRY', '1')
REVISION = '336b2ec4e8d4ac74740798dd40af44e74659ecaf'

if __name__ == '__main__':
    from huggingface_hub import hf_hub_download
    from concurrent.futures import ThreadPoolExecutor, as_completed
    files = ['chinese-hubert-base/config.json', 'chinese-hubert-base/preprocessor_config.json',
             'chinese-hubert-base/pytorch_model.bin', 'chinese-roberta-wwm-ext-large/config.json',
             'chinese-roberta-wwm-ext-large/tokenizer.json', 'chinese-roberta-wwm-ext-large/pytorch_model.bin',
             's1v3.ckpt', 'sv/pretrained_eres2netv2w24s4ep4.ckpt',
             'v2Pro/s2Gv2ProPlus.pth', 'v2Pro/s2Dv2ProPlus.pth']
    manifest = ROOT / 'config/tts-model-files.json'
    known = {r['file']: r['sha256'] for r in json.loads(manifest.read_text())} if manifest.exists() else {}
    def download(name):
        local = ROOT / 'models/gpt-sovits' / name
        if local.is_file() and name in known:
            with local.open('rb') as stream:
                if hashlib.file_digest(stream, 'sha256').hexdigest() == known[name]:
                    print(f'Verified cached: {name}', flush=True)
                    return str(local)
        result = hf_hub_download(repo_id='lj1995/GPT-SoVITS', revision=REVISION,
                                 filename=name, local_dir=ROOT / 'models/gpt-sovits')
        print(f'Ready: {name}', flush=True)
        return result
    with ThreadPoolExecutor(max_workers=2) as pool:
        futures = {pool.submit(download, name): name for name in files}
        failures = []
        for future in as_completed(futures):
            try:
                future.result()
            except Exception as exc:
                failures.append(futures[future])
                print(f'Failed: {futures[future]}: {type(exc).__name__}: {exc}', flush=True)
    if failures:
        raise SystemExit('Incomplete downloads; rerun to resume: ' + ', '.join(failures))
    print(ROOT / 'models/gpt-sovits')
