# 菲比助手 · Phoebe Assistant

> **ver0.9.1** · 基于 Tauri 2、React 和 Pi Agent 的本机 AI 桌面宠物与受控本机操作 Agent

<p align="center">
  <img src="apps/desktop/public/pet/phoebe-orb-avatar-v1.png" width="152" alt="菲比助手头像">
</p>

菲比助手是一个桌面常驻角色与本机 AI Agent 结合的实验项目。它使用透明置顶窗口展示可动的菲比桌宠，通过独立聊天窗口连接 DeepSeek，并在本机管理 API Key、对话历史、显式记忆和 GPT-SoVITS 语音。

从 ver0.3 起，菲比不再只是聊天：所有操作系统操作统一经过 Rust 权限网关（Node Sidecar 只提交请求，Rust 独立校验并执行），可以在用户授权目录内受控地读取、写入、移动和删除文件，并加入了按 Token 预算的上下文管理，避免长时间会话把上下文喂爆。ver0.4 进一步接入应用与文件操作：可以枚举已安装应用、启动或切换应用，并在文件管理器中显示或用指定应用打开授权目录内的文件——全程仍由同一个权限网关审批。ver0.5 补齐了真正的多会话：会话列表、新建／切换／重命名／删除，消息按会话隔离，切换时把该会话的文本历史重新灌回模型上下文，可以接着聊。ver0.6 进一步把会话按**交互模式**隔离：助手模式与聊天模式各自有独立的会话列表与模型上下文，切回模式能接着上一段聊，不会串味；两个模式的聊天窗口也有不同的视觉主题与空状态提示。ver0.7 接入**受控浏览器（Phase 4）**：独立的 Browser Sidecar 用 playwright-core 驱动本机 Chrome/Edge 无头实例，可以打开网页、读取快照、点击、输入、选择、等待和提取文本，全部走同一个 Rust 审批网关。ver0.8 把**长期记忆升级为 Agent 工具授权链**：模型可以提议保存（`remember_preference`）或删除（`forget_preference`）偏好，但要经用户逐次确认与敏感词校验；新增绑定目标的 `launch_wuthering_waves` 鸣潮启动工具；并为长对话加了**会话摘要**，超预算时把早期消息压缩为结构化检查点摘要而非直接丢弃。ver0.9 加入**图像/视频多模态识别**（附加图片；视频自动抽帧，需选用支持图像的模型）与**系统级文档读写**（`read_system_file` / `write_system_file`，按绝对路径、逐次确认、屏蔽敏感位置）。ver0.9.1 接入**视频分析后端**：新增 `analyze_video` 工具，把授权目录内的短片交给阿里云百炼 Qwen-VL-Max 做视频理解，菲比再把返回的文字分析转述为复盘讲解。

ver0.7 当前以 **macOS Apple Silicon** 为主要验收环境；代码保留 Windows 适配，但尚未完成 Windows 实机验收。

## 已实现功能

### ver0.9.1 新增

- **视频分析工具（路径 C · 前两步）**：新增 `analyze_video`，把授权目录内的视频交给阿里云百炼 Qwen-VL-Max 做视频理解，返回文字分析，菲比再用自身人设转述为复盘/建议。≤16 MB 的短片直接上传；更大的长视频用 `startSec`/`endSec` 指定时间窗（单次≤300 秒），桌面核心用 ffmpeg 本地裁剪后再上传。视频会离开本机，因此按文件夹授权逐次确认（信任模式可对已授权文件夹免确认）。
- **DashScope API Key 管理**：设置页新增「DashScope API Key（视频分析）」输入框，与 DeepSeek 密钥分开保存在系统钥匙串/凭证管理器，不回显、不写入项目文件。

### ver0.9 新增

