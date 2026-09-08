#!/usr/bin/env python3
"""Export reviewed Japanese annotations in upstream GPT-SoVITS .list format."""
import csv
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    with (ROOT / 'data/transcripts.tsv').open(newline='') as file:
        rows = list(csv.DictReader(file, delimiter='\t'))
    accepted = []
    for row in rows:
        if row['reviewed'].lower() != 'true':
            continue
        name = row['file']
        path = ROOT / 'assets/voice/roxy_vad_8-10s' / name
        text = row['text'].strip()
        if (Path(name).name != name or not path.is_file() or not text
                or row['language'] != 'ja' or any(c in text + str(path) for c in '|\r\n')):
            raise SystemExit(f'Invalid reviewed annotation: {name}')
        accepted.append(f'{path}|roxy|ja|{text}')
    if not accepted:
        raise SystemExit('尚无已校对字幕：未导出训练集，也未开始训练。')
    target = ROOT / 'generated/training/roxy.list'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('\n'.join(accepted) + '\n')
    print(f'Exported {len(accepted)}/{len(rows)} reviewed clips: {target}')


if __name__ == '__main__':
    main()
