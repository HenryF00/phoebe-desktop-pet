"""Official DeepSeek HTTPS/SSE client; caller owns the secret in memory only."""
import json
import time
import httpx

ENDPOINT = 'https://api.deepseek.com/chat/completions'

class DeepSeekChat:
    def __init__(self, key, model, transport=None):
        if not key: raise ValueError('请双击洛琪希，在设置中填写 DeepSeek API Key。')
        self.key = key; self.model = model; self.transport = transport
    def answer(self, turn, prompt, system, output_schema=None):
        with httpx.Client(transport=self.transport, timeout=httpx.Timeout(45,connect=10), trust_env=False) as client:
            with client.stream('POST', ENDPOINT, headers={'Authorization':f'Bearer {self.key}'}, json={
                'model':self.model, 'messages':[{'role':'system','content':system},{'role':'user','content':prompt}],
                'stream':True, 'response_format':{'type':'json_object'}, 'thinking':{'type':'disabled'},
                'max_tokens':1536}) as response:
                errors = {401:'DeepSeek 密钥无效，请在设置中检查。',402:'DeepSeek 余额不足，请检查账户。',
                          429:'DeepSeek 请求频率或额度受限，请稍后再试。'}
                if response.status_code in errors: raise ValueError(errors[response.status_code])
                if response.status_code >= 400: raise ValueError('DeepSeek 请求失败，请检查设置中的模型名称。')
                turn.on_cancel = response.close
                ended = False; has_text = False; deadline = time.monotonic()+180
                try:
                    for line in response.iter_lines():
                        if turn.cancelled.is_set(): return
                        if time.monotonic()>deadline: raise ValueError('DeepSeek 回复等待过久，已停止。')
                        if not line.startswith('data:'): continue
                        data = line[5:].strip()
                        if data == '[DONE]': break
                        event = json.loads(data)
                        for choice in event.get('choices', []):
                            delta = choice.get('delta',{}).get('content') or ''
                            if delta: has_text = True; yield delta
                            finish = choice.get('finish_reason')
                            if finish:
                                if finish != 'stop': raise ValueError('DeepSeek 回复未完整生成，请重试或缩短问题。')
                                ended = True
                    if not turn.cancelled.is_set() and (not ended or not has_text):
                        raise ValueError('DeepSeek 回复中断或为空，请重试。')
                finally: turn.on_cancel = None
