import json,sys,unittest
from pathlib import Path
from unittest.mock import Mock,patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from teaching_board import clean_board,lesson_context,followup_job,CLOSINGS
from chat_settings import preferences
from interaction_routing import route_request
from chat_worker import Worker,Turn

class LessonConversationTests(unittest.TestCase):
    def test_followup_retains_original_source_and_bounded_rounds(self):
        board={'task_id':'original','original':'原始财报 100 USD','title':'财报','conversation':[]}
        for i in range(9):
            job=followup_job(str(i),'继续解释'+str(i),'解释'+str(i),board)
            board=clean_board({'title':'追问','takeaway':'解释','points':[]},job,preferences())
        context=lesson_context(board)
        self.assertEqual(context['source'],'原始财报 100 USD')
        self.assertEqual(context['current_explanation'],'解释8')
        self.assertEqual(len(context['conversation']),12)
        self.assertEqual(context['conversation'][-2]['content'],'继续解释8')

    def test_closing_is_last_separate_speech_segment_in_selected_languages(self):
        value={'narration':[{'caption':'要点','speech':'説明','focus':'takeaway'}]}
        b=clean_board(value,{'id':'x','result':''},preferences({'subtitle_language':'zh','voice_language':'ja'}))
        self.assertEqual(b['narration'][-1]['focus'],'closing')
        self.assertEqual(b['narration'][-1]['speech'],CLOSINGS['ja'])
        self.assertEqual(b['closing_caption'],CLOSINGS['zh'])
        self.assertEqual(b['narration'][0]['caption'],'要点')
        self.assertNotIn('closing_caption',clean_board({}, {'id':'x','result':''},preferences()))

    def test_financial_followup_explains_existing_board_but_fresh_lookup_delegates(self):
        self.assertEqual(route_request('那这份财报为什么增长这么快？',mode='lesson')['execution'],'lesson')
        fresh=route_request('帮我查今天的股价',mode='lesson')
        self.assertEqual((fresh['execution'],fresh['presentation']),('task','board'))

    def test_worker_uses_board_context_for_text_and_transcribed_followups(self):
        previous={'task_id':'old','title':'财报','original':'仅此来源 100 USD','conversation':[{'role':'user','content':'上个问题'}]}
        for audio in (False,True):
            worker=Worker();worker.boards.get=Mock(return_value=previous);worker.boards.prepare=Mock()
            model=Mock();model.answer.return_value=iter([json.dumps({'answer':'这是因为需求增长。'})])
            worker.connect=Mock(return_value=model)
            command={'id':'new','mode':'lesson','board_id':'old','history':[{'role':'user','content':'普通聊天不能混入'}],
                'settings':preferences(),'text':'那为什么？'}
            if audio:command.update(audio='/tmp/not-a-recording.wav',text='')
            with patch('chat_worker.emit'),patch('chat_worker.transcribe',return_value='语音追问') as asr,patch('chat_worker.CompanionMemory') as memory:
                memory.return_value.context.return_value=([],None)
                worker.run(Turn('new'),command)
                payload=json.loads(model.answer.call_args.args[1])
                self.assertEqual(payload['previous_board']['source'],'仅此来源 100 USD')
                self.assertEqual(payload['history'],[])
                self.assertEqual(payload['question'],'语音追问' if audio else '那为什么？')
                job=worker.boards.prepare.call_args.args[0]
                self.assertEqual(job['parent_id'],'old')
                self.assertEqual(job['id'],'chat-new')
                self.assertEqual(job['root_source'],'仅此来源 100 USD')
            worker.boards.close()
