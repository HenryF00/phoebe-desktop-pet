import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock,patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from chat_worker import Worker,Turn
from delegated_tasks import TaskManager
import codex_chat

class ReconnectTests(unittest.TestCase):
    def test_login_file_replacement_invalidates_client_without_reading_credentials(self):
        with tempfile.TemporaryDirectory() as root,patch.dict('os.environ',{'CODEX_HOME':root}):
            path=Path(root)/'auth.json';path.write_text('test fixture')
            client=codex_chat.CodexChat.__new__(codex_chat.CodexChat)
            client.closed=False;client.auth_invalidated=False
            client.process=Mock();client.process.poll.return_value=None
            client.login_revision=codex_chat.credential_revision()
            self.assertFalse(client.needs_reconnect())
            replacement=Path(root)/'replacement';replacement.write_text('other fixture')
            replacement.replace(path)
            self.assertTrue(client.needs_reconnect())

    def test_auth_error_invalidates_client_and_hides_raw_response(self):
        client=codex_chat.CodexChat.__new__(codex_chat.CodexChat)
        client.auth_invalidated=False
        with self.assertRaises(codex_chat.CodexAuthError) as error:
            client.check_auth_error({'message':'token_revoked private response'})
        self.assertTrue(client.auth_invalidated)
        self.assertNotIn('private response',str(error.exception))
        self.assertFalse(codex_chat.auth_failure({'message':'request 14012 timed out'}))

    def test_chat_replaces_live_but_stale_client(self):
        worker=Worker()
        old=Mock();old.process.poll.return_value=None;old.needs_reconnect.return_value=True
        new=Mock();new.model='current-account-model';worker.codex=old
        with patch('codex_chat.CodexChat',return_value=new),patch('chat_worker.emit'):
            self.assertIs(worker.connect(Turn('reconnect')),new)
        old.close.assert_called_once();worker.boards.close()

    def test_idle_tasks_replace_stale_client(self):
        with tempfile.TemporaryDirectory() as root:
            new=Mock();factory=Mock(return_value=new)
            manager=TaskManager(root,lambda e:None,client_factory=factory)
            old=Mock();old.process.poll.return_value=None;old.needs_reconnect.return_value=True;manager.client=old
            self.assertIs(manager.connect(),new);old.close.assert_called_once();manager.close()

    def test_account_change_does_not_interrupt_running_task(self):
        with tempfile.TemporaryDirectory() as root:
            manager=TaskManager(root,lambda e:None,client_factory=Mock())
            old=Mock();old.process.poll.return_value=None;old.needs_reconnect.return_value=True;manager.client=old
            manager.jobs['active']={'status':'running'}
            with self.assertRaisesRegex(ValueError,'账号'):
                manager.connect()
            old.close.assert_not_called();manager.jobs.clear();manager.close()
