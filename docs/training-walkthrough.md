# 洛琪希日语音色：从素材到微调

目标是让模型朗读新输入的日语文字。现有约 10 分钟素材用于参考音色和小样本微调，不从零训练通用语音大模型。是否像目标角色，必须通过试听判断。

更新：本机的基础模型合成已跑通，安装状态、启动命令与两次短句测量见 [本地语音运行说明](local-tts.md)。下面的数据校对与微调步骤仍未完成。

## 1. 生成待校对字幕

项目的 `.venv` 为 Apple Silicon 本地 ASR 环境，依赖版本保存在 `config/asr-requirements.lock`。所有命令从项目根目录执行：

```sh
cd /Users/ren/Documents/GitHub/Roxy
.venv/bin/python scripts/transcribe_voice.py
.venv/bin/python scripts/import_asr_drafts.py
```

默认使用 `mlx-community/whisper-large-v3-turbo`。若 Hugging Face 直连不可用，可在第一条 Python 命令前添加 `HF_ENDPOINT=https://hf-mirror.com HF_HUB_DISABLE_XET=1`，只下载公开权重；音频仍在本机识别。也可用 `--model /绝对路径/模型目录` 加载已下载模型。

首次下载约 1.6GB 的识别模型；它只负责语音转文字，不负责生成声音。结果按片段保存在 `data/asr-drafts/*.json`，再次执行可跳过相同音频和模型的已完成结果。导入脚本只填充 `data/transcripts.tsv` 的空白且未校对行，不覆盖已有文本，不自动标记 reviewed。

逐段播放 WAV，修正字幕：只写实际说出的日语，不翻译、不补全被截断的台词。检查是否混有其他角色、音乐、明显混响或切断音节。不能确认的行保留 `reviewed=false`；确认文本和声音可用后才改为 `true`。需要重新切分的声音另存派生数据，并先更新数据表和导出脚本的数据源，不能用与音频不对应的字幕凑数。

## 2. 先建立未微调的合成基线

基线候选为 GPT-SoVITS v2ProPlus。上游列有 Apple Silicon 路径；本机的版本兼容性、内存和速度必须通过运行确认。独立安装在 `vendor/GPT-SoVITS`，使用独立 Python 环境，避免与 MLX ASR 的 NumPy/PyTorch 版本混装。

重建环境时按 [GPT-SoVITS 官方安装说明](https://github.com/RVC-Boss/GPT-SoVITS#installation) 安装依赖和 FFmpeg。v2ProPlus 需要对应 SoVITS、GPT、HuBERT、BERT 和声纹权重；训练还需匹配的判别器权重。本机已下载这些权重，启动脚本会生成 CPU、`is_half: false` 的 `custom` 配置并指向本地文件。

挑一段已经校对的干净日语音频：

```sh
.venv/bin/python scripts/prepare_reference.py roxy_seg_0016_0147910-0157476.wav
```

上面的文件名只是根据首尾音量筛出的候选，尚不能仅凭该指标认定其适合；若该行未校对，命令会拒绝。随后在 GPT-SoVITS 自己的环境与目录启动官方 `api_v2.py`，绑定 `127.0.0.1:9880`。再回 Roxy 项目执行：

```sh
.venv/bin/python scripts/tts_probe.py --text 'こんにちは。今日も一緒に頑張りましょう。'
```

输出 `generated/probe.wav` 和耗时报告。这一步使用参考音频合成新台词，并没有微调模型。探针等待完整 WAV，不代表桌宠已接入实时语音。

## 3. 导出并训练角色音色

字幕核对后：

```sh
.venv/bin/python scripts/export_training.py
```

输出 `generated/training/roxy.list`，遵循上游格式 `音频绝对路径|roxy|ja|日语原文`。没有已校对样本时拒绝生成，不会把 ASR 猜测直接当训练真值。

打开 GPT-SoVITS WebUI，选择与基础权重匹配的版本，设置实验名如 `roxy-ja-v1`，将 `.list` 路径用于数据准备。顺序是：

1. 数据准备：提取文本/音素、HuBERT 音频特征和语义 token；确认每个已选样本成功处理。
2. SoVITS 微调：学习声音生成与角色音色。先用小 batch 做一个短训练，检查能否完成、内存和耗时；不要直接跑长轮数。
3. GPT 微调：学习文本到语音语义的映射，影响发音、停顿和表达。仍使用同一份正确对应的数据。
4. 保留多个 checkpoint，用固定的未见测试句比较；轮数越多不等于越好，重复、漏字和过拟合也是失败信号。

上游截至本次检查仍提示 Mac GPU 训练可能降低质量，暂用 CPU。因此本机先验证 CPU 训练，关闭半精度；具体训练时长暂未实测。不能把 Amadeus 的 CUDA 参数照搬过来，也不要把 v3/v4 的 LoRA 流程与 v2ProPlus 混用。若以后选择 NVIDIA 机器训练，得到匹配的权重后再回本机测试推理；本项目没有申请云资源或上传素材。

## 4. 对比并接回桌宠

在合成服务载入训练出的 GPT `.ckpt` 与 SoVITS `.pth`，继续传入干净参考音频和准确日文。用 `data/voice-evaluation-ja.txt` 对比基础模型与各轮 checkpoint，记录参考片段、模型、随机种子、音色、读音、噪音、漏字/重复和生成耗时。不要仅试听训练集原台词。

声音合格后，再将“日语回复 → 本机 TTS → 分块播放 → 嘴型与状态”接入桌宠。实时对话还需要麦克风识别、回复生成、首音延迟控制和打断播放；训练出音色模型只是其中一部分。

## 依据

- [MLX Whisper 官方使用说明](https://github.com/ml-explore/mlx-examples/tree/main/whisper)
- [所用 ASR 模型](https://huggingface.co/mlx-community/whisper-large-v3-turbo)
- [GPT-SoVITS 官方安装、数据格式与微调说明](https://github.com/RVC-Boss/GPT-SoVITS)
- [官方训练 WebUI 实现](https://github.com/RVC-Boss/GPT-SoVITS/blob/main/webui.py)
