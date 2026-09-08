import json
import sys
from pathlib import Path
import threading
import unittest
import httpx
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from chat_settings import preferences,instructions
from deepseek_chat import DeepSeekChat
from bilingual_reply import BilingualReply
from chat_worker import Turn

class SettingsTests(unittest.TestCase):
    def test_languages_independent_and_key_not_in_public_preferences(self):
        p=preferences({'subtitle_language':'en','voice_language':'zh','api_key':'never-persist'})
        self.assertNotIn('api_key',p)
        self.assertIn('caption必须用英文',instructions(p))
        self.assertIn('speech必须用简体中文',instructions(p))
        with self.assertRaises(ValueError):preferences({'voice_language':'wrong'})
    def test_deepseek_sse_and_schema(self):
        reply=json.dumps({'segments':[{'caption':'Welcome.','speech':'欢迎。'}]},ensure_ascii=False)
        def handler(request):
            payload=json.loads(request.content)
            self.assertEqual(str(request.url),'https://api.deepseek.com/chat/completions')
            self.assertEqual(payload['response_format'],{'type':'json_object'})
            self.assertEqual(payload['thinking'],{'type':'disabled'})
            self.assertNotIn('tools',payload)
            events=['data: '+json.dumps({'choices':[{'delta':{'content':part},'finish_reason':None}]})+'\n\n' for part in (reply[:25],reply[25:])]
            events+=['data: '+json.dumps({'choices':[{'delta':{},'finish_reason':'stop'}]})+'\n\n','data: [DONE]\n\n']
            return httpx.Response(200,text=''.join(events))
        model=DeepSeekChat('test-only-not-a-real-key','deepseek-v4-flash',httpx.MockTransport(handler))
        parser=BilingualReply();caption=''
        for delta in model.answer(Turn('test'),'hello',instructions(preferences())):
            parser.feed(delta);caption+=parser.drain_text()
        parser.feed('',final=True)
        self.assertEqual(caption,'Welcome.')
    def test_deepseek_missing_or_rejected_key_is_actionable_and_not_leaked(self):
        with self.assertRaisesRegex(ValueError,'设置'):DeepSeekChat('','deepseek-v4-flash')
        model=DeepSeekChat('test-secret','deepseek-v4-flash',httpx.MockTransport(lambda r:httpx.Response(401,text='test-secret')))
        with self.assertRaisesRegex(ValueError,'密钥无效') as raised:list(model.answer(Turn('x'),'hi','json'))
        self.assertNotIn('test-secret',str(raised.exception))
    def test_deepseek_truncated_answer_is_not_accepted(self):
        model=DeepSeekChat('test-only','deepseek-v4-flash',httpx.MockTransport(lambda r:httpx.Response(200,text='data: [DONE]\n\n')))
        with self.assertRaisesRegex(ValueError,'中断或为空'):list(model.answer(Turn('x'),'hi','json'))

if __name__=='__main__':unittest.main()
