"""Minimal status-file observer; never reads Codex transcripts or task contents."""
from pathlib import Path
import json
import time

CAPTIONS = {'review':'这一轮处理结束了，来看看结果吧。', 'waiting':'有件事需要你确认，请看一下 Codex。'}

PHRASES = {
    'review': '今回の作業が終わりました。結果を確認してくださいね。',
    'waiting': '確認してほしいことがあります。コーデックスを見てくださいね。',
}


LOCALIZED = {
    'zh':CAPTIONS, 'ja':PHRASES,
    'en':{'review':'This turn has finished. Come take a look at the results.',
          'waiting':'Something needs your confirmation. Please check Codex.'},
}


class StatusObserver:
    def __init__(self, directory=None, now=None):
        self.directory = directory or Path.home() / 'Library/Application Support/RoxyHD/codex-status'
        self.started = time.time() if now is None else now
        self.seen = {}; self.baseline = False

    def poll(self, now=None):
        now = time.time() if now is None else now
        records = []
        for path in self.directory.glob('*.json'):
            try:
                v = json.loads(path.read_text())
                if not isinstance(v.get('updated'), (float,int)): continue
                if not -5 <= now - v['updated'] < 1800: continue
                if not isinstance(v.get('session'), str) or not isinstance(v.get('turn'), str): continue
                records.append(v)
            except (OSError, ValueError): continue
        notifications = []
        for v in sorted(records, key=lambda r:r['updated']):
            state = v.get('state')
            # Stop describes a finished turn, not proof of successful goal completion.
            if state not in PHRASES or (state == 'review' and v.get('event') != 'Stop'): continue
            key = (v['session'], v['turn'], state)
            if key in self.seen: continue
            self.seen[key] = now
            if self.baseline and v['updated'] > self.started:
                notifications.append(state)
        self.seen = {k:t for k,t in self.seen.items() if now-t < 3600}
        self.baseline = True
        # Coalesce identical announcements within one poll, retaining distinct statuses.
        return list(dict.fromkeys(notifications)), bool(records)