- **图像/视频多模态识别**：聊天窗可附加 PNG/JPEG/WebP 图片，或短视频（前端自动抽取若干关键帧）；图像作为多模态内容交给模型，需在设置里选择 `DeepSeek V4 Flash Vision` 模型。图片在前端下采样压缩，发送后不会随上下文重复发送。
- **系统级文档读写**：新增 `read_system_file` / `write_system_file`，模型可用**绝对路径**读写系统任意位置的文本文件（如“把总结保存到 ~/Desktop/report.md”）。每次调用都逐次确认并展示绝对路径（写入还展示 diff）；`.ssh`、钥匙串、`.env` 等敏感位置即使指定也被拒绝。
- **自定义称呼**：设置里可填菲比对你的称呼（默认「漂泊者」，留空恢复默认）；名称会作为数据注入系统提示，仅在合适场合自然使用。
- **生日祝福**：设置里可填生日（`MM-DD`）；生日当天首次对话，菲比会主动送上祝福并切换为开心表情，每天只祝福一次（用本机日期去重）。生日语音**直接播放内置的「017 生日祝福」预录音频**（菲比原声），不再现场合成。

### ver0.8 新增

- **会话摘要**：长对话的历史超过 Token 预算时，不再简单丢弃最旧消息，而是先调用模型生成一份结构化检查点摘要（目标/约束/已完成/进行中/关键决定/下一步/关键上下文），保留摘要 + 最近消息；后续再超时会把新消息并入旧摘要（增量更新）。摘要作为一条带标记的 user 消息注入，模型可读；摘要失败时回退到原有的丢弃行为。
- **长期记忆的 Agent 工具授权链**：新增 `remember_preference` / `forget_preference`，模型可以提议保存或删除偏好，但保存按标题去重（同标题更新内容）、删除按标题精确匹配；两者都**逐次确认**，且保存经过容量与敏感词校验（拒绝密码/凭证/支付信息）。记忆仍为 `user_explicit` 来源，与手动管理同一信任级别。
- **鸣潮启动工具**：`launch_wuthering_waves` 按应用名扫描本机安装目录（匹配 `Wuthering Waves` / `鸣潮`），模型不需要传路径；首次启动确认，之后信任模式可免确认（信任键 `app:wuthering-waves`）。

### ver0.7 新增

- **受控浏览器（Phase 4）**：新增独立 Browser Sidecar（`packages/agent/src/browser-sidecar.mjs`），用 playwright-core 驱动本机 Chrome/Edge 的**无头实例**（独立临时配置，不触碰你的登录会话与历史）。
- 八个浏览器工具全部走 Rust 审批网关：`browser_open`（打开 HTTPS，需逐次确认，信任模式可按域名免确认）、`browser_snapshot`（生成带元素 ref 的可访问快照）、`browser_click`、`browser_type`、`browser_select`、`browser_wait`、`browser_extract_text`、`browser_close`。
- 浏览器页面快照给每个可交互元素分配 `e0/e1/…` 引用，模型据此点击/输入/选择；打开页面之后的操作视为已授权（auto），不再逐次打断。
- 浏览器进程由 Rust 统一管理：应用退出时关闭自己启动的无头浏览器；只接受 HTTPS、拒绝内嵌凭据链接。

### ver0.6 新增

- **按交互模式隔离会话**：SQLite 升级到 v4，`conversations` 新增 `mode` 列；助手模式与聊天模式各自维护独立的会话列表、激活会话和模型上下文。
- 切换模式会自动切到该模式的会话并重新灌入其文本历史（经 `pruneMessages` 预算裁剪）；回复也按发起时的模式保存，切回模式能接着上一段聊，不会串味。
- 两个模式的聊天窗口有**视觉区别**：聊天模式用暖色主题、亲近空状态和闲聊建议；助手模式保持蓝金主题与任务型建议。头部有「助手／聊天」模式徽章。

### ver0.5 新增

- **多会话（方案 B）**：SQLite 升级到 v3，新增 `app_meta` 与 `conversations.title/updated_at`；消息按 `conversation_id` 隔离，默认会话 `desktop-main` 自动沿用旧数据。
- 聊天窗“更多”菜单里提供会话列表，支持**新建／切换／重命名／删除**；标题自动取首轮提问（可手动修改），删除最后一个会话会自动新建一个空会话。
- 切换会话时，Rust 会把该会话最近 50 轮的 user/assistant 文本通过 `conversation` 命令重新灌回 Sidecar 的 `agent.state.messages`（经 `pruneMessages` 预算裁剪），切回去能接着聊；工具调用与结果不持久化。
- “清理聊天历史”现在只清理当前会话，会话列表保留。
- **定位改为系统原生获取**：`get_device_location` 不再依赖 WebView 定位和“使用当前位置”按钮，改由 Rust 直接调用 macOS CoreLocation / Windows Geolocation，首次触发系统授权框，之后按需一次性取城市级位置。

