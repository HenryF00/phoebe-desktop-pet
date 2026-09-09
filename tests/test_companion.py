import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from bilingual_reply import BilingualReply
from companion_memory import CompanionMemory

class CompanionTests(unittest.TestCase):
    def test_every_chunk_boundary_keeps_expression_out_of_caption(self):
        value = {'segments': [
            {'caption': '谢谢你。', 'speech': 'ありがとうございます。', 'expression': 'shy'},
            {'caption': '我们继续吧。', 'speech': '続けましょう。', 'expression': 'nod'}]}
        raw = json.dumps(value, ensure_ascii=True)
        for width in (1, 2, 7, 23, len(raw)):
            parser = BilingualReply(); display = ''; pairs = []
            for start in range(0, len(raw), width):
                pairs += parser.feed(raw[start:start+width]); display += parser.drain_text()
            parser.feed('', final=True)
            self.assertEqual(display, '谢谢你。我们继续吧。')
            self.assertEqual(pairs, value['segments'])
        with self.assertRaises(ValueError):
            BilingualReply().feed('{"segments":[{"caption":"x","speech":"y","expression":"run_shell"}]}', final=True)

    def test_explicit_memory_persistence_edit_disable_and_forget(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'notes.json'; store = CompanionMemory(path)
            self.assertEqual(store.context('我喜欢茶'), ('', ''))
            self.assertEqual(store.context('请记住：叫我小任')[0], '叫我小任')
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            restarted = CompanionMemory(path)
            self.assertEqual(restarted.context('你好')[0], '叫我小任')
            restarted.context('请记住：叫我小任')
            self.assertEqual(restarted.read()['notes'], '叫我小任')
            self.assertIn('没有找到', restarted.context('忘记：小任')[1])
            restarted.write({'enabled': False, 'notes': '叫我小任'})
            self.assertEqual(restarted.context('请记住：其他称呼')[0], '')
            self.assertEqual(restarted.read()['notes'], '叫我小任')
            restarted.write({'enabled': True, 'notes': '叫我小任'})
            self.assertEqual(restarted.context('忘记：叫我小任')[0], '')
            path.write_text('broken')
            with self.assertRaisesRegex(ValueError, '无法读取'): restarted.context('你好')
            self.assertEqual(path.read_text(), 'broken')

if __name__ == '__main__': unittest.main()
