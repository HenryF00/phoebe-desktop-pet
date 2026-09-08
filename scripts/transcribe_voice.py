#!/usr/bin/env python3
"""Local Japanese ASR; resumable drafts, never changes reviewed transcripts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import time
import wave

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault('HF_HOME', str(ROOT / 'models/huggingface'))
os.environ.setdefault('HF_HUB_DISABLE_TELEMETRY', '1')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--model', default='mlx-community/whisper-large-v3-turbo')
    parser.add_argument('--limit', type=int, default=0, help='0 means all clips')
    args = parser.parse_args()
    import mlx_whisper
    import numpy as np
    from scipy.signal import resample_poly

    output = ROOT / 'data/asr-drafts'
    output.mkdir(parents=True, exist_ok=True)
    files = sorted((ROOT / 'assets/voice/roxy_vad_8-10s').glob('*.wav'))
    if args.limit:
        files = files[:args.limit]
    for index, path in enumerate(files, 1):
        target = output / f'{path.stem}.json'
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if target.exists():
            previous = json.loads(target.read_text())
            if previous['source_sha256'] == digest and previous['model'] == args.model:
                print(f'[{index}/{len(files)}] cached {path.name}', flush=True)
                continue
        with wave.open(str(path)) as wav:
            if wav.getnchannels() != 1 or wav.getsampwidth() != 2:
                raise SystemExit(f'Expected mono PCM16: {path}')
            rate = wav.getframerate()
            samples = np.frombuffer(wav.readframes(wav.getnframes()), dtype='<i2').astype(np.float32) / 32768
        # Array input avoids an ffmpeg dependency for these known PCM WAV files.
        from math import gcd
        divisor = gcd(rate, 16000)
        samples = resample_poly(samples, 16000 // divisor, rate // divisor).astype(np.float32)
        start = time.perf_counter()
        result = mlx_whisper.transcribe(samples, path_or_hf_repo=args.model,
                                       language='ja', task='transcribe', temperature=0,
                                       condition_on_previous_text=False, verbose=False)
        record = dict(file=path.name, source_sha256=digest, model=args.model,
                      text=result['text'].strip(), language='ja', reviewed=False,
                      elapsed_seconds=round(time.perf_counter() - start, 3),
                      segments=result.get('segments', []))
        temporary = target.with_suffix('.tmp')
        temporary.write_text(json.dumps(record, ensure_ascii=False, indent=2) + '\n')
        temporary.replace(target)
        print(f'[{index}/{len(files)}] {path.name}: {record["text"]}', flush=True)
    print('ASR 草稿保存在 data/asr-drafts；未修改人工校对表。', flush=True)


if __name__ == '__main__':
    main()