### ver0.4 新增

- **应用与文件操作（Phase 3）**：`list_installed_apps`、`launch_application`、`focus_application`、`reveal_file`、`open_file_with_application`。
- 模型只能用不透明的 `appId`（来自枚举结果）引用应用，**不能传入任意文件路径**；启动、切换、显示、用指定应用打开都需要逐次确认，信任模式可对单个应用“始终允许”。
- `reveal_file` / `open_file_with_application` 只能作用于**已授权目录内**的文件，复用相对路径与越权检查。

### ver0.3 新增

- **操作权限网关（Phase 1）**：Node↔Rust 双向工具协议、逐次审批、操作策略、审计与急停。
- **受控文件（Phase 2）**：授权目录 + 相对路径的只读与写入/移动/删除；绝对路径从不进入模型上下文。
- **上下文预算管理**：CJK 感知的 Token 裁剪，每次模型请求前（含同一轮内的多次工具调用）都会执行，工具结果按需分页；右键快捷菜单会显示当前上下文占用（x / 24k）。
- **错误分类**：把上下文溢出、鉴权、余额、限流与网络问题分开提示，不再统一显示“模型请求失败”。

### 桌宠与交互

- 透明、无边框、始终置顶的 Tauri 桌宠窗口。
- 支持拖动、屏幕边界限制、跨启动位置恢复和多显示器坐标适配。
- 单击打开／收起聊天，双击打开设置，右键打开快捷菜单。
- 系统托盘支持恢复隐藏的桌宠、打开聊天、打开设置和退出。
- 75%、100%、125%、150% 固定比例缩放，以及 84×84 的圆形头像“图标化”模式。
- 助手模式与聊天模式使用不同立绘和动作节奏。
- 待机呼吸、自然眨眼、思考、工具执行、倾听、说话、点头、说明、害羞、担忧、挥手和取消复位等状态。
- 连续空闲 120 秒后自动触发一次挥手，并支持手动挥手。

### Agent 聊天

- 独立的 Pi Agent Sidecar，通过 JSON Lines 与 Rust 桌面核心通信。
- DeepSeek 流式文字对话、Markdown 显示、中止生成，并把上下文溢出、鉴权、余额、限流与网络等失败原因分开提示。
- 已接入菲比人设系统提示，同时保留真实性、工具权限和不伪造结果的边界。
- **助手模式**：适合任务、资料和工具结果，屏幕保留完整回答。
- **聊天模式**：回答默认更简短、更自然，适合角色对话。
- 对话完成后生成 `emotion` 和 `gesture`，用于驱动桌宠反应。
- 上下文按 Token 预算管理：每次请求前（含同一轮内的多次工具调用）裁剪历史，先丢弃旧的工具输出、再考虑对话文本，并对过大的工具输出做截断；单次文件读取默认 16 KB、可按 `offset` 分页，避免累积 Tool 结果把上下文喂爆。历史超过预算时先做**会话摘要**，把早期消息压缩为结构化检查点摘要而非直接丢弃。
- 右键菜单可查看当前轮次与本次启动的 Token 用量，以及当前上下文占用（x / 24k）。

### 工具能力

