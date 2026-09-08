#!/usr/bin/env python3
"""Inspect audio format and boundaries. This does not identify speakers or transcribe."""
from array import array
import csv
import hashlib
import json
import math
from pathlib import Path
import sys
import wave

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'assets/voice/roxy_vad_8-10s'

def dbfs(samples):
    energy = sum((v / 32768) ** 2 for v in samples) / max(1, len(samples))
    return round(10 * math.log10(max(energy, 1e-18)), 2)

def main():
    manifest = json.loads((SOURCE / 'manifest.json').read_text())
    entries = {x['file']: x for x in manifest['items']}
    records = []
    for path in sorted(SOURCE.glob('*.wav')):
        with wave.open(str(path), 'rb') as f:
            rate, channels, width, count = f.getframerate(), f.getnchannels(), f.getsampwidth(), f.getnframes()
            if width != 2 or channels != 1:
                raise SystemExit(f'Unexpected audio format: {path.name}')
            pcm = array('h', f.readframes(count))
        if sys.byteorder != 'little':
            pcm.byteswap()
        duration = count / rate
        assert abs(duration - entries[path.name]['duration_sec']) < .002
        edge = int(rate * .04)
        head, tail = dbfs(pcm[:edge]), dbfs(pcm[-edge:])
        records.append(dict(file=path.name, sample_rate=rate, channels=channels, bits=width * 8,
                            seconds=round(duration, 4), rms_dbfs=dbfs(pcm),
                            start_40ms_dbfs=head, end_40ms_dbfs=tail,
                            boundary_quiet_candidate=head < -40 and tail < -40,
                            clipped_sample_count=sum(abs(x) >= 32760 for x in pcm),
                            sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    assert set(entries) == {r['file'] for r in records}
    report = dict(count=len(records), total_seconds=round(sum(x['seconds'] for x in records), 3),
                  total_bytes=sum(p.stat().st_size for p in SOURCE.glob('*.wav')),
                  quiet_boundary_candidates=sum(x['boundary_quiet_candidate'] for x in records),
                  transcription_present=False, speaker_identity_verified=False,
                  note='Quiet boundaries do not prove sentence completeness or clean single-speaker audio.', files=records)
    (ROOT / 'data/voice-audit.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    table = ROOT / 'data/transcripts.tsv'
    if not table.exists():
        with table.open('w', newline='') as f:
            writer = csv.writer(f, delimiter='\t')
            writer.writerow(['file', 'language', 'text', 'reviewed', 'notes'])
            for row in records:
                writer.writerow([row['file'], 'ja', '', 'false', ''])
    candidates = sorted(records, key=lambda x: max(x['start_40ms_dbfs'], x['end_40ms_dbfs']))[:9]
    lines = ['# 日语参考语音候选', '', '仅按片段首尾的音量排序，尚未试听核对说话人、背景音乐、情绪和完整句子。', '',
             '请优先试听，随后在 `data/transcripts.tsv` 填写原声逐字日文；不要填写中文翻译。', '']
    for row in candidates:
        lines.append(f"- [{row['file']}](../assets/voice/roxy_vad_8-10s/{row['file']}) — {row['seconds']:.2f}s，首/尾 {row['start_40ms_dbfs']}/{row['end_40ms_dbfs']} dBFS")
    (ROOT / 'docs/reference-candidates.md').write_text('\n'.join(lines) + '\n')
    print(json.dumps({k: v for k, v in report.items() if k != 'files'}, ensure_ascii=False))

if __name__ == '__main__':
    main()

