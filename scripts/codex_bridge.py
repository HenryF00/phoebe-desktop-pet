#!/usr/bin/env python3
"""Read Codex hook JSON from stdin. Persist status only; never transcript or tool input."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time

EVENTS = {'UserPromptSubmit', 'PreToolUse', 'PostToolUse', 'PermissionRequest', 'Stop', 'Interrupt', 'SessionEnd'}

def update(root, event, now=None):
    now = time.time() if now is None else now
    name = event.get('hook_event_name')
    session = event.get('session_id')
    if name not in EVENTS or not isinstance(session, str) or not session:
        return
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    key = hashlib.sha256(session.encode()).hexdigest()[:24]
    target = root / (key + '.json')
    with (root / (key + '.lock')).open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            previous = json.loads(target.read_text())
        except (OSError, ValueError):
            previous = {}
        turn = event.get('turn_id') or previous.get('turn', '')
        same_turn = previous.get('turn') == turn
        # Late parallel tool completions must not resurrect a finished turn.
        if same_turn and previous.get('event') in ('Stop', 'Interrupt', 'SessionEnd') and name in ('PreToolUse', 'PostToolUse', 'PermissionRequest'):
            return
        waiting = set(previous.get('waiting', [])) if same_turn else set()
        tool_id = str(event.get('tool_use_id') or 'approval')
        if name == 'UserPromptSubmit':
            waiting.clear()
        if name == 'PermissionRequest':
            waiting.add(tool_id)
        if name == 'PreToolUse' and 'request_user_input' in str(event.get('tool_name', '')):
            waiting.add(tool_id)
        if name == 'PostToolUse':
            waiting.discard(tool_id)
        state = 'waiting' if waiting else 'running'
        if name in ('Stop', 'Interrupt', 'SessionEnd'):
            waiting.clear()
            state = 'review' if name == 'Stop' else 'idle'
        record = dict(version=1, session=key, turn=turn, state=state, event=name, updated=now, waiting=sorted(waiting))
        fd, temp = tempfile.mkstemp(prefix=key, suffix='.tmp', dir=root)
        try:
            with os.fdopen(fd, 'w') as out:
                json.dump(record, out)
            os.replace(temp, target)
        finally:
            if os.path.exists(temp):
                os.unlink(temp)

def main():
    try:
        event = json.loads(sys.stdin.read(8 * 1024 * 1024))
        root = Path.home() / 'Library/Application Support/RoxyHD/codex-status'
        update(root, event)
    except Exception:
        # A pet must never block a Codex turn, including on a full disk.
        pass
    print('{}')

if __name__ == '__main__':
    main()
