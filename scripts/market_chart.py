"""Read-only daily stock charts. Prices come from Nasdaq, never from generated text."""
import datetime as dt
import math
import re
from urllib.request import getproxies
import httpx

ALIASES={'英伟达':'NVDA','輝達':'NVDA','nvidia':'NVDA','苹果':'AAPL','apple':'AAPL','特斯拉':'TSLA','tesla':'TSLA','微软':'MSFT','microsoft':'MSFT','亚马逊':'AMZN','amazon':'AMZN','谷歌':'GOOGL','google':'GOOGL'}

def symbol_for(job):
    title=job.get('title','')+' '+job.get('prompt','')
    raw=job.get('result','')
    if not re.search(r'股票|股价|行情|财报|走势|stock|share price|earnings|nasdaq|nyse',title+' '+raw,re.I):return None
    for name,symbol in ALIASES.items():
        if name in title.lower() or re.search(r'\b'+symbol+r'\b',title):return symbol
    match=re.search(r'\b(?:NASDAQ|NYSE)\s*[:：]\s*([A-Z]{1,6}(?:-[A-Z])?)\b',title+' '+raw)
    if match:return match[1]
    # Accept an explicit ticker in a stock request, never an arbitrary word from the answer.
    excluded={'AI','US','USA','ETF','CEO','IPO','EPS','PE','USD','ROXY'}
    candidates=[s for s in re.findall(r'\b[A-Z]{1,5}\b',title) if s not in excluded]
    return candidates[0] if len(candidates)==1 else None

def normalize(payload,symbol):
    data=payload.get('data') or {}
    if data.get('symbol','').upper()!=symbol:raise ValueError('行情代码不匹配')
    rows=[]
    def number(value):
        n=float(str(value).replace('$','').replace(',',''))
        if not math.isfinite(n) or n<0:raise ValueError('Invalid market value')
        return n
    for value in (data.get('tradesTable') or {}).get('rows') or []:
        try:
            day=dt.datetime.strptime(value['date'],'%m/%d/%Y').date()
            o,h,l,c=[number(value[k]) for k in ('open','high','low','close')]
            v=number(value['volume'])
            if day>dt.date.today() or l<=0 or l>min(o,c) or h<max(o,c):continue
            rows.append({'date':day.isoformat(),'open':o,'high':h,'low':l,'close':c,'volume':v})
        except (ValueError,TypeError,KeyError):continue
    rows=sorted({r['date']:r for r in rows}.values(),key=lambda r:r['date'])[-120:]
    if len(rows)<2:raise ValueError('没有足够的历史行情')
    return {'symbol':symbol,'currency':'USD','source':'Nasdaq','source_url':f'https://www.nasdaq.com/market-activity/stocks/{symbol.lower()}/historical',
        'as_of':rows[-1]['date'],'fetched_at':dt.datetime.now(dt.timezone.utc).isoformat(),'price_basis':'日线 OHLC · 来源原始价格，未自行复权','rows':rows}

def fetch_market(symbol):
    if not re.fullmatch(r'[A-Z]{1,6}(?:-[A-Z])?',symbol):raise ValueError('股票代码无效')
    today=dt.date.today()
    # Respect the user's existing macOS proxy; do not change network settings.
    proxy=getproxies().get('https')
    if proxy and '://' not in proxy:proxy='http://'+proxy
    with httpx.Client(proxy=proxy,timeout=httpx.Timeout(18,connect=8),headers={'User-Agent':'Mozilla/5.0','Accept':'application/json','Origin':'https://www.nasdaq.com'}) as client:
        response=client.get(f'https://api.nasdaq.com/api/quote/{symbol}/historical',params={'assetclass':'stocks','limit':150,'fromdate':str(today-dt.timedelta(days=190)),'todate':str(today)})
        response.raise_for_status()
        return normalize(response.json(),symbol)

def market_context(chart):
    rows=chart['rows'][-60:];first,last=rows[0],rows[-1]
    change=(last['close']/first['close']-1)*100
    return (f"\n独立行情快照（与报告期间区分）：{chart['symbol']}，Nasdaq，USD，{chart['price_basis']}。"
        f"{first['date']} 收盘 {first['close']} USD，{last['date']} 收盘 {last['close']} USD；"
        f"默认图展示最近60个交易日；此区间收盘变化 {change:+.2f}%。这不是实时价格。来源：{chart['source_url']}\n")
