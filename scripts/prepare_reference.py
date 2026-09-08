#!/usr/bin/env python3
"""Select one reviewed Japanese transcript as the reference for local synthesis."""
import argparse
import csv
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('file', help='WAV filename from data/transcripts.tsv')
    args = parser.parse_args()
    with (ROOT / 'data/transcripts.tsv').open(newline='') as f:
        rows = {r['file']: r for r in csv.DictReader(f, delimiter='\t')}
    row = rows.get(args.file)
    if not row or row['reviewed'].lower() != 'true' or not row['text'].strip():
        raise SystemExit('先在 data/transcripts.tsv 填写逐字日文、核对音频，并将 reviewed 标为 true。')
    path = ROOT / 'assets/voice/roxy_vad_8-10s' / args.file
    if path.parent != ROOT / 'assets/voice/roxy_vad_8-10s' or not path.is_file():
        raise SystemExit('Reference file is not present in the imported dataset.')
    config = ROOT / 'config/voice.json'
    value = json.loads(config.read_text())
    value.update(reference_audio=str(path.relative_to(ROOT)), reference_text=row['text'].strip(),
                 reference_reviewed=True, status='reference_ready; local TTS model/service still required')
    config.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    print('参考音频已配置；尚未生成、训练或验证任何 AI 音频。')

if __name__ == '__main__':
    main()

