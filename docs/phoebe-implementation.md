# 菲比助手实施记录

当前项目使用 Tauri 2 工作区，桌面端入口为 `apps/desktop`。

## 当前已落地

- npm 与 Cargo workspace。
- Tauri 2 透明、无边框、置顶的 PetWindow，独立可聚焦且可隐藏的 ChatWindow，以及可调整大小、滚动和键盘操作的 SettingsWindow；React 占位界面只用于结构验收，不代表角色立绘。双击桌宠、快捷菜单和托盘均可打开设置。
- Rust WindowManager 对桌宠拖动做当前显示器工作区边界限制，聊天窗随桌宠定位；托盘提供恢复桌宠、打开聊天和退出入口。拖动后在应用数据目录保存显示器名称与尺寸、缩放、角色逻辑高度及归一化坐标，重启时按工作区恢复；没有匹配显示器时回退主屏。聊天窗隐藏或系统关闭请求不销毁 WebView，也不会取消仍在运行的对话。
- 共享的 AssistantEvent、ToolCall、ApprovalRequest、VoiceRequest 类型。
- Rust 只读工具网关示例：当前时间与本应用状态。其他工具不会被前端调用，也没有任意命令入口。
- Phase 1 操作权限网关已落地：Node Pi Sidecar 通过 JSONL 双向协议提交 `tool_request`，Rust `ToolBroker` 独立校验本轮白名单、`deny_unknown_fields` 参数 Schema 与操作策略，用聊天窗 React 模态（Rust 原生对话框兜底）取得用户确认，执行后回写 `tool_result`；Sidecar 无法自行执行或批准操作。`get_current_time`、`get_system_status` 与新增 `open_url` 均经由该网关，`open_url` 只接受 HTTPS、禁止内嵌凭据，信任模式可对单个域名“始终允许”。操作策略三档（关闭／标准／信任）默认标准，聊天模式不注册会产生本机副作用的操作工具（只读的 `list_granted_folders` 在两种模式下均注册）；每次执行或拒绝写入应用数据目录的 `audit.jsonl`，并提供 `panic_stop_operations` 急停。原先未被前端调用的 `run_read_only_tool` 死命令已移除。
- Phase 2a 受控文件只读已落地：用户通过原生目录选择器授权文件夹，Rust 生成不透明 `grantId` 并将 `path` 保留在本地，模型只能看到 `grantId`、标签与权限。新增 `list_granted_folders`、`list_directory`、`read_text_file`、`search_files`：Rust 对相对路径做规范化与包含性检查，拒绝绝对路径、`..`、根目录前缀与符号链接逃逸，屏蔽 `.ssh`、`.aws`、钥匙串、`.env` 等敏感位置，并限制读取大小（≤512KB）、目录条目（≤500）与搜索范围。文件夹授权是前置条件：未授权目录直接拒绝；标准模式每次读取逐次确认，信任模式对已授权目录免重复确认。目录授权保存在 `grants.json`，设置页可查看与撤销。
- Phase 2b 受控写入已落地：新增 `write_file`、`move_file`、`delete_file`，仅对用户选择“可读写”授权的文件夹生效，且三类工具**无论策略如何都强制逐次确认**。写入在审批模态展示基于行差异的 diff，执行时先写同目录临时文件再 rename 原子替换；目标为符号链接时直接拒绝，`createOnly` 阻止覆盖；移动限定在同一授权目录内且拒绝覆盖现有目标；删除默认移动到系统废纸篓（macOS `~/.Trash`，其他平台回退到应用数据目录的恢复区），跨卷时返回明确错误。模型侧工具结果只包含授权标签与相对路径。
- Phase 3 应用/文件操作已落地：`apps.rs` 有界扫描 `/Applications`、`/System/Applications`、`~/Applications`（不跟随符号链接、不进入 `.app` 内部，结果缓存 10 分钟），为每个应用生成基于路径哈希的稳定不透明 `appId`。新增 `list_installed_apps`（只读、两模式均注册）、`launch_application`、`focus_application`（仅接受枚举到的 `appId`，绝不接受路径，信任键 `app:<appId>`）、`reveal_file` 与 `open_file_with_application`（仅在已授权目录内，复用 relative path 越权检查，信任键 `folder:<grantId>`）。应用信任键与域名/目录授权共用 `grants.json`。macOS 使用 `/usr/bin/open` 与参数数组（无 shell），Windows 走 `cmd /C start` / `explorer /select,` / 直接执行，Linux 回退 `xdg-open`。
- Phase 4 受控浏览器已落地：独立 Browser Sidecar（`packages/agent/src/browser-sidecar.mjs`）用 playwright-core 驱动本机 Chrome/Edge 的**无头实例**（独立临时 user-data-dir，不触碰登录会话与历史）。Rust 侧 `browser.rs` 的 `BrowserService` 像 TTS/Agent 一样管理固定子进程，通过 JSONL 逐条命令/结果往返（90 秒超时）。八个工具走同一审批网关：`browser_open`（只接受 HTTPS、逐次确认、信任键 `domain:<host>`）、`browser_snapshot`（给可交互元素打 `data-phoebe-ref` 生成 `e0/e1/…` 引用）、`browser_click`/`browser_type`/`browser_select`（按 ref 定位）、`browser_wait`/`browser_extract_text`/`browser_close`；打开页面之后的页面内操作视为已授权（auto）。发布包用 esbuild 把 playwright-core 打成 `phoebe-browser.cjs`，并在 bundle banner 里 patch `require.resolve` 让它找到合成 package.json。
- 上下文预算管理已落地：历史上按 `messages.slice(-16)` 的条数截断改为 CJK 感知的 Token 预算裁剪（`packages/agent/src/context.mjs`，默认 24000 Token）。Pi 自带的 `estimateTokens` 用 `chars/4`，对中文严重低估，因此改为分开估算中文字符与拉丁字符，并按预算从新到旧保留、截断过大的历史工具输出、丢弃开头的孤立 toolResult。裁剪通过 `AgentOptions.transformContext` 挂接，因此**每次模型请求前（包括同一轮内多次工具调用之间）都会执行**，而不只是每轮一次。裁剪分两层：先丢弃最旧的工具调用及其结果（体积大、一次性用途），保留 user/assistant 对话文本；仍超预算才丢最旧的完整消息。同时把 `read_text_file` 的单次返回上限从 512KB 降到默认 16KB/最多 32KB，并在工具结果里返回字节范围与 `offset` 提示，模型可分页继续读取。
- 失败分类已落地（`packages/agent/src/errors.mjs`）：不再把所有失败统一报成“模型请求失败”，而是结合 Pi 的 `isContextOverflow` 与错误文本，区分上下文溢出、鉴权失败、余额/配额不足、限流/繁忙、网络错误，并把截断后的原始错误一并显示，便于定位。
- 上下文占用可观测：sidecar 在裁剪后上报 `context_usage` 事件（估算 Token 与预算），Rust 存入 `ContextUsageSnapshot` 并转发 `context-usage-changed`，右键快捷菜单在 Token 卡片下显示“上下文 x / 24k”与占用进度条，超预算时标红。
- Pi Agent Core 独立 Sidecar 的 JSON Lines 协议与 DeepSeek 对话适配，包含流式文本、取消和三项本轮工具：免独立搜索密钥的 `web_search`、仅读取用户本轮选择的短文本文件、通过操作系统原生定位一次性取得城市级粗略位置。Rust 开发模式按需启动固定脚本，release 模式只启动与应用同行的固定 Node 运行时和打包资源中的编译后 Agent，过滤后的内部事件才送到 UI。构建前会做无网络的 Agent 状态自检，打包时附带 Node 运行时许可文件。
- DeepSeek Key 保存在新应用专用的 macOS Keychain/Windows Credential Manager 条目；开发模式仍可回退到进程环境变量。设置窗口现在有安全输入框，可新增或替换密钥，Rust 只允许 SettingsWindow 调用保存命令；WebView 没有读取或回显命令。密钥不写入设置文件、SQLite、安装包或仓库。桌面核心首次读取后会仅在本次进程内缓存密钥，状态检查和后续每轮对话不再反复访问钥匙串；退出应用即清除内存缓存。未签名开发包重新构建后仍可能被 macOS 当作新的访问者，稳定免提示需要正式代码签名并在系统提示中授予长期访问。
- 设置文件在应用数据目录，只保存允许的 DeepSeek 文字模型、隐私模式，以及可选的本机回环 HTTP 搜索代理地址，不含密钥。模型或代理切换从下一轮对话起由 Rust 注入固定 Pi Sidecar；未完成、取消及隐私模式下的对话仅留在内存。开机启动由 Rust 侧官方插件管理、默认关闭；当前 `bundle.active=false`，调试版与未打包发布版均禁用登录项注册，正式安装包仍需两端实机验收。
- 本机 SQLite 数据库目前迁移到版本 4，包含 `conversations`、`messages`、`memories`、`app_meta`、`schema_migrations`；从版本 1 升级保留原有消息，版本 2 的 `desktop-main` 自动成为助手模式默认会话。`conversations` 新增 `mode` 列（assistant/chat），激活会话按模式存入 `app_meta`（`active_conversation:assistant` / `active_conversation:chat`）。Rust 在 Pi 真正完成文字回复时原子保存一对消息，重复事件不生成重复条目，失败或取消轮次仍只留在内存。只恢复每个会话最近 50 轮历史。隐私模式会在开始与完成时双重核验，两次中任意一次为隐私则不持久化。数据库打不开不阻断对话，但会提示历史不可用。
- 多会话：会话按交互模式隔离，各自支持新建／切换／重命名／删除，标题自动取首轮提问（可手动修改）；切换时 Rust 通过 `conversation` 命令把该会话的 user/assistant 文本重新灌回 Sidecar 的 `agent.state.messages`（经 `pruneMessages` 预算裁剪），工具调用与结果不持久化。`AgentSupervisor` 记录当前已灌入的 `(模式, 会话)`，模式或会话变化时在下一轮 `prompt` 前重灌；回复按发起时的模式保存（`ActiveRun.mode`），切换模式需 Agent 空闲。“清理聊天历史”只清理当前模式的当前会话，删除会话需 Rust 原生确认框确认。
- 设置窗提供用户显式长期记忆的查看、新增、编辑和删除。Rust 验证容量与敏感词，每次新增、修改和删除都使用绑定 SettingsWindow 的原生确认框；模型与聊天窗口没有直接写入记忆的命令。下一轮 Agent 只接收最多 20 条已保存偏好作为数据，Pi Sidecar 内存会话限制在最近 16 条消息，不赋予工具权限。记忆读取失败时退化为无记忆对话。ver0.8 起新增 `remember_preference` / `forget_preference`：模型可提议保存（按标题去重）或删除（按标题匹配）偏好，但逐次确认且经敏感词校验。
- 会话摘要已落地（`packages/agent/src/summary.mjs`）：历史 Token 超过阈值（默认 18000）且消息足够多时，先调用模型生成结构化检查点摘要（目标/约束/已完成/进行中/关键决定/下一步/关键上下文），保留摘要 + 最近 40% 消息；后续再超时把新消息并入旧摘要（增量更新）。摘要作为带标记的 user 消息注入（默认 convertToLlm 保留），摘要失败时回退到 `pruneMessages` 的丢弃行为。
- 菲比中文动态 TTS 已接入 Rust 后端。发布包内置目标平台的可迁移 Python 环境、GPT-SoVITS 推理代码、基础模型、GPT e15、SoVITS e8 与 `021_自我介绍.wav`。`VoiceService` 先检测固定的 `127.0.0.1:9880`：已有健康服务则复用，否则校验 SHA-256 与平台清单、首次展开到应用数据目录、执行 `conda-unpack` 并启动直属子进程；正常退出只关闭自己启动的服务。收到 `completed.reply.speech_text` 后生成 WAV，经格式与大小验证再播放，结束后删除。助手模式只朗读精简摘要，聊天模式完整朗读简短对话；每条已完成回复均可手动重播。详细构建和资源边界见 [菲比本机动态语音](phoebe-voice.md)。
- 桌宠右键快捷菜单提供互斥的“助手模式／聊天模式”和独立的语音开关。两种模式现在都使用透明全身逐帧素材，不再显示聊天海报卡片。动画由 `public/pet/animations.json` 驱动：助手模式偏正式待机，聊天模式使用更亲近的合手待机，并提供眨眼、思考、说明、点头、开心、关切与挥手状态。Agent 在完整回复时按文字与工具调用生成 `emotion/gesture`，桌宠在 GPT-SoVITS 真正进入 `speaking` 时使用同一提示切换动作；手动重播也会携带原回复动作提示。无持杖动作不显示权杖，挥手动作由另一只手明确持有一根连续权杖。

