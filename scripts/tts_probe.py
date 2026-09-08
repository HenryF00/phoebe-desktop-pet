#!/usr/bin/env python3
"""Buffered synthesis probe for a separately running local GPT-SoVITS api_v2 server.

This is a quality/throughput probe, not the desktop streaming playback integration.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import time
from urllib.parse import urlparse
from urllib.request import Request, urlopen
import wave

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--text', default='こんにちは。今日も一緒に頑張りましょう。')
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--draft-reference', metavar='WAV_FILENAME',
                        help='Explicit unreviewed ASR reference for a draft preview only')
    args = parser.parse_args()
    config = json.loads((ROOT / 'config/voice.json').read_text())
    if args.draft_reference:
        name = args.draft_reference
        if Path(name).name != name:
            raise SystemExit('Expected a dataset WAV filename.')
        reference = ROOT / 'assets/voice/roxy_vad_8-10s' / name
        draft_path = ROOT / 'data/asr-drafts' / f'{Path(name).stem}.json'
        if not reference.is_file() or not draft_path.is_file():
            raise SystemExit('Missing dataset audio or ASR draft.')
        draft = json.loads(draft_path.read_text())
        if hashlib.sha256(reference.read_bytes()).hexdigest() != draft['source_sha256'] or not draft['text'].strip():
            raise SystemExit('Draft reference text is empty or audio has changed.')
        config.update(reference_audio=str(reference.relative_to(ROOT)), reference_text=draft['text'],
                      reference_reviewed=False)
        print('未校对试听：使用 ASR 草稿参考文本，不修改正式音色配置。')
    elif not config.get('reference_reviewed') or not config.get('reference_text') or not config.get('reference_audio'):
        raise SystemExit('参考音频和逐字日文尚未配置，请先运行 prepare_reference.py。')
    reference = (ROOT / config['reference_audio']).resolve()
    if not reference.is_file() or not reference.is_relative_to(ROOT):
        raise SystemExit('Invalid or missing project reference audio.')
    endpoint = config['endpoint']
    parsed = urlparse(endpoint)
    if parsed.scheme != 'http' or parsed.hostname not in ('127.0.0.1', 'localhost', '::1'):
        raise SystemExit('此探针仅连接本机 GPT-SoVITS 服务。')
    payload = dict(text=args.text, text_lang='ja', ref_audio_path=str(reference),
                   prompt_text=config['reference_text'], prompt_lang='ja',
                   text_split_method='cut5', batch_size=1, media_type='wav', streaming_mode=False, seed=42)
    if args.dry_run:
        print(json.dumps(payload, ensure_ascii=False, indent=2)); return
    request = Request(endpoint, data=json.dumps(payload).encode(), headers={'Content-Type': 'application/json'}, method='POST')
    start = time.perf_counter()
    with urlopen(request, timeout=180) as response:
        data = response.read()
    elapsed = time.perf_counter() - start
    with wave.open(io.BytesIO(data)) as audio:
        duration = audio.getnframes() / audio.getframerate()
        if duration <= 0:
            raise SystemExit('TTS returned empty audio.')
    dest = ROOT / 'generated'; dest.mkdir(exist_ok=True)
    stem = 'draft-preview' if args.draft_reference else 'probe'
    (dest / f'{stem}.wav').write_bytes(data)
    report = dict(generation_seconds=elapsed, audio_seconds=duration, realtime_factor=elapsed / duration,
                  text=args.text, reference_audio=config['reference_audio'],
                  reference_text=config['reference_text'], reference_reviewed=config['reference_reviewed'],
                  seed=payload['seed'], audio_path=str(dest / f'{stem}.wav'),
                  mode='buffered; not a first-audio-latency or peak-memory measurement')
    (dest / f'{stem}.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(report, ensure_ascii=False, indent=2))

if __name__ == '__main__':
    main()
