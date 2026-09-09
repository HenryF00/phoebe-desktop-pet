"""Validated public preferences; API keys are never saved here."""
from pathlib import Path
PERSONA = (Path(__file__).resolve().parents[1] / "config/roxy-persona.txt").read_text()
DEFAULTS = {'provider':'codex', 'subtitle_language':'zh', 'voice_language':'ja',
            'task_delivery':'auto', 'codex_model':'', 'deepseek_model':'deepseek-v4-flash'}
LANGUAGES = {'zh':'简体中文', 'en':'英文', 'ja':'日语'}

def preferences(value=None):
    source = value or {}; result = dict(DEFAULTS)
    for key, options in [('provider',('codex','deepseek')),('subtitle_language',('zh','en','ja')),('task_delivery',('auto','manual')),
                         ('voice_language',('zh','en','ja'))]:
        chosen = source.get(key, result[key])
        if chosen not in options: raise ValueError('聊天设置无效，请重新选择语言和模型来源。')
        result[key] = chosen
    for key in ('codex_model','deepseek_model'):
        chosen = source.get(key, result[key])
        if not isinstance(chosen,str) or len(chosen) > 120 or '\n' in chosen:
            raise ValueError('模型名称无效。')
        result[key] = chosen.strip()
    if not result['deepseek_model']: result['deepseek_model'] = DEFAULTS['deepseek_model']
    return result

def instructions(settings):
    caption = LANGUAGES[settings['subtitle_language']]; speech = LANGUAGES[settings['voice_language']]
    return (PERSONA + '\n必须输出JSON对象，格式为 {"segments":[{"caption":"字幕内容","speech":"对应的口语译文","expression":"calm"}]}。'
        f'每段caption必须用{caption}，speech必须用{speech}，内容逐句对应。'
        '每段按caption、speech、expression的顺序输出。expression只能是calm、shy、nod、sad、thoughtful之一。'
        '每次最多三段，每段只表达一句意思。不论用户输入什么语言，都严格使用上述输出语言。')
