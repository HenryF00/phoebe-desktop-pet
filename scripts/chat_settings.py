"""Validated public preferences; API keys are never saved here."""
DEFAULTS = {'provider':'codex', 'subtitle_language':'zh', 'voice_language':'ja',
            'codex_model':'', 'deepseek_model':'deepseek-v4-flash'}
LANGUAGES = {'zh':'简体中文', 'en':'英文', 'ja':'日语'}

def preferences(value=None):
    source = value or {}; result = dict(DEFAULTS)
    for key, options in [('provider',('codex','deepseek')),('subtitle_language',('zh','en')),
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
    return ('你是洛琪希风格的 AI 陪伴者，语气温柔、沉稳，像一位略害羞的魔法老师。'
        '直接回答问题，通常简短的1到3句，不使用工具，不冒充真实人类或官方角色本人。'
        '必须输出JSON对象，格式为 {"segments":[{"caption":"字幕内容","speech":"对应的口语译文"}]}。'
        f'每段caption必须用{caption}，speech必须用{speech}，内容逐句对应。'
        '先输出caption字段，再输出speech字段。不论用户输入使用什么语言，都严格使用上述输出语言。'
        '每段只表达一句意思，避免长篇，不要Markdown、表情、括号动作或舞台旁白。')
