import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from status_voice import StatusObserver
from codex_bridge import update
from chat_worker import Sentences, Turn
from codex_chat import launch_args
from bilingual_reply import BilingualReply


class VoiceTests(unittest.TestCase):
    def test_bilingual_pairs_arbitrary_token_boundaries(self):
        text=json.dumps({'segments':[{'caption':'你今天辛苦了。','speech':'今日もお疲れさまでした。'},
                                    {'caption':'一起休息吧。','speech':'一緒に休みましょう。'}]},ensure_ascii=False)
        p=BilingualReply();out=[]
        for char in text:out.extend(p.feed(char))
        out.extend(p.feed('',final=True))
        self.assertEqual([x['caption'] for x in out],['你今天辛苦了。','一起休息吧。'])
        self.assertEqual(len(p.items),2)
    def test_chinese_caption_streams_before_japanese_or_object_end(self):
        p=BilingualReply()
        self.assertEqual(p.feed('{"segments":[{"caption":"今天'),[])
        self.assertEqual(p.drain_text(),'今天')
        p.feed('辛苦了。","speech":"まだ')
        self.assertEqual(p.drain_text(),'辛苦了。')
        self.assertEqual(p.drain_text(),'')

    def test_partial_unicode_escapes_never_leak_to_caption(self):
        text=json.dumps({'segments':[{'caption':'你说"你好"\n魔法🪄','speech':'こんにちは'}]},ensure_ascii=True)
        p=BilingualReply();out=''
        for char in text:
            p.feed(char);out+=p.drain_text()
        p.feed('',final=True)
        self.assertEqual(out,'你说"你好"\n魔法🪄')

    def test_bilingual_incomplete_response_is_rejected(self):
        p=BilingualReply();p.feed('{"segments":[{"caption":"你好","speech":"こんにちは"}')
        with self.assertRaises(ValueError):p.feed('',final=True)

    def test_splitter_stream_boundaries(self):
        s = Sentences()
        self.assertEqual(s.feed('こんにちは'), [])
        self.assertEqual(s.feed('。元気ですか？はい'), ['こんにちは。', '元気ですか？'])
        self.assertEqual(s.feed('', final=True), ['はい'])
        self.assertEqual(s.feed('', final=True), [])

    def test_splitter_bounds_long_unpunctuated_reply(self):
        s=Sentences(); out=s.feed('あ'*250);out+=s.feed('',final=True)
        self.assertEqual([len(x) for x in out], [100,100,50])

    def test_status_baseline_dedup_multiple_tasks_and_wait(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            def event(session,turn,name,now,**rest):
                update(root,dict(session_id=session,turn_id=turn,hook_event_name=name,**rest),now)
            event('old','t0','Stop',99)
            observer=StatusObserver(root,now=100)
            self.assertEqual(observer.poll(100)[0],[])
            event('a','t1','UserPromptSubmit',101);event('b','t2','UserPromptSubmit',101)
            self.assertEqual(observer.poll(101)[0],[])
            event('a','t1','Stop',102)
            self.assertEqual(observer.poll(102)[0],['review'])  # Other task still running.
            self.assertEqual(observer.poll(103)[0],[])
            event('a','t1','PostToolUse',104)
            self.assertEqual(observer.poll(104)[0],[])  # Late tool cannot resurrect turn.
            event('b','t2','PermissionRequest',105,tool_use_id='approval-1')
            self.assertEqual(observer.poll(105)[0],['waiting'])
            event('b','t2','PreToolUse',106,tool_use_id='other')
            self.assertEqual(observer.poll(106)[0],[])
            event('b','t2','Stop',107)
            self.assertEqual(observer.poll(107)[0],['review'])
            event('b','t3','UserPromptSubmit',108);observer.poll(108)
            event('b','t3','Stop',109)
            self.assertEqual(observer.poll(109)[0],['review'])

    def test_status_interrupt_is_not_success(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);observer=StatusObserver(root,now=100);observer.poll(100)
            update(root,dict(session_id='x',turn_id='t',hook_event_name='Interrupt'),101)
            self.assertEqual(observer.poll(101)[0],[])
            (root/'broken.json').write_text('{')
            (root/'invalid.json').write_text(json.dumps({'updated':'oops'}))
            self.assertEqual(observer.poll(102)[0],[])

    def test_cancelled_turn_never_emits_late_audio(self):
        import chat_worker
        received=[]; old=chat_worker.emit;chat_worker.emit=received.append
        try:
            t=Turn('old');t.cancel();t.send('audio',data='late')
            self.assertEqual(received,[])
        finally:chat_worker.emit=old

    def test_no_external_tools_in_child_configuration(self):
        args=launch_args()
        self.assertIn('mcp_servers={}',args)
        self.assertIn('web_search="disabled"',args)
        self.assertIn('model_provider="openai"',args)
        for name in ('hooks','shell_tool','apps','plugins','computer_use','multi_agent'):
            self.assertTrue(any(args[i:i+2]==['--disable',name] for i in range(len(args))))
        self.assertFalse(any('bypass' in arg for arg in args))

if __name__=='__main__':unittest.main()
