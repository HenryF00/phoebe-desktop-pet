import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
import chat_worker

class ChineseReferenceTests(unittest.TestCase):
    def test_audio_only_mode_applies_only_to_chinese_and_keeps_reference_audio(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder).resolve();(root/'config').mkdir();(root/'ref.wav').write_bytes(b'test fixture')
            config={'reference_reviewed':True,'reference_audio':'ref.wav','reference_text':'こんにちは。',
                    'chinese_reference_mode':'audio_only'}
            (root/'config/voice.json').write_text(json.dumps(config))
            with patch.object(chat_worker,'ROOT',root):
                chinese=chat_worker.reference('zh')
                self.assertEqual(chinese['prompt_text'],'')
                self.assertEqual(chinese['ref_audio_path'],str(root/'ref.wav'))
                for language in ['ja','en',None]:
                    self.assertEqual(chat_worker.reference(language)['prompt_text'],'こんにちは。')
                del config['chinese_reference_mode'];(root/'config/voice.json').write_text(json.dumps(config))
                self.assertEqual(chat_worker.reference('zh')['prompt_text'],'こんにちは。')
