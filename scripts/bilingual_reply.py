"""Incrementally decode independently localized captions and speech."""
import json
import re

EXPRESSIONS = ('calm', 'shy', 'nod', 'sad', 'thoughtful')

REPLY_SCHEMA = {'type':'object', 'additionalProperties':False,
    'properties':{'segments':{'type':'array', 'minItems':1, 'maxItems':3, 'items':{
        'type':'object', 'additionalProperties':False, 'properties':{
            'caption':{'type':'string'}, 'speech':{'type':'string'},
            'expression':{'type':'string', 'enum':list(EXPRESSIONS)}}, 'required':['caption','speech','expression']}}},
    'required':['segments']}

class BilingualReply:
    def __init__(self):
        self.raw = ''; self.cursor = None; self.items = []; self.displayed = ''
    def feed(self, delta, final=False):
        self.raw += delta; result = []
        if self.cursor is None:
            start = self.raw.find('[')
            if start >= 0: self.cursor = start + 1
        if self.cursor is not None:
            while True:
                while self.cursor < len(self.raw) and self.raw[self.cursor] in ' \r\n\t,': self.cursor += 1
                if self.cursor == len(self.raw) or self.raw[self.cursor] == ']': break
                try: item, end = json.JSONDecoder().raw_decode(self.raw, self.cursor)
                except json.JSONDecodeError: break
                if (not isinstance(item, dict) or not {'caption','speech'} <= set(item)
                        or set(item) - {'caption','speech','expression'} or any(
                        not isinstance(item[k], str) or not item[k].strip() for k in ('caption','speech'))
                        or item.get('expression', 'calm') not in EXPRESSIONS):
                    raise ValueError('字幕生成格式不完整，请重试。')
                self.cursor = end; self.items.append(item); result.append(item)
        if final:
            try: value = json.loads(self.raw)
            except json.JSONDecodeError: raise ValueError('字幕生成中断，请重试。')
            if value != {'segments':self.items} or not 1 <= len(self.items) <= 3:
                raise ValueError('没有收到完整的字幕与语音文本，请重试。')
        return result

    def drain_text(self):
        """Expose only decoded caption string contents, even before a pair finishes.

        Keep an unfinished escape or UTF-16 surrogate pair buffered; JSON syntax,
        speech translations and partial escape sequences never reach the UI.
        """
        texts = []
        for match in re.finditer(r'(?<!\\)"caption"\s*:\s*"', self.raw):
            start = match.end(); end = start; escaped = False
            while end < len(self.raw):
                char = self.raw[end]
                if char == '"' and not escaped: break
                if char == '\\' and not escaped: escaped = True
                else: escaped = False
                end += 1
            fragment = self.raw[start:end]
            decoded = ''
            for trim in range(min(12, len(fragment)) + 1):
                candidate = fragment[:len(fragment)-trim] if trim else fragment
                try: value = json.loads('"' + candidate + '"')
                except json.JSONDecodeError: continue
                if any(0xD800 <= ord(ch) <= 0xDFFF for ch in value): continue
                decoded = value; break
            texts.append(decoded)
        full = ''.join(texts)
        if not full.startswith(self.displayed):
            raise ValueError('流式字幕解析失败，请重试。')
        delta = full[len(self.displayed):]; self.displayed = full
        return delta
