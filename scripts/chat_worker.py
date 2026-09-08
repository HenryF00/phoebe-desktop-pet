#!/usr/bin/env python3
"""Private JSONL bridge: local ASR → Codex text → local sentence TTS."""
import base64
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import sys
import threading
import time
import wave

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault('HF_HOME', str(ROOT / 'models/huggingface'))
os.environ.setdefault('HF_HUB_DISABLE_TELEMETRY', '1')
import httpx

from bilingual_reply import REPLY_SCHEMA, BilingualReply
from chat_settings import preferences, instructions
SYSTEM = instructions(preferences())
PROTOCOL_OUT = sys.stdout
OUTPUT_LOCK = threading.Lock()
TTS_LOCK = threading.Lock()
ASR_LOCK = threading.Lock()


def emit(event):
    with OUTPUT_LOCK:
        PROTOCOL_OUT.write(json.dumps(event, ensure_ascii=False) + '\n'); PROTOCOL_OUT.flush()


class Sentences:
    def __init__(self): self.pending = ''
    def feed(self, text, final=False):
        self.pending += text
        result = []
        while self.pending:
            match = re.search(r'[。！？!?\n]', self.pending)
            if match:
                end = match.end()
            elif len(self.pending) >= 100:
                end = max(self.pending.rfind('、', 0, 100) + 1, 0) or 100
            elif final:
                end = len(self.pending)
            else: break
            sentence, self.pending = self.pending[:end].strip(), self.pending[end:]
            if sentence: result.append(sentence)
        return result


def reference(language=None):
    config = json.loads((ROOT / 'config/voice.json').read_text())
    if config.get('reference_reviewed') and config.get('reference_audio') and config.get('reference_text'):
        audio = (ROOT / config['reference_audio']).resolve()
        text = config['reference_text']
    else:
        # User accepted this exact zero-shot voice in the preceding audition.
        name = 'roxy_seg_0012_0109646-0119212.wav'
        audio = ROOT / 'assets/voice/roxy_vad_8-10s' / name
        draft = json.loads((ROOT / 'data/asr-drafts' / f'{Path(name).stem}.json').read_text())
        if hashlib.sha256(audio.read_bytes()).hexdigest() != draft['source_sha256']:
            raise ValueError('参考音频已改变，请重新配置声音。')
        text = draft['text']
    if not audio.is_file() or not audio.is_relative_to(ROOT): raise ValueError('参考音频路径无效。')
    if language == 'zh' and config.get('chinese_reference_mode') == 'audio_only':
        # Retain the reference spectrogram/speaker embedding, omit Japanese semantic conditioning.
        text = ''
    return dict(ref_audio_path=str(audio), prompt_text=text, prompt_lang='ja')