- 获取当前时间与本应用状态。
- 联网搜索，返回可解析的 HTTPS 来源链接；支持可选的本机回环 HTTP 代理。
- 仅读取用户在当前轮次主动选择的小型文本文件。
- 通过操作系统原生定位（macOS CoreLocation / Windows Geolocation）获取城市级粗略位置；首次调用触发系统定位授权，之后按需一次性读取，不持续跟踪、不依赖 WebView 定位。
- 经用户确认后，用系统默认浏览器打开单个 HTTPS 链接，并支持“始终允许该网站”。
- **受控文件（Phase 2a/2b）**：授权目录 + 相对路径。只读工具 `list_granted_folders`、`list_directory`、`read_text_file`、`search_files`；写入工具 `write_file`、`move_file`、`delete_file` 仅在用户选择“可读写”授权后可用，且**每次写入或删除都必须逐次确认并展示 diff**。Rust 会规范化路径并拒绝绝对路径、`..`、符号链接逃逸与符号链接写入，屏蔽 `.ssh`、钥匙串、`.env` 等敏感位置，限制文件大小与搜索范围；写入采用临时文件 + rename 的原子方式，删除默认移入系统废纸篓。绝对路径从不进入模型上下文，也不会出现在工具结果里。
- **应用与文件操作（Phase 3）**：`list_installed_apps` 只读枚举已安装应用（不透明 `appId` + 名称，缓存 10 分钟）；`launch_application`、`focus_application` 只能使用枚举到的 `appId`，**绝不接受路径**；`reveal_file`、`open_file_with_application` 只能引用已授权目录内的文件。启动/切换/显示/打开均逐次确认，信任模式可对单个应用“始终允许”。macOS 用 `/usr/bin/open`（`-R`/`-a`）以参数数组调用，不经过 shell。
- **受控浏览器（Phase 4）**：独立 Browser Sidecar 用 playwright-core 驱动本机 Chrome/Edge 的**无头实例**（独立临时配置，不触碰登录会话与历史）。`browser_open` 只接受 HTTPS、逐次确认（信任模式按域名免确认）；`browser_snapshot` 给可交互元素分配 `e0/e1/…` 引用，供 `browser_click`/`browser_type`/`browser_select` 定位；另有 `browser_wait`、`browser_extract_text`、`browser_close`。打开页面之后的操作视为已授权（auto）。浏览器进程由 Rust 管理，退出时关闭自己启动的无头浏览器。
- **操作权限网关（Phase 1）**：所有操作系统操作由 Rust `ToolBroker` 统一处理。Node Sidecar 只提交 `tool_request`，Rust 独立校验每轮白名单、参数 Schema 与操作策略，执行后把 `tool_result` 写回；Sidecar 无法自行执行或批准操作。
- 操作策略提供“关闭／标准／信任”三档，默认“标准”逐次确认；聊天模式不注册会产生本机副作用的操作工具（只读的 `list_granted_folders` 除外）。审批以聊天窗模态为主、Rust 原生对话框兜底。
- 每次执行或拒绝都会写入本机 `audit.jsonl`，并可从聊天窗或设置页立即停止所有操作。
- Rust 工具网关会校验调用者、参数和允许列表，不提供任意 Shell 执行入口。

### 本机数据与安全

- DeepSeek API Key 只保存到 macOS Keychain／Windows Credential Manager，不写入仓库、对话历史或安装包。
- 设置、窗口位置、SQLite 历史和长期记忆均位于各自设备的应用数据目录，当前不做云端同步。
- 操作审计写入应用数据目录的 `audit.jsonl`；“信任”授权保存在 `grants.json`，只包含用户明确“始终允许”的网站域名。
- 隐私模式下不持久化本轮对话。
- 多会话：会话按交互模式隔离，各自保存最近 50 轮对话；可在会话列表中新建、切换、重命名、删除，并支持清空当前会话历史。
- 长期记忆可由用户在设置中显式管理，也可由 Agent 通过 `remember_preference` / `forget_preference` 提议（逐次确认），均经过本机确认与敏感内容检查。

### 语音

- 已接入本机 GPT-SoVITS `api_v2.py` 动态中文 TTS。
- 发布包内置与当前平台匹配的 Python 环境、GPT-SoVITS 推理代码、基础模型和菲比 e15/e8 权重；安装后不需要另开终端。
- Rust 会先检测 `127.0.0.1:9880`：已有健康服务时直接复用，否则自动展开并启动内置服务；退出时只关闭由本次应用启动的子进程。
- 助手模式只朗读简短结论；聊天模式完整朗读简短回答。
- 每条已完成回复都提供简单的重播按钮。
- 支持停止播放、丢弃过期合成结果，并在播放结束后删除临时 WAV。
- 第一次启用语音会在应用数据目录完成校验和展开，后续启动直接复用；训练数据不会进入安装包。

