# 菲比助手 · Phoebe Assistant

> **ver0.1** · 基于 Tauri 2、React 和 Pi Agent 的本机 AI 桌面宠物原型

<p align="center">
  <img src="apps/desktop/public/pet/phoebe-orb-avatar-v1.png" width="152" alt="菲比助手头像">
</p>

菲比助手是一个桌面常驻角色与本机 AI Agent 结合的实验项目。它使用透明置顶窗口展示可动的菲比桌宠，通过独立聊天窗口连接 DeepSeek，并在本机管理 API Key、对话历史、显式记忆和 GPT-SoVITS 语音。

ver0.1 当前以 **macOS Apple Silicon** 为主要验收环境；代码保留 Windows 适配，但尚未完成 Windows 实机验收。

## 已实现功能

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
- DeepSeek 流式文字对话、Markdown 显示、中止生成和错误恢复。
- 已接入菲比人设系统提示，同时保留真实性、工具权限和不伪造结果的边界。
- **助手模式**：适合任务、资料和工具结果，屏幕保留完整回答。
- **聊天模式**：回答默认更简短、更自然，适合角色对话。
- 对话完成后生成 `emotion` 和 `gesture`，用于驱动桌宠反应。
- 右键菜单可查看当前轮次与本次启动的 Token 用量。

### 工具能力

- 获取当前时间与本应用状态。
- 联网搜索，返回可解析的 HTTPS 来源链接；支持可选的本机回环 HTTP 代理。
- 仅读取用户在当前轮次主动选择的小型文本文件。
- 仅在用户当前轮次主动授权时获取粗略位置。
- Rust 工具网关会校验调用者、参数和允许列表，不提供任意 Shell 执行入口。

### 本机数据与安全

- DeepSeek API Key 只保存到 macOS Keychain／Windows Credential Manager，不写入仓库、对话历史或安装包。
- 设置、窗口位置、SQLite 历史和长期记忆均位于各自设备的应用数据目录，当前不做云端同步。
- 隐私模式下不持久化本轮对话。
- 保存最近 50 轮对话，支持清空历史。
- 长期记忆由用户在设置中显式新增、修改或删除，并经过本机确认与敏感内容检查。

### 语音

- 已接入本机 GPT-SoVITS `api_v2.py` 动态中文 TTS。
- 助手模式只朗读简短结论；聊天模式完整朗读简短回答。
- 每条已完成回复都提供简单的重播按钮。
- 支持停止播放、丢弃过期合成结果，并在播放结束后删除临时 WAV。
- 安装包只携带一段推理参考音频，不包含 GPT-SoVITS 环境、训练数据或模型权重。

详细启动与权重配置见 [菲比本机动态语音](docs/phoebe-voice.md)。

## 快速开始

### 环境需求

- Node.js 22+
- npm
- Rust stable 工具链
- Tauri 2 在目标系统上的构建依赖
- DeepSeek API Key
- 可选：本机 GPT-SoVITS API，用于动态语音

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

release 构建会打包固定的 Node Sidecar、编译后的 Agent、Node 许可文件和 TTS 参考音频。

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

仓库中保留的旧 Swift/AppKit 与 Roxy 文件只作为迁移参考；当前菲比桌面应用的入口是 `apps/desktop`。

## ver0.1 已知边界

- 尚未完成 Apple Developer ID 签名、公证和自动更新。
- Windows 构建与 Credential Manager 需要进一步实机验收。
- GPT-SoVITS 推理环境和 e15/e8 权重需要用户在本机单独启动。
- 定位能力仍受 macOS WebView 与系统权限限制。
- 联网搜索依赖外部搜索页面，可能遇到限流、验证页或网络超时。
- 尚未接入语音输入、ASR、免提抢话和回声消除。
- 当前不提供任意电脑操作或无限制“全权操作”开关。
- 历史、记忆和 API Key 不做设备间同步。

更详细的边界与开发记录见 [菲比助手实施记录](docs/phoebe-implementation.md)。

## 声明

本项目是个人学习与本机实验用的非官方项目。角色、原作及相关素材权利归各自权利人所有；请勿将素材用于商业发行或其他未经授权的用途。