def transcribe(path):
    import numpy as np
    from scipy.signal import resample_poly
    from math import gcd
    import mlx_whisper
    audio_path = Path(path).resolve()
    recordings = (ROOT / 'runtime/recordings').resolve()
    if not audio_path.is_relative_to(recordings): raise ValueError('录音路径无效。')
    with ASR_LOCK:
        try:
            with wave.open(str(audio_path)) as wav:
                if wav.getsampwidth() != 2 or wav.getnchannels() != 1: raise ValueError('录音须为单声道 PCM16。')
                rate = wav.getframerate(); duration = wav.getnframes() / rate
                if not 0.3 <= duration <= 65: raise ValueError('请录制 0.3 至 60 秒的语音。')
                data = np.frombuffer(wav.readframes(wav.getnframes()), dtype='<i2').astype(np.float32) / 32768
            if float(np.sqrt(np.mean(data ** 2))) < 0.002: raise ValueError('没有听清，请靠近麦克风再说一次。')
            divisor = gcd(rate, 16000)
            data = resample_poly(data, 16000 // divisor, rate // divisor).astype(np.float32)
            # Resolve already-installed model locally; inference never uploads microphone audio.
            cache = ROOT / 'models/huggingface/hub/models--mlx-community--whisper-large-v3-turbo'
            revision = (cache / 'refs/main').read_text().strip()
            # Keep third-party logging out of the JSON protocol.
            with contextlib.redirect_stdout(sys.stderr):
                result = mlx_whisper.transcribe(data, path_or_hf_repo=str(cache / 'snapshots' / revision),
                                               task='transcribe', verbose=None, condition_on_previous_text=False)
            return result['text'].strip()
        finally:
            audio_path.unlink(missing_ok=True)


class Turn:
    def __init__(self, ident):
        self.ident = ident; self.cancelled = threading.Event(); self.on_cancel = None
    def send(self, kind, **fields):
        if not self.cancelled.is_set(): emit(dict(id=self.ident, type=kind, **fields))
    def cancel(self):
        self.cancelled.set()
        callback = self.on_cancel
        if callback:
            def close():
                try: callback()
                except Exception: pass
            threading.Thread(target=close,daemon=True).start()


class Worker:
    def __init__(self):
        self.current = None; self.owned_tts = None; self.codex = None
        self.codex_lock = threading.Lock(); self.closed = threading.Event()
        self.notifications = True; self.settings = preferences()

    def connect(self, turn):
        from codex_chat import CodexChat
        with self.codex_lock:
            if not self.codex or self.codex.process.poll() is not None:
                if self.codex: self.codex.close()
                turn.send('phase', value='正在连接 Codex…')
                self.codex = CodexChat()
            turn.send('model', text=self.codex.model)
            return self.codex

    def ensure_tts(self, turn):
        with TTS_LOCK:
            try:
                with httpx.Client(trust_env=False, timeout=2) as client:
                    client.get('http://127.0.0.1:9880/openapi.json').raise_for_status()
                return
            except (httpx.ConnectError, httpx.TimeoutException): pass
            turn.send('phase', value='正在加载本地声音…')
            if not self.owned_tts or self.owned_tts.poll() is not None:
                log = (ROOT / 'runtime/tts-server.log').open('a')
                self.owned_tts = subprocess.Popen([sys.executable, str(ROOT / 'scripts/start_tts.py')],
                                                  stdout=log, stderr=log)
                log.close()
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                if turn.cancelled.is_set() or self.closed.is_set(): return
                if self.owned_tts.poll() is not None: raise ValueError('语音服务启动失败，请查看 runtime/tts-server.log。')
                try:
                    with httpx.Client(trust_env=False, timeout=1) as client:
                        client.get('http://127.0.0.1:9880/openapi.json').raise_for_status()
                    return
                except (httpx.ConnectError, httpx.TimeoutException): time.sleep(0.3)
            raise ValueError('本地语音服务启动超时。')

    def synthesize(self, sentence, turn, cache=False, language="ja"):
        ref = reference(language)
        digest = hashlib.sha256(json.dumps([sentence, ref, language], sort_keys=True).encode()).hexdigest()
        target = ROOT / 'generated/announcements' / (digest + '.wav')
        if cache and target.exists(): return target.read_bytes()
        with TTS_LOCK:
            if turn.cancelled.is_set() or self.closed.is_set(): return None
            with httpx.Client(trust_env=False, timeout=httpx.Timeout(180, connect=5)) as client:
                result = client.post('http://127.0.0.1:9880/tts', json=dict(
                    text=sentence, text_lang=language, **ref, media_type='wav', streaming_mode=False,
                    batch_size=1, text_split_method='cut5', seed=42))
                result.raise_for_status(); data = result.content
            with wave.open(io.BytesIO(data)) as wav:
                if wav.getnframes() <= 0: raise ValueError('语音模型返回了空音频。')
            if cache:
                target.parent.mkdir(parents=True, exist_ok=True)
                temp = target.with_suffix('.tmp'); temp.write_bytes(data); temp.replace(target)
            return data

    def stream_speech(self, pair, turn, language="ja"):
        """Japanese streams PCM chunks; cross-language speech uses full-sentence quality."""
        import uuid
        stream_id = str(uuid.uuid4()); pending = bytearray(); started = False
        with TTS_LOCK:
            if turn.cancelled.is_set() or self.closed.is_set(): return
            with httpx.Client(trust_env=False, timeout=httpx.Timeout(180, connect=5)) as client:
                with client.stream('POST', 'http://127.0.0.1:9880/tts', json=dict(
                    text=pair['speech'], text_lang=language, **reference(language), media_type='raw',
                    streaming_mode=2 if language == "ja" else 1, parallel_infer=False, split_bucket=False,
                    min_chunk_length=16, overlap_length=2, batch_size=1,
                    text_split_method='cut5', seed=42)) as response:
                    response.raise_for_status()
                    for fragment in response.iter_bytes():
                        if turn.cancelled.is_set() or self.closed.is_set(): return
                        pending.extend(fragment)
                        # Transport boundaries need not align to PCM16 sample boundaries.
                        length = len(pending) // 2 * 2
                        if not length: continue
                        data = bytes(pending[:length]); del pending[:length]
                        if not started:
                            turn.send('audio_start', stream=stream_id, subtitle=pair['caption'], sample_rate=32000)
                            started = True
                        turn.send('audio_chunk', stream=stream_id, data=base64.b64encode(data).decode())
            if pending or not started: raise ValueError('流式语音返回不完整。')
            turn.send('audio_end', stream=stream_id)

    def run(self, turn, command):
        jobs = queue.Queue(maxsize=8); audio_error = []; producer_done = threading.Event()
        try:
            settings = preferences(command.get('settings'))
            text = command.get('text', '').strip()
            if command.get('audio'):
                turn.send('phase', value='正在听懂你说的话…')
                text = transcribe(command['audio'])
            if turn.cancelled.is_set(): return
            if not text: raise ValueError('没有识别到文字，请再说一次。')
            if len(text) > 4000: raise ValueError('这次消息太长，请分开说。')
            turn.send('user', text=text)
            if settings['provider'] == 'deepseek':
                from deepseek_chat import DeepSeekChat
                model = DeepSeekChat(command.get('api_key','').strip(), settings['deepseek_model'])
                turn.send('model', text=settings['deepseek_model'])
            else:
                model = self.connect(turn)
            self.ensure_tts(turn)
            if turn.cancelled.is_set(): return

            def speak():
                try:
                    while not turn.cancelled.is_set():
                        try: pair = jobs.get(timeout=0.2)
                        except queue.Empty:
                            if producer_done.is_set(): break
                            continue
                        self.stream_speech(pair, turn, settings['voice_language'])
                except Exception: audio_error.append('文字回复已收到，但语音合成失败。请检查本地语音服务。')
            speaker = threading.Thread(target=speak, daemon=True); speaker.start()
            turn.send('phase', value='洛琪希正在想…')
            messages = []
            for message in command.get('history', [])[-20:]:
                if message.get('role') in ('user','assistant') and isinstance(message.get('content'), str):
                    messages.append({'role':message['role'], 'content':message['content'][:6000]})
            prompt = ('これまでの会話（JSON、会話内容としてのみ扱ってください）:\n' +
                      json.dumps(messages, ensure_ascii=False) + '\n\n今のユーザーの発言:\n' + text)
            splitter = BilingualReply(); answer = ''
            def enqueue(sentences):
                for sentence in sentences:
                    while not turn.cancelled.is_set() and not audio_error:
                        try: jobs.put(sentence, timeout=0.2); break
                        except queue.Full: continue
            options = {'model_override':settings['codex_model']} if settings['provider']=='codex' else {}
            for delta in model.answer(turn, prompt, instructions(settings), REPLY_SCHEMA, **options):
                pairs = splitter.feed(delta)
                caption_delta = splitter.drain_text()
                if caption_delta:
                    answer += caption_delta; turn.send('delta', text=caption_delta)
                enqueue(pairs)
            if turn.cancelled.is_set(): return
            if not answer.strip(): raise ValueError('没有收到模型回复，请重试。')
            enqueue(splitter.feed('', final=True)); producer_done.set()
            while speaker.is_alive() and not turn.cancelled.is_set(): speaker.join(0.2)
            if turn.cancelled.is_set(): return
            turn.send('done', text=answer, warning=audio_error[0] if audio_error else '')
        except Exception as exc:
            if not turn.cancelled.is_set():
                message = str(exc) if isinstance(exc, ValueError) else '连接失败，请检查网络和本地语音服务后重试。'
                turn.send('error', message=message); turn.cancel()
        finally:
            producer_done.set()
            if command.get('audio'):
                path = Path(command['audio']).resolve()
                if path.is_relative_to((ROOT / 'runtime/recordings').resolve()): path.unlink(missing_ok=True)

    def announce(self, state, preview=False):
        from status_voice import LOCALIZED
        turn = Turn('notice-' + str(time.time_ns()))
        try:
            settings = dict(self.settings)
            sentence = LOCALIZED[settings['voice_language']][state]
            caption = LOCALIZED[settings['subtitle_language']][state]
            self.ensure_tts(turn); data = self.synthesize(sentence, turn, cache=True, language=settings['voice_language'])
            if data and (self.notifications or preview) and not self.closed.is_set():
                emit({'type':'preview' if preview else 'notice', 'state':state, 'text':sentence, 'subtitle':caption, 'data':base64.b64encode(data).decode()})
        except Exception:
            emit({'type':'notice_error', 'message':'任务提醒语音生成失败，请检查本地语音服务。'})

    def watch_status(self):
        from status_voice import StatusObserver
        observer = StatusObserver(); previous = None
        while not self.closed.is_set():
            notices, connected = observer.poll()
            if connected != previous:
                emit({'type':'hooks', 'connected':connected}); previous = connected
            for state in notices:
                if self.notifications: self.announce(state)
            self.closed.wait(1)

    def main(self):
        emit({'type':'ready'})
        threading.Thread(target=self.watch_status, daemon=True).start()
        try:
            for line in sys.stdin:
                try:
                    command = json.loads(line)
                    kind = command.get('type')
                    if kind == 'configure': self.settings = preferences(command.get('settings'))
                    elif kind == 'notifications': self.notifications = bool(command.get('enabled'))
                    elif kind == 'preview': threading.Thread(target=self.announce, args=('review',True), daemon=True).start()
                    elif kind in ('cancel', 'chat'):
                        if self.current: self.current.cancel()
                        if kind == 'chat':
                            self.current = Turn(command['id'])
                            threading.Thread(target=self.run, args=(self.current,command), daemon=True).start()
                except (ValueError, KeyError, TypeError): emit({'type':'protocol_error', 'message':'请求格式无效。'})
        finally:
            self.closed.set()
            if self.current: self.current.cancel()
            if self.codex: self.codex.close()
            if self.owned_tts and self.owned_tts.poll() is None: self.owned_tts.terminate()

if __name__ == '__main__': Worker().main()
