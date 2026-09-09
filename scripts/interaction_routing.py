"""Execution and presentation are independent. Only leading commands change modes."""
import re

COMMANDS={'委托':'task','task':'task','delegate':'task','小黑板':'board','黑板':'board','board':'board',
          '聊天':'chat','chat':'chat'}

def controls(value):
    text=value.strip(); selected=[]
    while True:
        match=re.match(r'^[@/](委托|task|delegate|小黑板|黑板|board|聊天|chat)(?=\s|[:：]|$)[\s:：]*',text,re.I)
        if not match: break
        selected.append(COMMANDS[match.group(1).lower()]);text=text[match.end():].lstrip()
    return text,selected

def wants_board(prompt,result='',explicit='auto'):
    if explicit=='board': return True
    if explicit=='chat' or re.search(r'不用.{0,3}(小黑板|图表)|不要.{0,3}(小黑板|图表)|一句话|简短回答|简单说',prompt): return False
    if re.search(r'小黑板|可视化|画.{0,3}(图|曲线)|图表|财报|财务分析|数据分析|对比.{0,12}(差异|优缺点)|比较.{0,12}(差异|优缺点)',prompt,re.I):return True
    if re.search(r'(详细|系统|通俗|分步|深入).{0,5}(解释|讲解|说明)|(?:给我|帮我)?(?:讲解|解释一下|讲讲).{2,}|原理.{0,6}(解释|说明)|怎么理解',prompt):return True
    if len(prompt)>=9 and re.match(r'^(?:请解释)?为什么(?!你|我|她|他|洛)',prompt):return True
    # A data table / multi-step explanatory result can benefit from a board even if not requested.
    return bool(len(result)>500 and (re.search(r'\|[^\n]*\|\n\|[\s:|-]+\|',result) or
        (re.search(r'原因|意味着|原理|区别|步骤',result) and re.search(r'^\s*(?:[-*]|\d+[.、])\s*',result,re.M))))

def route_request(value,project=None,mode='chat',attachments=None):
    text,commands=controls(value)
    continuing=mode=='lesson'
    explicit='board' if 'board' in commands or continuing else 'auto'
    if commands and commands[-1]=='chat':
        if attachments:raise ValueError('/聊天不处理截图，请移除附件或改用 /委托。')
        return {'text':text,'execution':'chat','presentation':'chat','project':None}
    if text.startswith(('小黑板：','小黑板:','讲解：','讲解:')):
        explicit='board';text=re.sub(r'^(?:小黑板|讲解)[:：]\s*','',text)
    task=bool(project or mode=='task' or attachments or 'task' in commands or
        re.match(r'^(?:请)?(?:委托|帮我执行任务|帮我查|帮我搜索|帮我分析股票)',text) or
        (re.match(r'^(?:请)?(?:帮我|给我)?(?:看|查|分析|研究)',text) and re.search(r'股票|股价|行情|财报|美股|港股|A股',text)))
    # Current/external facts require the existing tool-capable task path, even for a lesson.
    if re.search(r'最新|今天|实时|最近|搜一下|联网|搜索',text) and wants_board(text,explicit=explicit):task=True
    if not continuing and re.search(r'股价|行情|财报|财务数据',text) and not re.search(r'什么是|是什么意思|怎么读|如何读|阅读方法|概念',text):task=True
    lesson=wants_board(text,explicit=explicit)
    return {'text':text,'execution':'task' if task else 'lesson' if lesson else 'chat',
            'presentation':explicit,'project':project}