## 接下来的开发顺序

1. 显示器热插拔及睡眠唤醒的主动校验已落地（`window_manager::watch_display_changes`，每 2 秒轮询桌宠是否越界工作区，越界则重新 clamp 并重锚聊天窗/菜单）；稳定显示器标识与双端实机检查（透明、穿透、拖动、位置恢复、设置窗焦点、开机启动）仍待验收。
2. 完成 Windows x64 语音运行包生成与 NSIS 实机验收、生产签名／公证、自动更新和依赖许可证审查。macOS Apple Silicon 的内置语音启动、合成与进程退出已完成真实 `.app` 验收。
3. 会话摘要、显式记忆的 Agent 工具授权链、首次确认且绑定目标的鸣潮工具。长期记忆的用户手动管理已接入，Agent 工具授权链（`remember_preference` / `forget_preference`）、鸣潮工具与会话摘要均已接入；仍需真实 DeepSeek 回复与重启恢复的实机验证。
4. 按住说话、麦克风录音、ASR 与流式/分句 TTS 队列。完整回复后的动态 TTS、停止播放、整轮语音动作同步和内置推理服务已接入；当前尚未实现逐句表情切换。

目前没有 WebView API Key 读取、Agent 假回复能力。Phase 1 权限网关、Phase 2 受控文件、Phase 3 应用/文件操作与 Phase 4 受控浏览器已经打通，系统辅助功能控制将在后续阶段接入同一网关。操作策略只决定应用内是否逐次确认，不能绕过 macOS 辅助功能、屏幕录制、文件与钥匙串等系统权限，因此仍不提供绕过系统权限的“全权操作”开关。未来接入电脑控制时可增加标准／全权两级本机策略，但该策略只决定应用内部是否逐次确认，不能绕过 macOS 辅助功能、屏幕录制、文件与钥匙串等系统权限。正式构建不依赖开发目录中的 Node 脚本，也不从系统 PATH 查找 Node。当前 macOS `.app` 是未签名的本机验收包；已真实运行 DeepSeek 文字对话并观察到 Pi 调用 `web_search`。本机从 Finder 启动时不继承终端代理环境，直连 Jina Reader 超时；经用户明确同意，已在本机应用数据目录配置 `http://127.0.0.1:7897`，并在真实桌宠窗口得到搜索结果和可引用网址。公开搜索服务可能限流、缓存或返回验证页；工具只返回可解析的 HTTPS 链接，不绕过验证。文件原生选择与短文本附加/移除已在真实窗口验证，Pi 文件工具已通过离线测试；定位已改为系统原生获取（macOS CoreLocation），不再依赖 WebView 定位，首次会弹系统定位授权框，Windows 定位路径已实现但尚未实机验收。Windows 包亦尚未实机验收。
