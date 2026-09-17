# 菲比本机动态语音

当前 Tauri 版本在收到 `completed.reply.speech_text` 后，由 Rust 连接固定的本机
`http://127.0.0.1:9880/tts`，生成完整 WAV，再交给系统播放器播放。文字回复、历史保存和
语音合成彼此解耦；语音失败不会丢失文字回复。

旧 Swift/AppKit 项目的 Python Worker 和 `VoiceController` 只是设计参考，不是当前应用的
运行入口。当前应用不会启动或打包 GPT-SoVITS，也不会把训练集或模型权重复制进 `.app`。
发布包只包含 `021_自我介绍.wav` 这一段推理参考音频。

## 启动服务

在第一个终端运行：

```sh
mkdir -p /private/tmp/phoebe-numba-cache /private/tmp/phoebe-matplotlib-cache
source /Users/henryfang/miniconda3/etc/profile.d/conda.sh
conda activate GPTSoVits
cd /Users/henryfang/Desktop/个人/GPT-SoVITS
NUMBA_CACHE_DIR=/private/tmp/phoebe-numba-cache \
MPLCONFIGDIR=/private/tmp/phoebe-matplotlib-cache \
python api_v2.py -a 127.0.0.1 -p 9880
```

服务只应绑定 `127.0.0.1`，不要改为局域网或公网监听。

在第二个终端加载项目基线权重。WebUI 中选中的模型不会自动同步到独立 API：

```sh
curl --noproxy '*' -G 'http://127.0.0.1:9880/set_gpt_weights' \
  --data-urlencode 'weights_path=GPT_weights_v2/phoebe_zh_v2-e15.ckpt'

curl --noproxy '*' -G 'http://127.0.0.1:9880/set_sovits_weights' \
  --data-urlencode 'weights_path=SoVITS_weights_v2/phoebe_zh_v2_e8_s912.pth'
```

随后在菲比设置中开启“自动播放菲比语音”，点击“检测服务”。

## 双模式朗读策略

- 助手模式：屏幕保留完整答案，但只朗读回复首段的简短结论或下一步操作。语音摘要会限制长度，详细步骤、来源、代码和路径留在聊天窗口。
- 聊天模式：Agent 默认只回答 1～3 句、约 120 个汉字以内；整段自然语言答案都会朗读。
- 两种模式都不会朗读 Markdown 标记、网址、代码块或文件路径。Markdown 链接只朗读链接文字；代码块替换成“代码已显示在聊天窗口”。
- 右键菜单中的语音项目会随模式显示“精简播报”或“完整朗读”。语音总开关关闭后两种模式都不自动播放；消息播放按钮仍重播该消息已经生成的 `speech_text`。

当前模式由 Rust 从本机设置随每一轮 prompt 发送给固定 Agent Sidecar。Sidecar 使用模式化系统提示生成回复，并在完成事件中分别返回 `display_text` 与 `speech_text`；GPT-SoVITS 仍只接收 `speech_text`。

## 固定基线

- GPT：`GPT_weights_v2/phoebe_zh_v2-e15.ckpt`
- SoVITS：`SoVITS_weights_v2/phoebe_zh_v2_e8_s912.pth`
- 参考音频：`phoebe_voice_zh/wav/021_自我介绍.wav`
- 参考文本：`我是隐海修会的教士，菲比。岁主在上，愿你的旅途永远有爱与光明垂耀。`
- `text_lang=zh`、`prompt_lang=zh`
- `text_split_method=cut1`（凑四句一切）
- `speed_factor=1`
- `fragment_interval=0.32`
- `top_k=15`、`top_p=1`、`temperature=1`
- `seed=-1`，对应不锁定随机种子
- `media_type=wav`、`streaming_mode=false`、`batch_size=1`

`fragment_interval=0.32` 只作用于切分后的片段；短文本在 `cut1` 下通常仍是一个合成段，
不会强制在“菲比。”与下一句之间插入停顿。

## 运行边界

- Rust HTTP 客户端禁用代理，只允许固定回环地址，避免本机搜索代理干扰 9880。
- 返回值限制为 64 MiB，并验证 `RIFF/WAVE` 头后才写入应用缓存目录。
- macOS 使用系统 `/usr/bin/afplay`；Windows 使用系统 `System.Media.SoundPlayer`。
- 新一轮提问会停止上一段播放。合成中的上游计算不能强制终止，但迟到结果会按 generation
  丢弃，不再播放。
- 临时 WAV 播放结束后删除；训练权重、参考训练集和生成音频不会写入聊天历史。
- 设置中的“检测服务”只能确认 API 和参考音频可用；`api_v2.py` 没有查询当前权重的接口，
  因此仍需按上面的命令加载 e15/e8。

## 本机验收记录

2026-09-16 使用上述配置生成测试句“你好，我是菲比。祝你今天也有爱与光明相伴。”，得到
32 kHz、16-bit、单声道 PCM WAV，时长 6.8 秒，并通过 macOS 系统播放器完整播放。
