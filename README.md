# 菲比助手 · Phoebe Assistant

> **ver0.3** · 基于 Tauri 2、React 和 Pi Agent 的本机 AI 桌面宠物与受控本机操作 Agent

<p align="center">
  <img src="apps/desktop/public/pet/phoebe-orb-avatar-v1.png" width="152" alt="菲比助手头像">
</p>

菲比助手是一个桌面常驻角色与本机 AI Agent 结合的实验项目。它使用透明置顶窗口展示可动的菲比桌宠，通过独立聊天窗口连接 DeepSeek，并在本机管理 API Key、对话历史、显式记忆和 GPT-SoVITS 语音。

从 ver0.3 起，菲比不再只是聊天：所有操作系统操作统一经过 Rust 权限网关（Node Sidecar 只提交请求，Rust 独立校验并执行），可以在用户授权目录内受控地读取、写入、移动和删除文件，并加入了按 Token 预算的上下文管理，避免长时间会话把上下文喂爆。

ver0.3 当前以 **macOS Apple Silicon** 为主要验收环境；代码保留 Windows 适配，但尚未完成 Windows 实机验收。

## 已实现功能

### ver0.3 新增

- **操作权限网关（Phase 1）**：Node↔Rust 双向工具协议、逐次审批、操作策略、审计与急停。
- **受控文件（Phase 2）**：授权目录 + 相对路径的只读与写入/移动/删除；绝对路径从不进入模型上下文。
- **上下文预算管理**：CJK 感知的 Token 裁剪，每次模型请求前（含同一轮内的多次工具调用）都会执行，工具结果按需分页。
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
- 上下文按 Token 预算管理：每次请求前（含同一轮内的多次工具调用）裁剪历史，先丢弃旧的工具输出、再考虑对话文本，并对过大的工具输出做截断；单次文件读取默认 16 KB、可按 `offset` 分页，避免累积 Tool 结果把上下文喂爆。
- 右键菜单可查看当前轮次与本次启动的 Token 用量。

### 工具能力

- 获取当前时间与本应用状态。
- 联网搜索，返回可解析的 HTTPS 来源链接；支持可选的本机回环 HTTP 代理。
- 仅读取用户在当前轮次主动选择的小型文本文件。
- 仅在用户当前轮次主动授权时获取粗略位置。
- 经用户确认后，用系统默认浏览器打开单个 HTTPS 链接，并支持“始终允许该网站”。
- **受控文件（Phase 2a/2b）**：授权目录 + 相对路径。只读工具 `list_granted_folders`、`list_directory`、`read_text_file`、`search_files`；写入工具 `write_file`、`move_file`、`delete_file` 仅在用户选择“可读写”授权后可用，且**每次写入或删除都必须逐次确认并展示 diff**。Rust 会规范化路径并拒绝绝对路径、`..`、符号链接逃逸与符号链接写入，屏蔽 `.ssh`、钥匙串、`.env` 等敏感位置，限制文件大小与搜索范围；写入采用临时文件 + rename 的原子方式，删除默认移入系统废纸篓。绝对路径从不进入模型上下文，也不会出现在工具结果里。
- **操作权限网关（Phase 1）**：所有操作系统操作由 Rust `ToolBroker` 统一处理。Node Sidecar 只提交 `tool_request`，Rust 独立校验每轮白名单、参数 Schema 与操作策略，执行后把 `tool_result` 写回；Sidecar 无法自行执行或批准操作。
- 操作策略提供“关闭／标准／信任”三档，默认“标准”逐次确认；聊天模式不注册操作工具。审批以聊天窗模态为主、Rust 原生对话框兜底。
- 每次执行或拒绝都会写入本机 `audit.jsonl`，并可从聊天窗或设置页立即停止所有操作。
- Rust 工具网关会校验调用者、参数和允许列表，不提供任意 Shell 执行入口。

### 本机数据与安全

- DeepSeek API Key 只保存到 macOS Keychain／Windows Credential Manager，不写入仓库、对话历史或安装包。
- 设置、窗口位置、SQLite 历史和长期记忆均位于各自设备的应用数据目录，当前不做云端同步。
- 操作审计写入应用数据目录的 `audit.jsonl`；“信任”授权保存在 `grants.json`，只包含用户明确“始终允许”的网站域名。
- 隐私模式下不持久化本轮对话。
- 保存最近 50 轮对话，支持清空历史。
- 长期记忆由用户在设置中显式新增、修改或删除，并经过本机确认与敏感内容检查。

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

### 安装依赖

```sh
npm ci
```

### 启动桌面应用

```sh
npm run desktop:dev
```

启动后，右键菲比 → **打开设置** → 保存 DeepSeek API Key，再单击桌宠打开聊天。

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

## ver0.3 已知边界

- 尚未完成 Apple Developer ID 签名、公证和自动更新。
- Windows 构建与 Credential Manager 需要进一步实机验收。
- 首次语音展开和 CPU 模型加载较慢，并额外占用约 4～5 GB 应用数据空间；当前未提供自动清理旧运行包的界面。
- macOS 内置语音已完成真实安装包验收；Windows 管理与打包路径已实现，但仍需 Windows x64 实机生成运行包并验收。
- 定位能力仍受 macOS WebView 与系统权限限制。
- 联网搜索依赖外部搜索页面，可能遇到限流、验证页或网络超时。
- 尚未接入语音输入、ASR、免提抢话和回声消除。
- 已实现 Phase 1 操作权限网关与 Phase 2 受控文件（只读 + 写入/移动/删除）；应用启动与浏览器操控仍待后续阶段。
- 历史、记忆和 API Key 不做设备间同步。

更详细的边界与开发记录见 [菲比助手实施记录](docs/phoebe-implementation.md)。

## 声明

本项目是个人学习与本机实验用的非官方项目。角色、原作及相关素材权利归各自权利人所有；请勿将素材用于商业发行或其他未经授权的用途。
