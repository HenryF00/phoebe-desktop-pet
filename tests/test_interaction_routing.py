import sys,unittest,threading
from unittest.mock import patch
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from interaction_routing import route_request,wants_board
from chat_settings import preferences
from teaching_board import clean_board
class RoutingTests(unittest.TestCase):
    def test_execution_and_presentation_independent(self):
        cases=[('你好','chat'),('kimo是什么意思','chat'),('你为什么害羞','chat'),('详细解释一下复利的原理','lesson'),
            ('@小黑板 解释复利','lesson'),('@委托 运行测试','task'),('/委托 运行测试','task'),
            ('@委托 @小黑板 分析这些数据','task'),('@聊天 简单解释股票','chat'),('小黑板：帮我分析最新财报','task')]
        for text,expected in cases:self.assertEqual(route_request(text)['execution'],expected,text)
        self.assertFalse(wants_board('委托：运行测试','测试通过。'))
        self.assertTrue(wants_board('分析财报','完成。'))
        self.assertFalse(wants_board('不用小黑板，分析财报','详细分析'))
        self.assertEqual(route_request('@聊天 你好',project={'id':'x'},mode='task')['project'],None)
        with self.assertRaises(ValueError):route_request('@聊天 你好',attachments=['image.png'])
    def test_command_boundaries_and_empty_body(self):
        self.assertEqual(route_request('邮箱 user@example.com')['text'],'邮箱 user@example.com')
        self.assertEqual(route_request('我想知道 @委托 是什么')['execution'],'chat')
        self.assertEqual(route_request('@小黑板')['text'],'')
        self.assertEqual(route_request('/Users/ren/test')['execution'],'chat')
    def test_japanese_caption_and_focus_validation(self):
        settings=preferences({'subtitle_language':'ja','voice_language':'ja'})
        job={'id':'x','result':'','status':'completed','title':'test'}
        board=clean_board({'points':[{'heading':'題','explanation':'説明'}],
            'narration':[{'caption':'翻訳','speech':'日本語の原文','focus':'metric-3','gesture':'invented'}]},job,settings)
        self.assertEqual(board['narration'][0],{'caption':'日本語の原文','speech':'日本語の原文','focus':'takeaway','gesture':'explain'})
    def test_brief_completion_uses_short_notification_without_board(self):
        import chat_worker
        worker=chat_worker.Worker();seen=[];done=threading.Event()
        worker.ensure_tts=lambda turn:None
        worker.synthesize=lambda *args,**kwargs:b'fake-wav-for-protocol-test'
        def emit(event):seen.append(event);done.set()
        with patch.object(chat_worker,'emit',emit):
            worker.task_finished({'id':'test-brief','presentation':'brief'})
            self.assertTrue(done.wait(2))
            self.assertEqual(seen[0]['type'],'task_brief_audio')
        worker.boards.close()
if __name__=='__main__':unittest.main()
