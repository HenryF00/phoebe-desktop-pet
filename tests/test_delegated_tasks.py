import json
from pathlib import Path
import queue
import sys
import tempfile
import threading
import time
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from delegated_tasks import TaskManager,route_task,launch_args

class FakeProcess:
    def poll(self): return None

class FakeClient:
    instances=[]
    def __init__(self,**kwargs):
        self.kwargs=kwargs;self.process=FakeProcess();self.writes=[];self.calls=[];self.instances.append(self)
    def request(self,method,params,**kwargs):
        self.calls.append((method,params))
        if method in ('thread/start','thread/resume'): return {'thread':{'id':'thread-one'}}
        if method=='turn/start': return {'turn':{'id':'turn-one'}}
        if method=='project/list': return {'data':[],'nextCursor':None}
        return {}
    def _write(self,value): self.writes.append(value)
    def close(self): pass

class TaskTests(unittest.TestCase):
    def wait_running(self,manager,ident):
        until=time.monotonic()+2
        while time.monotonic()<until:
            if manager.jobs[ident]['status']=='running':return
            time.sleep(.005)
        self.fail(manager.jobs[ident]['status'])
    def test_task_route_and_separate_capabilities(self):
        for text in ('委托：整理资料','帮我看下英伟达股票','帮我查一下天气'):
            self.assertTrue(route_task(text))
        self.assertFalse(route_task('你觉得什么是股票？'))
        self.assertTrue(route_task('检查代码',{'id':'p'}))
        self.assertTrue(route_task('看这张图',attachments=['image.png']))
        self.assertIn('web_search="live"',launch_args())
        self.assertNotIn('mcp_servers={}',launch_args())
        self.assertNotIn('--dangerously-bypass-approvals-and-sandbox',launch_args())
    def test_lifecycle_approval_ownership_final_only_and_restart(self):
        with tempfile.TemporaryDirectory() as directory:
            events=[];done=[];m=TaskManager(directory,events.append,done.append,FakeClient)
            try:
                ident=m.submit('test');self.wait_running(m,ident);client=m.client
                start=next(p for method,p in client.calls if method=='thread/start')
                self.assertNotIn('baseInstructions',start)
                self.assertNotIn('environments',start)
                self.assertEqual(start['sandbox'],'workspace-write')
                self.assertEqual(start['approvalPolicy'],'on-request')
                params={'threadId':'thread-one','turnId':'turn-one','itemId':'item','command':'cat file'}
                m.on_request({'id':101,'method':'item/commandExecution/requestApproval','params':params})
                first=m.jobs[ident]['request']['token']
                m.on_request({'id':102,'method':'item/permissions/requestApproval','params':{**params,'permissions':{'network':{'enabled':True}}}})
                second=m.jobs[ident]['request']['token']
                self.assertEqual(client.writes,[])
                m.respond(first,False)
                self.assertEqual(client.writes[-1],{'id':101,'result':{'decision':'decline'}})
                self.assertEqual(m.jobs[ident]['status'],'waiting')
                m.respond(second,True)
                self.assertEqual(client.writes[-1]['result'],{'permissions':{'network':{'enabled':True}},'scope':'turn'})
                with self.assertRaises(ValueError):m.respond(second,True)
                m.on_request({'id':999,'method':'item/commandExecution/requestApproval','params':{'threadId':'foreign'}})
                self.assertIn('error',client.writes[-1])
                for phase,text in [('commentary','working'),('final_answer','verified result')]:
                    m.on_event({'method':'item/completed','params':{'threadId':'thread-one','turnId':'turn-one','item':{'type':'agentMessage','phase':phase,'text':text}}})
                m.on_event({'method':'turn/completed','params':{'threadId':'thread-one','turn':{'status':'completed'}}})
                until=time.monotonic()+2
                while not done and time.monotonic()<until:time.sleep(.005)
                self.assertEqual(done[0]['result'],'verified result')
                path=Path(directory)/'runtime/tasks'/f'{ident}.json'
                self.assertEqual(path.stat().st_mode&0o777,0o600)
            finally:m.close()
            restarted=TaskManager(directory,lambda e:None,client_factory=FakeClient)
            try:self.assertEqual(restarted.jobs[ident]['result'],'verified result')
            finally:restarted.close()
    def test_cancellation_and_attachment_boundary(self):
        with tempfile.TemporaryDirectory() as directory:
            m=TaskManager(directory,lambda e:None,client_factory=FakeClient)
            try:
                with self.assertRaises(ValueError):m.submit('look',attachments=['/tmp/not-owned.png'])
                ident=m.submit('cancel this');self.wait_running(m,ident);m.cancel(ident)
                self.assertEqual(m.jobs[ident]['status'],'cancelled')
                m.on_event({'method':'turn/completed','params':{'threadId':'thread-one','turn':{'status':'completed'}}})
                self.assertEqual(m.jobs[ident]['status'],'cancelled')
            finally:m.close()
if __name__=='__main__':unittest.main()
