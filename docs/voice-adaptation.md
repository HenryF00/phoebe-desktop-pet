# 本地日语 AI 语音适配

## 素材检查结果

来源为用户提供的 `roxy_vad_8-10s` 文件夹。66 段共 607.3 秒，均为 24 kHz、单声道、16-bit PCM。原 manifest 记录的是 VAD 切片起止时间，没有字幕。

未检测到接近满幅的削波采样。9 段在首尾 40ms 内均低于 -40 dBFS，可优先试听。其他段不因此被判定为坏样本；首尾有声只表示需要检查边界。此检查没有验证角色身份、背景音乐、重叠说话人、日语内容或完整句子。

原文件完整复制，哈希逐一核对；不做破坏性降噪、重采样或增益调整。后续清理结果应另存为派生数据。

## 可复用的 Amadeus 思路

已在本机检查 Amadeus：其 TTS 后端支持逐块生成，内嵌实现为 GPT-SoVITS v3，主要针对 NVIDIA CUDA 优化。当前 Roxy 项目没有复制 Amadeus 的模型或角色语音。复用方向是语音请求、缓存、分块播放与取消机制。

这台电脑为 M4 Pro、16 核 GPU、48GB 统一内存。容量具备尝试单人 TTS 的条件；Mac 的 MPS/CPU 路径与 CUDA 路径不同。Amadeus 标注的 8GiB NVIDIA 显存是远程 Chat + 本地语音的目标配置，不是本机语音占用实测。

## 模型与参考音频

零样本合成需要通用基础模型、角色参考音频和对应的原语言文本。先选择自然语速、中性情绪、无旁人/音乐干扰、完整句子的日语片段。不要只因为文件名标为 8–10 秒就判定其可直接使用。

`config/voice.json` 初始不指定参考音频。字幕需存入 `data/transcripts.tsv`，并在核对后标记 reviewed。`prepare_reference.py` 将已核对的选择写入配置，避免误把自动识别或占位台词当成准确字幕。

若试听相似度不足，再用清理后的语音和准确日文字幕评估 GPT/SoVITS 微调。MPS 推理支持不代表训练路径已验证；模型训练与推理必须分别评估，不在未测量时承诺实时或训练效果。

## 接入形式

建议独立本机服务绑定 `127.0.0.1`，提供日语合成。桌宠请求包括文本、参考音频、参考文本和语言，服务返回音频。该接口也便于替换推理版本，而不将 Amadeus 的 Windows/CUDA 启动逻辑直接塞入 Swift App。

当前 `tts_probe.py` 对接上游 `api_v2.py` 的 `POST /tts`，使用 `text_lang=ja`、`prompt_lang=ja`、`media_type=wav` 和 `streaming_mode=false`，用于第一阶段音色/吞吐测量；它不是 Amadeus 内部接口，也没有证明任何真实模型已成功推理。

服务启动命令由所安装的 GPT-SoVITS 版本决定，当前上游示例：

```sh
python api_v2.py -a 127.0.0.1 -p 9880 -c GPT_SoVITS/configs/tts_infer.yaml
```

## 性能验收

对相同日语测试句分别测冷启动与预热后结果，记录：

- 模型加载时间、峰值进程内存及 MPS 分配量（如使用 MPS）。
- 发出请求到收到首个可播放 PCM 样本的时间，不能把 HTTP/WAV 头到达时间当作首音延迟。
- 生成耗时 / 音频时长，即 RTF。RTF < 1 表示总体生成快于播放，但仍需要测首音延迟和播放缓冲是否连续。
- 长句稳定性、音色相似度、日语读音，以及取消时是否立即停止播音。

完成这些验证后再将实时播放器接入桌宠。当前没有这些实测结果。

## 来源

- GPT-SoVITS 官方项目与 Apple Silicon 安装说明：https://github.com/RVC-Boss/GPT-SoVITS
- 官方合成接口：https://github.com/RVC-Boss/GPT-SoVITS/blob/main/api_v2.py
- 本机 Amadeus：`../Amadeus/tts/backends/gpt_sovits.py`、`../Amadeus/local_tts_infer.py`、`../Amadeus/README.md`（相对于 Roxy 项目根目录）。

