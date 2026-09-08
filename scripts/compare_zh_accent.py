#!/usr/bin/env python3
"""Reproducible Mandarin accent audition; ASR checks words, not accent quality."""
import argparse, hashlib, json, time, wave
from pathlib import Path
import httpx
from chat_worker import ROOT, reference

TEXT='你好，我是洛琪希。今天的任务已经完成了，我们先休息一下吧。'

def main():
    parser=argparse.ArgumentParser(); parser.add_argument('--text',default=TEXT); args=parser.parse_args()
    output=ROOT/'generated/zh-accent';output.mkdir(parents=True,exist_ok=True)
    ref=reference()
    # Use the annotated middle utterance, retaining original audio and creating a separate audition crop.
    crop=output/'reference-crop.wav'
    with wave.open(ref['ref_audio_path'],'rb') as src:
        rate=src.getframerate();params=src.getparams();src.setpos(int(2.22*rate));frames=src.readframes(int((6.34-2.22)*rate))
    with wave.open(str(crop),'wb') as dst:dst.setparams(params);dst.writeframes(frames)
    variants=[('A-current',{}),('B-no-japanese-prompt',{'prompt_text':''}),
              ('C-low-temperature',{'temperature':0.6}),
              ('D-short-reference',{'ref_audio_path':str(crop),'prompt_text':'パウロさん達も私の姿を見て驚いてたでしょ'})]
    results=[]
    with httpx.Client(trust_env=False,timeout=180) as client:
        for name,overrides in variants:
            payload=dict(text=args.text,text_lang='zh',**ref,media_type='wav',streaming_mode=False,
                         parallel_infer=False,batch_size=1,text_split_method='cut5',seed=42)
            payload.update(overrides);start=time.monotonic()
            response=client.post('http://127.0.0.1:9880/tts',json=payload);response.raise_for_status()
            path=output/(name+'.wav');path.write_bytes(response.content)
            with wave.open(str(path),'rb') as wav:
                seconds=wav.getnframes()/wav.getframerate();assert seconds>0.5
            result={'variant':name,'text':args.text,'overrides':overrides,'seed':42,'audio_seconds':seconds,
                    'generation_seconds':round(time.monotonic()-start,3),'path':str(path.relative_to(ROOT)),
                    'sha256':hashlib.sha256(response.content).hexdigest(),'accent_verdict':'needs listener comparison'}
            results.append(result);print(json.dumps(result,ensure_ascii=False),flush=True)
    (ROOT/'qa/zh-accent-comparison.json').write_text(json.dumps(results,ensure_ascii=False,indent=2)+'\n')
if __name__=='__main__':main()
