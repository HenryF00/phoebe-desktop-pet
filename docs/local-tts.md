# 本地日语合成已跑通

2026-09-08 在本机 M4 Pro / 48GB 统一内存上验证 GPT-SoVITS v2ProPlus CPU float32。没有创建或推送 GitHub 远程仓库，没有上传语音素材，没有开始音色微调。

## 启动与试听

从 `/Users/ren/Documents/GitHub/Roxy` 执行：

```sh
python3 scripts/start_tts.py
```

该命令在前台启动 `127.0.0.1:9880` 服务。若该端口已有本项目服务，不要重复启动。结束服务可在启动它的终端按 Ctrl+C。

另一个终端执行：

```sh
python3 scripts/tts_probe.py --draft-reference roxy_seg_0012_0109646-0119212.wav --text 'こんにちは。今日も一緒に頑張りましょう。'
afplay generated/draft-preview.wav
```

试听模式从对应的 ASR JSON 中读取参考日文，验证原 WAV 的 SHA-256 后发给本机服务。它不会将 ASR 字幕标为已校对，不修改 `config/voice.json` 的正式参考选择，也不训练模型。音色和自然度仍需回听评估。

正式使用时，先核对 `data/transcripts.tsv` 中选定片段的日文并标记 reviewed，再执行 `prepare_reference.py`；随后不带 `--draft-reference` 调用探针。生产参考检查仍保持有效。

## 本次测量

输入：`こんにちは。今日も一緒に頑張りましょう。`

| 项目 | 结果 |
| --- | --- |
| 首次合成请求 | 9.41 秒，输出 2.96 秒音频 |
| 同一服务预热后请求 | 1.50 秒，输出 3.08 秒音频 |
| 预热后 RTF | 0.487 |
| 首次请求期间采样的服务进程 RSS 最大值 | 4.54 GiB，采样间隔 0.2 秒 |
| 输出格式 | 32 kHz、单声道、PCM16 WAV |
| 第一段音频的本地 ASR 复核 | 与请求的日文一致 |

RTF 小于 1 表示这次短句生成快于播放。这里记录的是完整 WAV 返回时间，**没有测流式首音延迟**。首次请求不含服务启动及模型下载时间。内存是进程驻留内存采样，不是整机占用或保证捕捉到的瞬时峰值。预热请求与单独的 ASR 内容复核有时间重叠；仅有两次短句请求，不能代表长句或连续对话的延迟分布。

ASR 内容一致不证明角色音色相似，也不能替代试听。两次请求使用相同 seed，但输出时长略有差异，不保证逐字节重现波形。

## 环境与来源

- 上游源码放在 `vendor/GPT-SoVITS`，源码版本及归档 SHA-256 在 `config/tts-upstream.json`。从 GitHub 官方固定提交归档安装，241 个文件逐一验证 Git blob hash，未修改上游源码。
- Python 3.11 环境为 `runtime/tts-venv`，PyTorch/torchaudio 均为 2.7.0，依赖锁定在 `config/tts-requirements.lock`。`uv pip check` 已通过。
- Homebrew FFmpeg 已安装。
- 十个 GPT-SoVITS 权重/配置文件在 `models/gpt-sovits`，文件哈希和验证记录在 `config/tts-model-files.json`。
- 日语 Open JTalk 词典在 `models/open_jtalk_dic_utf_8-1.11`，FastText 语言识别模型在 `models/gpt-sovits/fast_langdetect`。官方来源及校验记录在 `config/tts-language-assets.json`。
- 上游默认的模型目录使用符号链接指向项目模型文件，启动配置生成在 `runtime/tts-infer.yaml`，CPU / float32，默认 `OMP_NUM_THREADS=8`。
- 所有模型、虚拟环境和生成音频都被 Git 忽略；原始语音素材仍保存在项目版本管理中。

以后可用 ASR 环境的 `.venv/bin/python scripts/download_tts_models.py` 校验已安装的十个上游模型文件；完整且哈希正确时无需联网。缺失时从 Hugging Face 固定版本下载。该脚本不下载另外的 Open JTalk / FastText 辅助文件。此前国内网络使用 `HF_ENDPOINT=https://hf-mirror.com HF_HUB_DISABLE_XET=1` 下载；tokenizer 的 resolve-cache 路由超时，最终从同一固定版本的 raw 路由下载，并验证 Git blob hash。

## 尚未完成

- 66 段训练字幕的逐段校对、说话人/背景音和切句检查。
- GPT 与 SoVITS 微调及训练前后的盲听比较。
- 麦克风对话、流式播放、打断和桌宠口型接入。

训练步骤见 [训练与合成操作说明](training-walkthrough.md)。
