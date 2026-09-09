"""Small, explicit, editable user notes. No inferred memories or model-written facts."""
import json
import os
from pathlib import Path
import re
import tempfile
import threading

LOCK = threading.Lock()
MAX_LENGTH = 4000

class CompanionMemory:
    def __init__(self, path): self.path = Path(path)
    def read(self):
        if not self.path.exists(): return {'enabled': True, 'notes': ''}
        try:
            value = json.loads(self.path.read_text())
            if not isinstance(value.get('notes'), str) or len(value['notes']) > MAX_LENGTH:
                raise ValueError()
            if not isinstance(value.get('enabled'), bool): raise ValueError()
            return {'enabled': value['enabled'], 'notes': value['notes']}
        except (OSError, ValueError, TypeError, AttributeError):
            raise ValueError('长期记忆无法读取，请在设置中检查并重新保存。')
    def write(self, value):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        fd, temp = tempfile.mkstemp(dir=self.path.parent, prefix='.memory-')
        try:
            with os.fdopen(fd, 'w') as f:
                json.dump(value, f, ensure_ascii=False); f.flush(); os.fsync(f.fileno())
            os.replace(temp, self.path)
        finally:
            if os.path.exists(temp): os.unlink(temp)
    def context(self, text):
        with LOCK:
            value = self.read(); notice = ''
            # Deliberate syntax avoids turning casual remarks or quoted history into facts.
            match = re.fullmatch(r'(?:请)?(记住|忘记)[：:]\s*(.+)', text.strip(), re.S)
            if value['enabled'] and match:
                action, note = match.groups(); note = note.strip()
                lines = value['notes'].splitlines()
                if '\n' in note or len(note) > 300:
                    notice = '记忆未修改：请每次提供一条不超过 300 字的内容。'
                elif action == '记住':
                    if note not in lines: lines.append(note)
                    notes = '\n'.join(lines)
                    if len(notes) > MAX_LENGTH: notice = '记忆已满，请先在设置中整理。'
                    else:
                        value['notes'] = notes; self.write(value); notice = '已记住：' + note
                elif note in lines:
                    lines.remove(note); value['notes'] = '\n'.join(lines); self.write(value)
                    notice = '已忘记：' + note
                else: notice = '没有找到完全匹配的记忆；可在设置中编辑。'
            elif match:
                notice = '长期记忆已关闭，本次没有保存或删除。'
            return (value['notes'] if value['enabled'] else ''), notice
