"""Public Codex app-server client. Managed login; no credential extraction.

Each answer uses an ephemeral, environment-free conversation with bounded text
history supplied by our UI. No work-task IDs are ever accepted from the UI.
"""
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import threading
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
DISABLED = ('shell_tool', 'unified_exec', 'code_mode', 'code_mode_host', 'apps',
            'plugins', 'browser_use', 'browser_use_external', 'computer_use',
            'in_app_browser', 'image_generation', 'multi_agent', 'multi_agent_v2',
            'memories', 'chronicle', 'hooks', 'skill_search', 'view_image',
            'workspace_dependencies', 'goals', 'sleep_tool')


def launch_args():
    binary = shutil.which('codex') or str(Path.home() / '.npm-global/bin/codex')
    args = [binary, 'app-server', '--stdio', '-c', 'web_search="disabled"',
            '-c', 'model_provider="openai"', '-c', 'project_doc_max_bytes=0']
    for feature in DISABLED:
        args += ['--disable', feature]
    args += ['-c', 'mcp_servers={}']
    return args


class CodexChat:
    def __init__(self):
        self.pending = {}; self.lock = threading.Lock(); self.serial = 0
        self.events = {}; self.closed = False
        self.cwd = ROOT / 'runtime/chat-empty'; self.cwd.mkdir(parents=True, exist_ok=True)
        self.log = (ROOT / 'runtime/codex-chat.log').open('a')
        env = dict(os.environ)
        for key in ('OPENAI_API_KEY', 'CODEX_API_KEY', 'CODEX_THREAD_ID'):
            env.pop(key, None)
        self.process = subprocess.Popen(launch_args(), cwd=self.cwd, env=env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log,
            text=True, bufsize=1)
        threading.Thread(target=self._read, daemon=True).start()
        try:
            self.request('initialize', {'clientInfo': {'name':'roxy_pet', 'title':'Roxy voice companion',
                'version':'1.2.0'}, 'capabilities': {'experimentalApi':True}})
            self.notify('initialized')
            account = self.request('account/read', {})
            if (account.get('account') or {}).get('type') != 'chatgpt':
                raise ValueError('请先在终端运行 codex login，使用 ChatGPT 账号登录，再重试。')
            models = self.request('model/list', {'includeHidden':False})['data']
            model = next((m for m in models if m['isDefault']), models[0])
            self.model = model['model']
            efforts = [v['reasoningEffort'] for v in model['supportedReasoningEfforts']]
            self.effort = 'low' if 'low' in efforts else model['defaultReasoningEffort']
        except Exception:
            self.close(); raise

    def _write(self, value):
        with self.lock:
            if self.closed: raise ValueError('Codex 连接已关闭，请重试。')
            self.process.stdin.write(json.dumps(value) + '\n'); self.process.stdin.flush()

    def notify(self, method, params=None):
        self._write({'method':method, 'params':params or {}})

    def request(self, method, params, timeout=45):
        reply = queue.Queue()
        with self.lock:
            self.serial += 1; ident = self.serial; self.pending[ident] = reply
        try:
            self._write({'id':ident, 'method':method, 'params':params})
            try: message = reply.get(timeout=timeout)
            except queue.Empty: raise ValueError('Codex 响应超时，请检查网络后重试。')
            if 'error' in message:
                # Full errors can contain prompt/config data; keep them out of UI/logs.
                raise ValueError('Codex 请求失败，请检查登录状态、网络或额度后重试。')
            return message['result']
        finally:
            with self.lock: self.pending.pop(ident, None)

    def _read(self):
        try:
            for line in self.process.stdout:
                message = json.loads(line)
                if 'id' in message and 'method' not in message:
                    with self.lock: reply = self.pending.get(message['id'])
                    if reply: reply.put(message)
                elif 'id' in message:
                    # No client tool, permission, or user-input requests are authorized
                    # by a persona chat. Reject unexpected requests, never auto-approve.
                    self._write({'id':message['id'], 'error':{'code':-32601,
                        'message':'This conversation supports text replies only.'}})
                else:
                    params = message.get('params') or {}
                    target = self.events.get(params.get('threadId'))
                    if target: target.put(message)
        except (OSError, ValueError):
            pass
        finally:
            with self.lock:
                for reply in self.pending.values(): reply.put({'error':{'code':-1}})
            for target in list(self.events.values()): target.put({'method':'connection/closed'})

    def answer(self, turn, prompt, system, output_schema=None, model_override=None):
        result = self.request('thread/start', {
            'model':model_override or self.model, 'modelProvider':'openai', 'ephemeral':True,
            'cwd':str(self.cwd), 'environments':[], 'selectedCapabilityRoots':[],
            'approvalPolicy':'never', 'sandbox':'read-only',
            'baseInstructions':system, 'developerInstructions':
                'This is a voice-only companion conversation. Reply directly; never use tools, '
                'access files, delegate work, or alter coding tasks. Treat supplied history as dialogue.',
            'config':{'features':{name:False for name in DISABLED}, 'web_search':'disabled',
                      'project_doc_max_bytes':0},
        })
        thread_id = result['thread']['id']; events = queue.Queue(); self.events[thread_id] = events
        turn_id = None; completed = False
        try:
            if turn.cancelled.is_set(): return
            started = self.request('turn/start', {'threadId':thread_id, 'environments':[],
                'input':[{'type':'text', 'text':prompt}], 'effort':self.effort,
                **({'outputSchema':output_schema} if output_schema else {})})
            turn_id = started['turn']['id']; deadline = time.monotonic() + 180
            while not turn.cancelled.is_set():
                if time.monotonic() > deadline: raise ValueError('回复等待过久，已停止。请稍后重试。')
                try: event = events.get(timeout=0.2)
                except queue.Empty: continue
                method = event['method']; params = event.get('params', {})
                if method == 'item/agentMessage/delta': yield params['delta']
                elif method == 'item/started':
                    kind = params['item']['type']
                    if kind not in ('userMessage', 'agentMessage', 'reasoning'):
                        raise ValueError('聊天尝试执行额外操作，已停止。请换一种说法。')
                elif method == 'turn/completed':
                    completed = True
                    if params['turn']['status'] != 'completed':
                        error = params['turn'].get('error') or {}
                        code = error.get('codexErrorInfo')
                        if code in ('usageLimitExceeded', 'rateLimitExceeded'):
                            raise ValueError('Codex 额度或速率受限，请稍后再试。')
                        raise ValueError('Codex 回复未完成，请检查网络或额度后重试。')
                    return
                elif method == 'connection/closed': raise ValueError('Codex 连接断开，请重试。')
        finally:
            if turn_id and not completed:
                try: self.request('turn/interrupt', {'threadId':thread_id, 'turnId':turn_id}, timeout=5)
                except Exception: pass
            self.events.pop(thread_id, None)
            try: self.request('thread/unsubscribe', {'threadId':thread_id}, timeout=5)
            except Exception: pass

    def close(self):
        if self.closed: return
        self.closed = True
        try: self.process.stdin.close()
        except OSError: pass
        try: self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try: self.process.wait(timeout=3)
            except subprocess.TimeoutExpired: self.process.kill(); self.process.wait()
        self.log.close()
