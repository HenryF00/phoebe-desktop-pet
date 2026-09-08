# ROXY Desktop Pet · 洛琪希语音桌宠

macOS 高清动漫桌宠，支持 Codex / DeepSeek 聊天、本地中英日声音和任务状态播报。

<p align="center"><img src="assets/pet/frames/master.png" width="220" alt="洛琪希桌宠角色预览"></p>

点击人物打字或长按麦克风说话，双击人物配置模型和语言，双击对话气泡查看聊天记录。字幕流式追加，声音在本机合成。当前版本 **1.6.1**。

这是非官方角色桌宠实验项目。角色素材与用户提供的语音样本保存在 `assets/`；本仓库不包含语音模型权重、Python 环境或聊天记录。

## 当前完成情况

- 已迁入可构建的原生桌宠源码、26 张高清动作帧、九类动作与四方向视线。
- 保留 1.1.2 光影修正：按材质校正明暗，眨眼仅改变眼部，不使用淡入淡出。
- 已导入 `assets/voice/roxy_vad_8-10s/` 原始语音目录，包含 66 个 WAV 和原清单，约 10 分 7 秒。
- 已准备音频体检、日文字幕核对表、参考音频配置工具和本地 GPT-SoVITS 合成探针。
- 已安装 MLX Whisper，下载识别模型并在本机完成全部 66 段日语自动转写；字幕仍需回听校对。
- 已安装 GPT-SoVITS v2ProPlus、CPU 推理环境与日语词典，并用参考音频生成了日语新台词。
- 本机短句测试：首次请求 9.41 秒 / 2.96 秒音频；预热后 1.50 秒 / 3.08 秒音频。采样观察到服务进程内存峰值约 4.54 GiB。这不是首音延迟或长对话稳定性测试。
- **1.5.0 人物旁流式聊天**：单击展开单行半透明输入栏，回车发送，右侧麦克风直接长按录音、松开发送。没有下排按钮和矩形输入高亮。双击人物打开设置，独立选择中英字幕、中英日语音，以及 Codex 订阅或 DeepSeek API；模型可自填，DeepSeek 密钥保存在 macOS 钥匙串。字幕随模型输出显示，日语 TTS 按音频块合成和播放，中英文为减少吞字采用逐句播放，沿用第 12 段参考声音，没有微调。
- **1.5.1 中文口音修正**：采用用户试听选中的 B 方案，中文仅使用参考音频，不带日语台词提示；聊天和播报统一生效。未重新训练，日语声音保留。见 [中文口音对照](docs/chinese-accent.md)。
- **1.6.1 对话记录与稳定流式字幕**：文字持续追加，语音按句播放，播放开始、换句和结束均不再覆盖正文。保留本轮输入和语音识别文字；双击气泡查看本机聊天记录，重启后可继续查看，双击人物仍是设置。
- 任务结束/等待确认播报已实现并缓存；本机 7 个 Hooks 尚未信任，需要在 Codex CLI 的 `/hooks` 中信任后才能收到真实事件。
- 使用步骤及实测边界见 [语音聊天使用说明](docs/voice-chat.md)。

项目使用 Swift / AppKit 构建桌面界面，通过 Python 连接聊天模型、本地 ASR 与 TTS。

## 构建桌宠

需要 macOS 和 Xcode Command Line Tools。以下操作在项目目录执行：

```sh
python3 scripts/build.py
```

产物是 `dist/Roxy HD.app`，双击即可打开。与旧版本使用同一 Bundle ID，因此保留原来的位置与大小；先退出旧桌宠再打开新项目构建的 App，避免重复实例。

App 的声音依赖此项目内的 Python 环境、模型和素材；移动项目后请重新构建，不能只复制 App 到另一台电脑。源语音文件放在项目里，不重复打包进 App。所有本地源文件保持原样，SHA-256 在 `data/voice-source-sha256.json`。

## 洛琪希语音怎么适配

建议先验证参考音频驱动的日语合成，再根据音色相似度决定是否微调。

