# 菲比助手 · 改进计划

> 本文档汇总当前版本（ver0.9）审查出的优化点与后续计划，按「收益 / 风险」排序。
> 完成一项后在对应条目下标注日期与结果；详细实现见 [菲比助手实施记录](phoebe-implementation.md)。

## 现状快照

打包产物 `.app` 约 **2.5 GB**，构成：

| 部分 | 体积 | 说明 |
|---|---|---|
| `Contents/Resources/voice-runtime` | **2.3 GB** | GPT-SoVITS 离线运行包（整个 Conda 环境 + 基础模型 + e15/e8 权重） |
| `Contents/MacOS/phoebe-agent-node` | 138 MB | 打包的 Node 22 运行时 |
| `Contents/MacOS/phoebe-desktop` | 47 MB | Rust 主程序（含内嵌前端） |
| `apps/desktop/public/pet` | 30 MB | 立绘/动作帧（PNG 27 MB + JPG 2.5 MB） |
| `packages/agent` bundle | ~6 MB | Agent Sidecar 2.0 MB + 浏览器 Sidecar 4.2 MB |

源码规模：Rust ~9.2k 行（`registry.rs` 1907、`voice.rs` 1152、`history.rs` 1088、`lib.rs` 957、`agent.rs` 957、`window_manager.rs` 744）；前端 `main.tsx` ~1600 行；Agent ~1.4k 行。

---

## P0 · 低风险、立刻见效

### P0-1 删除未引用的宠物素材

- **问题**：`apps/desktop/public/pet` 下有 7 个已无任何引用的文件（约 **10 MB**），会被 Vite 原样打进前端并嵌入主程序。
- **清单**：`phoebe-orb-avatar-v1.png`（已被 v2 取代）、`phoebe-assistant-blink-v1.png`、`phoebe-chat-blink-v1.png`、`phoebe-blink-prototype.png`、`phoebe-full-prototype.png`、`phoebe-wave-prototype.png`、`phoebe-portrait-prototype.png`。
- **方案**：确认无引用后删除（引用来源：`apps/desktop/src/**`、`public/pet/animations.json`）。
- **收益**：-10 MB，零风险。 **风险**：无。

### P0-2 宠物立绘转 WebP

- **问题**：`public/pet` 的 PNG 共 27 MB，均为带透明通道的立绘/动作帧。
- **方案**：新增构建前脚本把用到的 PNG 转 WebP（保留 alpha），更新 `animations.json` 与 `main.tsx`/`style.css` 的引用。
- **收益**：约 -12~15 MB。 **风险**：低（WebView 支持 WebP；透明与画质需目视确认）。

### P0-3 会话摘要固定用便宜模型

- **问题**：`packages/agent/src/summary.mjs` 复用当前对话模型；用户选 `deepseek-v4-pro` 时摘要也用 pro，成本翻倍。
- **方案**：摘要固定用 `deepseek-v4-flash`（或按 `PHOEBE_SUMMARY_MODEL` 覆盖）。
- **收益**：省钱、延迟更低。 **风险**：无（摘要质量对 flash 足够）。

---

## P1 · 收益大、需验证

### P1-1 语音运行时裁剪（对应「待办 11」）

- **问题**：`prepare-voice-runtime.py` 用 `conda_pack.pack()` 打包**整个** Conda 环境，包含训练脚本、WebUI(gradio)、tensorboard、jupyter、pytest、CUDA 相关等与推理无关的包。2.3 GB。
- **方案**：
  1. 新建推理专用依赖锁 `config/voice-inference-requirements.lock`，只保留 `api_v2.py` 推理链所需（torch/torchaudio CPU、numpy、scipy、librosa、soundfile、transformers、jieba、pypinyin 等）。
  2. 从零建最小环境再 `conda-pack`（而非打包现有大环境）。
  3. 打包后用 `archive_filter` 再剔除 `tests/`、`__pycache__`、示例数据、缓存。
  4. **必须冒烟校验**：启动 `api_v2.py` → 调 `/tts` → 校验返回有效 WAV（防止运行时动态 import 被裁掉）。
- **收益**：目标 **1~1.5 GB**（省 ~1 GB），首启展开更快。 **风险**：中（动态 import / 隐式依赖）。

### P1-2 语音包重建防护与日志

- **问题**：`prepare-release.mjs` 若判定“未就绪”会**静默重建**数 GB 的语音包（2026-09-18 发生过一次，导致客户端重新展开 4.4 GB）。
- **方案**：打印「复用 / 重建」及其原因；非显式 `PHOEBE_REBUILD_VOICE_RUNTIME=1` 时，若归档已存在且清单平台/架构匹配则拒绝重建。
- **收益**：避免误触发全量重展开与重复打包。 **风险**：低。

---

## P2 · 工程 / 发布

### P2-1 Windows NSIS 打包与实机验收
- 见 [Windows 打包与验收清单](windows-release.md)。需 Windows x64 机器生成语音包 + NSIS + 实机验收。

### P2-2 自动更新
- macOS / Windows 应用自更新（依赖签名/公证与更新源）。

### P2-3 生产签名 / 公证
- Apple Developer ID 签名 + 公证；否则 Gatekeeper 拦截、TCC 权限（定位/辅助功能/屏幕录制）不稳定。

### P2-4 CI 质量门
- `cargo clippy` + `cargo fmt --check` + 前端 lint（当前 CI 只有 test/build）。

### P2-5 拆分大文件
- `tools/registry.rs`（1907 行）按工具类别拆分；`lib.rs` 的命令按模块拆分；`main.tsx` 拆成 `ChatApp/SettingsApp/PetApp/QuickMenu` 等组件。

### P2-6 `cleanup_stale_runtimes` 精确匹配
- 现在按 `v*-*` 前缀删除，偏宽；改为解析 `runtime_id` 格式（`v{format}-{platform}-{arch}-{hash12}-{hash12}`）后再删。

---

## P3 · 功能完善（尚未实现）

详见 README「尚未完成的功能」。摘要：

- **语音输入**：按住说话、麦克风录音、ASR（macOS 用 mlx-whisper / Windows 用 faster-whisper 或 whisper.cpp）。
- **流式 / 分句 TTS 队列** 与 **逐句表情切换**；免提抢话、回声消除。
- **稳定显示器标识**：同型号多屏场景下重启恢复到正确屏幕。
- **Q 版形象**支持（优先级低）。
- **Phase 5**：辅助功能 / 截屏 / Shell（需签名后系统权限才稳定）。

---

## 建议执行顺序

1. **P0-1 / P0-2 / P0-3**（低风险，一次打包验证）。
2. **P1-2**（语音包重建防护，防止后续构建踩坑）。
3. **P1-1**（语音运行时裁剪，单独一轮 + 冒烟校验）。
4. **P2-4 / P2-5 / P2-6**（工程质量）。
5. **P3 语音输入**（体验价值最高）→ **P2-1/P2-3**（发布）→ **P3 Phase 5**。

## 验收方式

- 每次改动后：`cargo test`、`npm test`、`npm run build`、agent esbuild bundle 全绿。
- 涉及打包：重新 `tauri build --bundles app`，核对版本、体积、包内资源，并启动确认。
- 涉及语音：确认 `api_v2.py` 能起、能合成有效 WAV，并检查 `logs/gpt-sovits.log`。
