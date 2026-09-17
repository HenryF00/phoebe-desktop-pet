# 菲比本机动态语音

正式安装包已经包含与目标平台匹配的 Python 环境、GPT-SoVITS 推理代码、基础模型、菲比 GPT e15／SoVITS e8 权重和参考音频。用户安装后无需激活 Conda、执行 `api_v2.py` 或手动切换权重。

## 运行方式

Rust `VoiceService` 在语音开启时后台预热，并在每次合成前再次确认状态：

1. 探测固定回环地址 `127.0.0.1:9880`。若已有健康服务，直接复用且不取得其进程所有权。
2. 若端口不可用，校验安装包中两个压缩归档的 SHA-256、平台和 CPU 架构。
3. 第一次运行时安全展开到本机应用数据目录，执行 `conda-unpack` 完成路径迁移；完成标记写入后，后续启动直接复用。
4. 使用固定配置启动 `api_v2.py`，日志和缓存写入应用数据／缓存目录，不修改 `.app`。
5. 应用正常退出时只终止自己持有的子进程；复用的外部 9880 服务不会被关闭。

服务只绑定回环地址。文字回复、历史保存和语音合成彼此解耦；语音失败不会丢失文字回复。

## 双模式朗读策略

- 助手模式：屏幕保留完整答案，只朗读回复首段的简短结论或下一步操作。
- 聊天模式：Agent 默认只回答 1～3 句、约 120 个汉字以内，并完整朗读。
- 两种模式都不朗读 Markdown 标记、网址、代码块或文件路径。
- 语音总开关只控制自动播放；消息上的播放图标仍可重新合成该条消息的 `speech_text`。

## 固定基线

- GPT：`GPT_weights_v2/phoebe_zh_v2-e15.ckpt`
- SoVITS：`SoVITS_weights_v2/phoebe_zh_v2_e8_s912.pth`
- 参考音频：`phoebe_voice_zh/wav/021_自我介绍.wav`
- 参考文本：`我是隐海修会的教士，菲比。岁主在上，愿你的旅途永远有爱与光明垂耀。`
- `text_lang=zh`、`prompt_lang=zh`
- `text_split_method=cut1`、`speed_factor=1`、`fragment_interval=0.32`
- `top_k=15`、`top_p=1`、`temperature=1`、`seed=-1`
- `media_type=wav`、`streaming_mode=false`、`batch_size=1`

`fragment_interval=0.32` 只作用于切分后的片段；短文本在 `cut1` 下通常仍是一个合成段。

## 发布构建

发布构建必须在目标操作系统和 CPU 架构上运行。默认读取：

- GPT-SoVITS：仓库相邻目录 `../GPT-SoVITS`
- Conda 环境：`~/miniconda3/envs/GPTSoVits`

可用 `PHOEBE_GPTSOVITS_ROOT`、`PHOEBE_GPTSOVITS_ENV`、`PHOEBE_GPTSOVITS_PYTHON` 覆盖路径。所选环境需安装 `conda-pack`。执行：

```sh
npm run tauri:build -w @phoebe/desktop
```

构建脚本生成 `python-env.tar.gz`、`gpt-sovits.tar.gz`、SHA-256 清单和上游许可文件。该目录被 `.gitignore` 排除，权重和 Python 环境不会上传 GitHub。需要重新生成时设置 `PHOEBE_REBUILD_VOICE_RUNTIME=1`。

macOS 生成 `.app` 与 DMG；Windows 生成 NSIS 安装程序。由于 PyTorch 等包含原生库，两个平台必须各自构建运行包。

## 安全与资源边界

- HTTP 客户端禁用代理，只访问固定的本机回环端点。
- 清单拒绝绝对路径、父目录跳转、错误平台和错误架构；归档展开使用路径约束并校验 SHA-256。
- 返回音频限制为 64 MiB，验证 `RIFF/WAVE` 后才交给系统播放器；临时 WAV 播放结束即删除。
- 缓存、日志和模型展开目录均在本机应用数据目录，不写入聊天历史或仓库。
- 首次展开约占 4～5 GB，CPU 首次加载模型会比后续启动慢。

## 本机验收记录

2026-09-17 在 macOS Apple Silicon 的正式 `.app` 中完成：首次校验／展开、自动启动 9880、GPT e15 与 SoVITS e8 加载、参考音频中文合成、有效 32 kHz 16-bit 单声道 PCM WAV 返回、二次启动复用缓存，以及应用正常退出后关闭自有语音子进程。Windows 尚需在 Windows x64 实机生成对应运行包并复测。
