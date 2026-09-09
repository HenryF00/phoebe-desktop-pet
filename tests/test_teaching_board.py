import json
import sys
from pathlib import Path
import tempfile
import threading
import time
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from teaching_board import TeachingBoards, clean_board, signature
from chat_settings import preferences
from chat_worker import Turn

RAW='示例数据：2025年营收 100 亿元，2026年营收 120 亿元。来源：https://example.com/report'
JOB={'id':'qa-board','title':'示例公司的财报','status':'completed','result':RAW}
VALUE={'title':'收入增长，仍需观察利润','takeaway':'收入从100增长到120亿元。','points':[{'heading':'先看增长','explanation':'同口径收入提高。'}],
    'metrics':[{'label':'收入','value':'120','context':'2026年，亿元','evidence':RAW}],
    'chart':{'title':'收入对比','unit':'亿元','points':[{'label':'2025','value':'100','evidence':RAW},{'label':'2026','value':'120','evidence':RAW}]},
    'narration':[{'caption':'先看收入变化。','speech':'売上の変化を見ましょう。'}]}
class BoardsTests(unittest.TestCase):
    def test_grounding_rejects_invented_values_units_and_bad_numbers(self):
        value=json.loads(json.dumps(VALUE));board=clean_board(value,JOB,preferences())
        self.assertEqual(len(board['chart']['points']),2)
        value['chart']['points'][1]['value']='999'
        value['metrics'][0]['value']='12' # Must not match a substring of 120.
        board=clean_board(value,JOB,preferences())
        self.assertFalse(board['chart']['points']);self.assertFalse(board['metrics'])
        value=json.loads(json.dumps(VALUE));value['chart']['unit']='美元'
        self.assertFalse(clean_board(value,JOB,preferences())['chart']['points'])
        for invalid in ('NaN','Infinity','1e1000'):
            value['chart']['points'][0]['value']=invalid
            self.assertFalse(clean_board(value,JOB,preferences())['chart']['points'])
    def test_sources_are_extracted_from_original_and_failure_retained(self):
        value=dict(VALUE,sources=['https://invented.test/'])
        board=clean_board(value,dict(JOB,status='failed'),preferences())
        self.assertEqual(board['sources'],['https://example.com/report'])
        self.assertEqual(board['status'],'failed');self.assertEqual(board['original'],RAW)
    def test_cache_language_invalidation_and_private_storage(self):
        calls=[];events=[]
        class Client:
            def answer(self,*args,**kwargs): calls.append(1);yield json.dumps(VALUE)
            def close(self): pass
        with tempfile.TemporaryDirectory() as root:
            service=TeachingBoards(root,events.append,Turn,Client)
            def run(settings):
                events.clear();service.prepare(JOB,settings)
                deadline=time.time()+3
                while not any(e['type']=='board_ready' for e in events) and time.time()<deadline: time.sleep(.01)
                self.assertTrue(any(e['type']=='board_ready' for e in events))
            run(preferences());run(preferences());self.assertEqual(len(calls),1)
            run(preferences({'voice_language':'zh'}));self.assertEqual(len(calls),2)
            self.assertEqual(service.path(JOB['id']).stat().st_mode & 0o777,0o600)
            with self.assertRaises(ValueError):service.path('../escape')
            service.close()
    def test_failure_keeps_full_result_without_reading_it_aloud(self):
        class Client:
            def answer(self,*args,**kwargs): raise RuntimeError('offline')
            def close(self): pass
        with tempfile.TemporaryDirectory() as root:
            ready=threading.Event();events=[]
            def emit(event):
                events.append(event)
                if event['type']=='board_ready':ready.set()
            service=TeachingBoards(root,emit,Turn,Client);service.prepare(JOB,preferences())
            self.assertTrue(ready.wait(3));board=events[-1]['board']
            self.assertTrue(board['fallback']);self.assertFalse(board['narration']);self.assertEqual(board['original'],RAW)
            self.assertEqual(board['prepare_error'],'RuntimeError')
            class Recovered:
                def answer(self,*args,**kwargs):yield json.dumps(VALUE)
                def close(self):pass
            service.client_factory=Recovered;ready.clear();service.prepare(JOB,preferences())
            self.assertTrue(ready.wait(3));self.assertFalse(events[-1]['board']['fallback'],'Failed notes must not become a permanent cached result')
            service.close()
if __name__=='__main__':unittest.main()