详细启动与权重配置见 [菲比本机动态语音](docs/phoebe-voice.md)。

## 快速开始

### 环境需求

- Node.js 22+
- npm
- Rust stable 工具链
- Tauri 2 在目标系统上的构建依赖
- DeepSeek API Key
- 仅发布构建者需要：目标平台上的 GPT-SoVITS Conda 环境、e15/e8 权重和 `conda-pack`

平台差异：

- **macOS**：需要 Xcode Command Line Tools（`xcode-select --install`）。
- **Windows**：需要 **Visual Studio Build Tools（C++ 桌面开发）** 和 [WebView2 Evergreen Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Win10 21H2+ / Win11 已自带）。

### 安装依赖

```sh
npm ci
```

### 启动桌面应用（开发模式）

```sh
npm ci            # 或 npm install
npm run desktop:dev
```

启动后，右键菲比 → **打开设置** → 保存 DeepSeek API Key，再单击桌宠打开聊天。

- **macOS**：装好 Xcode Command Line Tools 后直接运行即可。
- **Windows**：先装好 Visual Studio Build Tools 与 WebView2，再运行同样的命令（`npm run desktop:dev`）。

### 浏览器 UI 预览

```sh
npm run desktop:web
```

- 桌宠：`http://127.0.0.1:1420/?view=pet`
- 聊天：`http://127.0.0.1:1420/?view=chat`
- 设置：`http://127.0.0.1:1420/?view=settings`

浏览器预览不具备系统钥匙串、本机窗口、托盘和真实 Agent Sidecar 能力。

## 构建与测试

```sh
# TypeScript 与前端生产构建
npm run build

# 共享协议和 Agent 测试
npm test

# Rust 桌面核心测试
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml

# 生成 macOS .app 与 DMG
npm run tauri:build -w @phoebe/desktop
```

release 构建会打包固定的 Node Sidecar、编译后的 Agent、Node 许可文件、参考音频，以及与**构建平台和 CPU 架构一致**的 GPT-SoVITS 离线运行包。默认来源是相邻目录 `../GPT-SoVITS` 和 `~/miniconda3/envs/GPTSoVits`；也可通过 `PHOEBE_GPTSOVITS_ROOT`、`PHOEBE_GPTSOVITS_ENV` 和 `PHOEBE_GPTSOVITS_PYTHON` 指定。生成的多 GB 归档位于 Git 忽略的 `apps/desktop/src-tauri/resources/voice-runtime/`。

macOS 构建产出 `.app` 和压缩 DMG；Windows 构建产出 NSIS 安装程序。语音运行包包含原生依赖，必须分别在 macOS Apple Silicon 和目标 Windows x64 机器上构建，不能把 macOS 归档直接用于 Windows。

### 打包与安装（按平台）

提供一键打包脚本（自动检查工具链与 GPT-SoVITS 输入后打包）：

- **macOS（Apple Silicon）**：`./scripts/build-macos.sh`，产物为 `target/release/bundle/macos/Phoebe Assistant.app` 与 DMG；双击 `.app` 即可运行（未签名/未公证的本地验收包，首次打开可能需右键「打开」）。
- **Windows x64**：`powershell -ExecutionPolicy Bypass -File .\scripts\build-windows.ps1`，产物为 `target/release/bundle/nsis/*-setup.exe`；脚本会自动生成 Windows 语音包（语音运行包是平台专属，不能复用 macOS 的）。完整步骤见 [Windows 打包与验收清单](docs/windows-release.md)。

也可以手动执行：macOS `npm run tauri:build -w @phoebe/desktop`；Windows 需先设置 `PHOEBE_GPTSOVITS_ROOT` / `PHOEBE_GPTSOVITS_ENV` 再执行同一条命令。

## 项目结构

```text
apps/desktop/                 React + Tauri 2 桌面应用
  public/pet/                 菲比立绘、动作帧与聊天背景
  src/                        桌宠、聊天、设置和动画 UI
  src-tauri/                  Rust 窗口、存储、工具、历史和语音实现
packages/agent/               Pi Agent Sidecar、人设、工具和语音文本策略
packages/shared/              桌面端与 Agent 共享协议
docs/                         实现、角色、素材和语音说明
phoebe_voice_zh/              语音元数据与打包所需的单段参考音频
```

当前菲比桌面应用的入口是 `apps/desktop`。

## 尚未完成的功能

- **语音输入**：按住说话、麦克风录音、ASR（本地 Whisper，macOS 用 mlx-whisper / Windows 用 faster-whisper 或 whisper.cpp）、流式/分句 TTS 队列、逐句表情切换、免提抢话与回声消除。
- **长视频分析（路径 C · 后续）**：当前 `analyze_video` 用 ffmpeg 裁剪时间窗后 base64 直传，尚未接入 DashScope 文件上传接口；也未做成本上限与每日调用计数。设计见 [视频分析方案](docs/phoebe-video-plan.md)。
- **发布版语音包精简**：当前语音运行包打包了整个 Conda 环境（含训练、WebUI、开发工具与未参与推理的 Python 包），未做裁剪。
- **Phase 5 辅助功能 / 截屏 / Shell**：屏幕录制、键鼠辅助功能（Accessibility / UI Automation）、受限 Shell 命令。依赖生产签名/公证后系统权限才稳定。
- **自动更新**：macOS / Windows 应用自更新。
- **自动清理旧运行包**：语音运行时更新后，旧的 4～5 GB 展开目录不会自动清理。
- **稳定显示器标识**：同型号多屏场景下重启恢复到正确屏幕。
- **Windows 发布**：Windows x64 语音包生成、NSIS 实机打包与验收（清单见 [docs/windows-release.md](docs/windows-release.md)）。
- **生产签名 / 公证**：Apple Developer ID 签名 + 公证（仅自用或朋友小范围使用可不做）。

更完整的优化点、收益与优先级见 [改进计划](docs/phoebe-improvement-plan.md)。

## ver0.9.1 已知边界

- 视频分析（`analyze_video`）会把视频上传到阿里云百炼，需先在设置里配置 DashScope API Key；长视频需用 `startSec`/`endSec` 分段分析，单次窗口≤300 秒，且未做成本上限与每日调用计数。
- 尚未完成 Apple Developer ID 签名、公证和自动更新。
- Windows 构建与 Credential Manager 需要进一步实机验收。
- 显示器热插拔/睡眠唤醒的主动位置校验已实现（2 秒轮询），但透明、穿透、拖动、位置恢复、设置窗焦点和开机启动仍需 macOS/Windows 双端实机验收；稳定显示器标识（同型号多屏场景）尚未实现。
- 首次语音展开和 CPU 模型加载较慢，并额外占用约 4～5 GB 应用数据空间；当前未提供自动清理旧运行包的界面。
- macOS 内置语音已完成真实安装包验收；Windows 管理与打包路径已实现，但仍需 Windows x64 实机生成运行包并验收。
- 定位依赖系统原生定位；macOS 首次会弹系统定位授权框，Windows 定位路径已实现但尚未实机验收。
- 联网搜索依赖外部搜索页面，可能遇到限流、验证页或网络超时。
- 尚未接入语音输入、ASR、免提抢话和回声消除。
- 已实现 Phase 1 操作权限网关、Phase 2 受控文件、Phase 3 应用/文件操作与 Phase 4 受控浏览器；系统辅助功能控制仍待后续阶段。
- 历史、记忆和 API Key 不做设备间同步。

更详细的边界与开发记录见 [菲比助手实施记录](docs/phoebe-implementation.md)。

## 声明

本项目是个人学习与本机实验用的非官方项目。角色、原作及相关素材权利归各自权利人所有；请勿将素材用于商业发行或其他未经授权的用途。
