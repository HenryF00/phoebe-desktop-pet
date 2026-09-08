#!/usr/bin/env python3
"""Run the pinned upstream GPT-SoVITS API locally with CPU float32 defaults."""
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    upstream = ROOT / 'vendor/GPT-SoVITS'
    python = ROOT / 'runtime/tts-venv/bin/python'
    weights = ROOT / 'models/gpt-sovits'
    dictionary = ROOT / 'models/open_jtalk_dic_utf_8-1.11'
    required = ['s1v3.ckpt', 'v2Pro/s2Gv2ProPlus.pth',
                'sv/pretrained_eres2netv2w24s4ep4.ckpt',
                'chinese-hubert-base/pytorch_model.bin',
                'chinese-roberta-wwm-ext-large/pytorch_model.bin',
                'chinese-roberta-wwm-ext-large/tokenizer.json', 'fast_langdetect/lid.176.bin']
    missing = [str(weights / name) for name in required if not (weights / name).is_file()]
    missing += [str(p) for p in (python, upstream / 'api_v2.py', dictionary / 'sys.dic') if not p.is_file()]
    if missing:
        raise SystemExit('尚未安装完整：\n' + '\n'.join(missing))
    pretrained = upstream / 'GPT_SoVITS/pretrained_models'
    pretrained.mkdir(exist_ok=True)
    for name in ('s1v3.ckpt', 'v2Pro', 'sv', 'chinese-hubert-base', 'chinese-roberta-wwm-ext-large', 'fast_langdetect'):
        link, target = pretrained / name, weights / name
        if not link.exists():
            link.symlink_to(target, target_is_directory=target.is_dir())
        elif link.resolve() != target.resolve():
            raise SystemExit(f'已有不同的模型文件，未覆盖：{link}')
    config = ROOT / 'runtime/tts-infer.yaml'
    config.write_text(json.dumps({'custom': {
        'version': 'v2ProPlus', 'device': 'cpu', 'is_half': False,
        't2s_weights_path': str(weights / 's1v3.ckpt'),
        'vits_weights_path': str(weights / 'v2Pro/s2Gv2ProPlus.pth'),
        'bert_base_path': str(weights / 'chinese-roberta-wwm-ext-large'),
        'cnhuhbert_base_path': str(weights / 'chinese-hubert-base'),
    }}, indent=2) + '\n')
    env = os.environ.copy()
    env.update(HF_HOME=str(ROOT / 'models/huggingface'), HF_HUB_DISABLE_TELEMETRY='1',
               GRADIO_ANALYTICS_ENABLED='False', PYTHONUNBUFFERED='1',
               OPEN_JTALK_DICT_DIR=str(dictionary))
    env.setdefault('OMP_NUM_THREADS', '8')
    os.chdir(upstream)
    os.execve(python, [str(python), 'api_v2.py', '-a', '127.0.0.1', '-p', '9880', '-c', str(config)], env)


if __name__ == '__main__':
    main()
