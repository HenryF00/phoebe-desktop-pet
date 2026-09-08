#!/usr/bin/env python3
"""Fill only empty, unreviewed TSV rows with hash-matched local ASR drafts."""
import csv
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    target = ROOT / 'data/transcripts.tsv'
    with target.open(newline='') as file:
        reader = csv.DictReader(file, delimiter='\t')
        fields = reader.fieldnames
        rows = list(reader)
    count = 0
    for row in rows:
        if row['text'].strip() or row['reviewed'].lower() == 'true':
            continue
        name = row['file']
        if Path(name).name != name:
            raise SystemExit(f'Invalid dataset filename: {name}')
        draft = ROOT / 'data/asr-drafts' / f'{Path(name).stem}.json'
        if not draft.exists():
            continue
        value = json.loads(draft.read_text())
        source = ROOT / 'assets/voice/roxy_vad_8-10s' / name
        if value['source_sha256'] != hashlib.sha256(source.read_bytes()).hexdigest():
            raise SystemExit(f'Audio changed since ASR: {name}')
        row['text'] = value['text']
        row['reviewed'] = 'false'
        row['notes'] = (row['notes'] + f'; ASR draft: {value["model"]}; 核对日文、切句、背景音和说话人').strip('; ')
        if any(s.get('compression_ratio', 0) > 2.4 for s in value.get('segments', [])):
            row['notes'] += '; 优先复核：ASR 检测到高重复度'
        if any(s.get('avg_logprob', 0) < -1 for s in value.get('segments', [])):
            row['notes'] += '; 优先复核：ASR 置信度低'
        count += 1
    temporary = target.with_suffix('.tmp')
    with temporary.open('w', newline='') as file:
        writer = csv.DictWriter(file, fieldnames=fields, delimiter='\t', lineterminator='\n')
        writer.writeheader()
        writer.writerows(rows)
    temporary.replace(target)
    print(f'Imported {count} drafts; no row was marked reviewed.')


if __name__ == '__main__':
    main()