1. **清理与标注素材**：先确认确为同一说话人，去掉旁人、音乐重叠、喊叫失真以及被截断的句子；用日语 ASR 转写后逐段核对，记录原声逐字日文，不能用中文翻译替代。
2. **挑参考音频**：从自然、清晰、完整的片段中选一段，填写 `data/transcripts.tsv` 的 `text`、`reviewed=true`，运行 `prepare_reference.py`。`docs/reference-candidates.md` 只是基于首尾音量筛出的试听候选，不保证音质或说话人正确。
3. **跑本地 TTS 试听**：安装支持 Apple Silicon 的 GPT-SoVITS 推理环境和基础权重，启动本机 API，用参考音频和字幕生成新日语句子。先评价音色、发音、长短句稳定性。
4. **必要时微调**：如果零样本音色不够接近，再用核对后的数据做角色微调。66 个切片不等于 66 句完整训练样本，不应直接整包送进训练。
5. **语音聊天已接入**：日语回复按句合成后依次播放，支持停止；已接入 PCM 音频块流式播放；尚未实现免提抢话、回声消除与口型。音量沿用系统输出音量。

实际操作顺序见 [训练与合成操作说明](docs/training-walkthrough.md)，实现边界见 [语音适配说明](docs/voice-adaptation.md)。

## 已提供的工具

```sh
# 启动本机 GPT-SoVITS 服务（保持此终端运行）
python3 scripts/start_tts.py

# 在另一终端生成未校对参考文本的试听；不修改正式音色配置
python3 scripts/tts_probe.py --draft-reference roxy_seg_0012_0109646-0119212.wav --text 'こんにちは。今日も一緒に頑張りましょう。'

# 本地自动转写；已有相同音频与模型的结果会跳过
.venv/bin/python scripts/transcribe_voice.py

# 只填充空白且未校对的字幕行，保留 reviewed=false
python3 scripts/import_asr_drafts.py

# 校对后导出 GPT-SoVITS 训练清单
python3 scripts/export_training.py

# 重新检查全部素材；不会覆盖已填写的字幕表
python3 scripts/audit_voice.py

# 在字幕表里核对并填写后，选定参考片段
python3 scripts/prepare_reference.py roxy_seg_0016_0147910-0157476.wav

# 检查将发送给本地 GPT-SoVITS 的请求（不启动或下载模型）
python3 scripts/tts_probe.py --dry-run

# 需先启动本机 GPT-SoVITS api_v2 服务
python3 scripts/tts_probe.py --text 'こんにちは。今日も一緒に頑張りましょう。'
```

探针输出 `generated/probe.wav` 和 `generated/probe.json`，测整段生成耗时、音频时长和实时率。它采用缓冲响应，不测首个可播放音频延迟，这项探针独立于 1.2.0 的聊天分句播放。

指定 `--draft-reference` 时输出改为 `generated/draft-preview.wav` 和 `.json`，其中明确记录 `reference_reviewed=false`。本次已生成的首段试听另存为 `generated/draft-preview-cold.wav`。环境、来源与实测说明见 [本地语音运行说明](docs/local-tts.md)。

## 目录

```text
Sources/main.swift                  原生桌宠
assets/pet/                         高清帧和动作清单
assets/voice/roxy_vad_8-10s/          用户提供的原始音频与清单
config/voice.json                   日语音色与本机服务配置
data/transcripts.tsv               待核对的日文字幕表
data/voice-audit.json               音频格式和切片边界检查
scripts/                           构建、素材检查、参考配置及试听探针
docs/                              语音适配说明及桌宠历史
qa/visual/                         光影修正记录
dist/                              构建产物（不提交）
models/                            后续模型权重（不提交）
generated/                         合成音频及测量结果（不提交）
```

Codex 状态桥接脚本在 `scripts/codex_bridge.py`。本机已有的 Hooks 安装位置仍为 `~/Library/Application Support/RoxyHD/bridge.py`；本次整理没有修改其信任状态或重新安装 Hooks。
