# 菲比助手 · Windows x64 打包与验收清单

> 语音运行时是平台专属二进制，macOS aarch64 的 `voice-runtime` 无法在 Windows 使用，
> **必须在 Windows x64 机器上重新生成**。本文档给出从零到可安装 `.exe` 的完整步骤，
> 以及逐项实机验收清单。

---

## 0. 前置说明

- 需要的机器：一台 **Windows 10/11 x64**（建议 ≥ 40 GB 可用磁盘，语音包 2.5G + 展开 4~5G）。
- 需要从 **macOS 机器拷贝**菲比专用语音权重（否则 Windows 上没有语音）。
- 全程在 Windows 上执行，`conda`/`python`/`pip`/`cargo`/`node` 命令都在 Windows 终端（PowerShell 或 CMD）里跑。

---

## 1. 环境准备（一次性）

| 工具 | 说明 | 验证命令 |
|---|---|---|
| Node.js | ≥ 22（bundle 脚本强制要求） | `node -v` |
| Rust + MSVC | rustup 装 `stable-x86_64-pc-windows-msvc`；需 **Visual Studio Build Tools**（C++ 桌面开发） | `rustc -V`、`cargo -V` |
| miniconda | 生成语音运行时用 | `conda --version` |
| Git | clone 仓库用 | `git --version` |
| WebView2 | Win10 21H2+ / Win11 自带；老系统需装 Evergreen Runtime | 打包后能开窗口即可 |

---

## 2. 准备 GPT-SoVITS + 菲比权重（最耗时）

### 2.1 从 macOS 机器拷贝权重和基础模型

macOS 机器路径：`/Users/henryfang/Desktop/个人/GPT-SoVITS`。

拷贝以下内容到 Windows 的 GPT-SoVITS 目录（比如 `D:\GPT-SoVITS`）：

```
GPT_weights_v2/phoebe_zh_v2-e15.ckpt          (约 155 MB)
SoVITS_weights_v2/phoebe_zh_v2_e8_s912.pth    (约 85 MB)
GPT_SoVITS/pretrained_models/chinese-roberta-wwm-ext-large
GPT_SoVITS/pretrained_models/chinese-hubert-base
GPT_SoVITS/pretrained_models/fast_langdetect
```

> 参考音频 `021_自我介绍.wav` 已在仓库 `phoebe_voice_zh/wav/` 里，无需单独拷贝。
> 训练用的其它 pretrained 模型（s1/s2/s2G 等）推理不需要，可不拷。

### 2.2 准备 GPT-SoVITS v2 代码 + conda 环境

1. clone GPT-SoVITS v2（权重是 `phoebe_zh_v2`，务必用 v2 分支）：
   ```powershell
   git clone https://github.com/RVC-Boss/GPT-SoVITS.git D:\GPT-SoVITS
   cd D:\GPT-SoVITS
   git checkout v2        # 或按仓库当前 v2 分支
   ```
   把 2.1 拷来的权重/模型放到对应位置（覆盖 clone 出来的目录）。

2. 创建 conda 环境（按 GPT-SoVITS v2 官方 Windows 指南，这里给关键步骤）：
   ```powershell
   conda create -n GPTSoVits python=3.9 -y
   conda activate GPTSoVits
   # CPU 版 PyTorch（按官方 requirements 装依赖）
   pip install torch torchaudio --index-url https://download.pytorch.org/whl/cpu
   pip install -r requirements.txt
   # ffmpeg 需在 PATH 中（GPT-SoVITS 依赖）
   # conda-pack 是打包必需
   pip install conda-pack
   ```

3. 确认权重已就位，并验证 API 能起：
   ```powershell
   python api_v2.py -a 127.0.0.1 -p 9880 -c GPT_SoVITS/configs/tts_infer.yaml
   ```
   能启动并加载权重即成功（之后 Ctrl+C 退出）。

---

## 3. 拉代码 + 打包

### 3.1 获取代码

