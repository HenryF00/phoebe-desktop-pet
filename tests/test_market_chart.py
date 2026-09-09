import sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from market_chart import symbol_for,normalize
class MarketChartTests(unittest.TestCase):
    def test_symbol_requires_stock_context_and_explicit_reference(self):
        self.assertEqual(symbol_for({'title':'请分析英伟达的财报'}),'NVDA')
        self.assertEqual(symbol_for({'title':'AMD 股价走势'}),'AMD')
        self.assertIsNone(symbol_for({'title':'解释 AI 的历史'}))
        self.assertIsNone(symbol_for({'title':'股价 <script>alert(1)</script>'}))
    def test_price_integrity_and_sorted_unique_dates(self):
        rows=[{'date':d,'open':'$10','high':'$12','low':'$9','close':c,'volume':'1,234'} for d,c in [('09/08/2026','$11'),('09/07/2026','$10'),('09/06/2026','$99')]]
        value={'data':{'symbol':'NVDA','tradesTable':{'rows':rows}}}
        chart=normalize(value,'NVDA')
        self.assertEqual([r['date'] for r in chart['rows']],['2026-09-07','2026-09-08'])
        self.assertEqual(chart['rows'][-1]['volume'],1234)
        self.assertEqual(chart['as_of'],'2026-09-08')
        with self.assertRaises(ValueError):normalize(value,'AAPL')
    def test_no_fake_graph_on_missing_or_nonfinite_prices(self):
        with self.assertRaises(ValueError):normalize({'data':None},'NVDA')
        with self.assertRaises(ValueError):normalize({'data':{'symbol':'NVDA','tradesTable':{'rows':[{'date':'09/08/2026','open':'nan','high':'12','low':'9','close':'10','volume':'100'}]}}},'NVDA')