```powershell
git clone <你的 phoebe-desktop-pet 仓库地址> phoebe-desktop-pet
cd phoebe-desktop-pet
```

### 3.2 安装依赖

```powershell
npm install
```

### 3.3 生成语音包 + 打包 NSIS

```powershell
$env:PHOEBE_GPTSOVITS_ROOT = "D:\GPT-SoVITS"
$env:PHOEBE_GPTSOVITS_ENV = "$env:USERPROFILE\miniconda3\envs\GPTSoVits"
npm run tauri:build -w @phoebe/desktop
```

说明：
- 脚本会先 `npm run build` + `bundle`（esbuild 打包 Agent + 浏览器 Sidecar），
  再检测 `resources/voice-runtime` 是否是 windows/x86_64；不是则调用
  `prepare-voice-runtime.py` 用上面的 GPT-SoVITS 环境生成（约数分钟到十几分钟）。
- 产物：`target/release/bundle/nsis/*-setup.exe`（NSIS 安装程序）。

---

## 4. 安装 + 实机验收清单

在 Windows 机器上运行 `setup.exe` 安装，然后逐项打勾：

### 基础与密钥
- [ ] 首次启动能看到透明置顶桌宠窗口。
- [ ] 设置里填入 DeepSeek Key 保存成功；**重启应用后 Key 仍在**（验证 Windows Credential Manager）。
- [ ] 发一句文字能收到回复（验证 Sidecar + DeepSeek）。

### 语音（重点验收）
- [ ] 首次合成：较慢（展开 4~5G + CPU 加载模型），能返回有效 WAV 并播放。
- [ ] 再次合成明显变快；**重启后**复用缓存、不再展开。
- [ ] 助手模式只朗读简短结论；聊天模式完整朗读短句。
- [ ] 应用正常退出后，语音子进程（`api_v2.py`）被关闭。

### 定位
- [ ] 问「我在哪个城市」→ 弹 Windows 位置授权 → 返回城市级坐标。
- [ ] 拒绝授权时给出明确提示。

### 受控浏览器（Phase 4）
- [ ] 「打开 example.com」→ 弹审批 → 无头 Chrome/Edge 打开并 snapshot。
- [ ] 多步操作（打开→搜索→点击）只首次确认。

### 受控文件 / 应用操作
- [ ] 授权一个文件夹后能 `list_directory` / `read_text_file`。
- [ ] `list_installed_apps` 能枚举到 Windows 应用；`launch_application` 能启动。
- [ ] 写入/删除弹审批并展示 diff；删除进回收站。

### 多会话 / 模式隔离 / 记忆
- [ ] 助手/聊天模式各自独立会话，切换模式不串味；聊天窗口有模式徽章和不同主题色。
- [ ] 「记住我喜欢 X」→ 弹 `remember_preference` 审批 → 设置页长期记忆出现该条。

### 窗口与系统集成
- [ ] 拖动、透明穿透、缩放、跨屏拖动正常。
- [ ] 拖到外接屏 → 拔掉外接屏 → 桌宠 2 秒内回到主屏（热插拔校验）。
- [ ] 睡眠唤醒后桌宠不越界。
- [ ] 重启后位置恢复正确。
- [ ] 托盘「显示菲比/聊天/设置/退出」正常。
- [ ] 开机启动开关生效（重启 Windows 验证）。

---

## 5. 已知会卡住的点（提前排查）

1. **语音运行时生成失败**：`conda-pack` 没装、权重/pretrained 没拷全、ffmpeg 不在 PATH。
2. **NSIS 打包失败**：缺 VS Build Tools（C++），或首次打包 tauri 下载 NSIS 依赖网络慢。
3. **定位/浏览器/开机启动无反应**：这些 Windows 分支代码写了但从未实机验收，出现问题是正常的，需要逐项排。
4. **透明/穿透异常**：和显卡驱动、多显示器 DPI 相关，先单屏验收再测多屏。

> 以上任何一步卡住，把**完整报错日志**发回来，我帮你定位。
