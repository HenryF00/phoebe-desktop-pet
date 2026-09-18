# 菲比助手 · 视频分析（路径 C）设计方案

> 目标：让 Agent 能分析用户授权的**长视频**（例如 FPS 对枪），做法是**本地抽帧 + 按需取图 + 分段预算控制**，
> 而不是把整段视频交给模型。DeepSeek 视觉模型是 image-only（`input: ["text","image"]`），
> 且单张图约 1k+ token、历史预算 24k，因此「一次看完整段视频」在设计上不可行。
>
> 本文档描述在现有「Rust 权限网关 + Node Sidecar」架构内实现该能力的方案。

## 1. 背景与约束

| 约束 | 说明 |
|---|---|
| 模型输入 | DeepSeek `deepseek-v4-flash-vision-exp` 只接受文本 + 图片，**无原生视频** |
| Token 预算 | 历史预算约 24k token，单图约 1k+；一次最多约 15–20 张图 |
| 视频时长 | 一场对局 20–40 分钟；1fps 也是上千帧，必须**分段 + 抽样** |
| 时间分辨率 | 对枪要看准星移动，需要 30–60fps 的**局部窗口**，不能全局高 fps |
| 进程边界 | Node Sidecar 不执行系统操作；所有执行由 Rust 网关完成 |
| ffmpeg | 系统 `/opt/homebrew/bin/ffmpeg` 可用；语音运行时内也带了 `python/bin/ffmpeg` |

**设计原则**：Rust 跑 ffmpeg；每个工具都有硬上限（帧数/分辨率/时长/字节）；只读；走现有审批与路径约束；
图片通过扩展后的 `tool_result` 回传，Sidecar 再作为 `ImageContent` 注入对话。

## 2. 总体流程

```
用户：分析 ~/Movies/match.mp4 里 12:30–12:40 的对枪
  │
  ▼
Pi Agent ── tool_request(video_probe) ──► Rust ToolBroker ── ffprobe ──► 元数据(文本)
  │
  ├─ tool_request(video_frames{mode:"sheet", start:750, end:780, fps:1}) ─► ffmpeg 抽帧+拼图
  │        └─ 返回 1 张网格图（如 5×5=25 帧）→ Sidecar 注入 ImageContent
  │
  ├─ 模型判断“交火在 762–766s”，再请求
  │   tool_request(video_frames{mode:"frames", start:762, end:766, fps:8, region:{...}}) ─► 8 张裁剪帧
  │
  └─ 汇总讲解 / 给训练建议
```

关键点：**抽样策略由模型自己决定**（先粗后细、按需放大），Rust 只负责安全地执行并限流。

## 3. 工具面（Rust `ToolBroker` + Node `brokerTool`）

### 3.1 `video_probe`
- 参数：`{ grantId, relativePath }`
- 行为：`ffprobe -v error -print_format json -show_format -show_streams`
- 返回（文本）：时长、分辨率、帧率、编码、码率、文件大小
- 策略：**auto**（只读元数据，无副作用）

### 3.2 `video_frames`
- 参数：
  ```jsonc
  {
    "grantId": "…", "relativePath": "match.mp4",
    "startSec": 750, "endSec": 780,
    "fps": 1,                       // 1..60，默认 1
    "mode": "sheet",                // "sheet" | "frames"
    "grid": "5x5",                  // 仅 sheet：1x1..8x8，默认 5x5
    "scaleWidth": 480,              // 每帧缩放宽度 160..1280，默认 480
    "region": { "x": 0.35, "y": 0.3, "w": 0.3, "h": 0.3 }  // 可选的相对裁剪区域
  }
  ```
- 行为：
  - `mode:"sheet"`：`-vf "fps=F,scale=W:-1,crop…,tile=gx gi"` → **1 张网格图**
  - `mode:"frames"`：抽 `maxFrames` 张独立图（≤ 8）
- 返回：文本（时间范围、帧数、覆盖时间点）+ 图片内容
- 策略：**标准模式逐次确认**（帧会发给 DeepSeek，属于数据外发）；信任模式可按 `folder:<grantId>` 免重复确认

### 3.3（Phase 2）`video_region_zoom`
- 在 `video_frames` 的 `region` 基础上，支持先粗后精的局部高 fps 抽帧，便于看清准星/敌人。
- 也可不新增工具，把 `region` 留在 `video_frames` 里（推荐）。

### 3.4 无状态
ffmpeg 是一次性进程，无需像浏览器那样维护会话/关闭工具。

## 4. 协议扩展：`tool_result` 支持图片

现状：`tool_result.content` 只有 `{type:"text"}`。Pi 的 `AgentToolResult.content` 本就支持
`(TextContent | ImageContent)[]`，因此只需让 Rust→Node 的 `tool_result` 也携带图片。

**新线格式**（向后兼容，纯文本时不变）：
```jsonc
{
  "type": "tool_result", "requestId": "…", "status": "success",
  "content": [
    { "type": "text",  "text": "～762–766s，8 帧，裁剪中心区域" },
    { "type": "image", "mimeType": "image/jpeg", "data": "<base64>" }
  ],
  "details": { "frames": 8, "startSec": 762, "endSec": 766 },
  "isError": false
}
```

改动点：
- `packages/agent/src/protocol.mjs`：`tool_result` 解析允许 `image` 分片（校验 mimeType ∈ png/jpeg/webp、
  分片数 ≤ 12、单片 base64 ≤ 1.5 MB、总量 ≤ 5 MB）。
- `packages/agent/src/sidecar.mjs`：`requestTool` 原样返回 `content`；`brokerTool` 直接
  `return { content: result.content, details }`（已是数组，无需再包一层）。
- Rust `ToolOutcome`：新增 `images: Vec<(String, String)>`（mime, base64），`into_line` 组装 `content` 数组。
- 前端：工具卡片仍只显示文本；图片不会进聊天消息（属于工具结果）。

> 安全：图片只从**已授权目录**的视频生成，且只在模型请求时外发；不写入历史/磁盘（除非用户另有要求）。

## 5. Rust 侧实现

新增 `apps/desktop/src-tauri/src/tools/video.rs`：

```rust
pub struct VideoMeta { duration_sec: f64, width: u32, height: u32, fps: f64, codec: String, bytes: u64 }
pub struct FrameSpec { start: f64, end: f64, fps: f64, mode: Mode, grid: (u32,u32),
                       scale_width: u32, region: Option<Region> }
pub struct ExtractedFrames { images: Vec<(String,String)>, coverage_secs: Vec<f64>, truncated: bool }

pub fn probe(path: &Path) -> Result<VideoMeta, String>;                 // ffprobe
pub fn extract(path: &Path, spec: &FrameSpec) -> Result<ExtractedFrames, String>; // ffmpeg
```

**ffprobe**：
```
ffprobe -v error -print_format json -show_format -show_streams <path>
```

**ffmpeg 网格图**（低 token 粗扫）：
```
ffmpeg -nostdin -ss <start> -to <end> -i <path> \
  -vf "fps=<fps>,scale=<w>:-2[,crop=w=iw*<rw>:h=ih*<rh>:x=iw*<rx>:y=ih*<ry>],tile=<gx>x<gy>" \
  -frames:v 1 -f image2pipe -vcodec mjpeg -
```

**ffmpeg 独立帧**（精细窗口）：
```
ffmpeg -nostdin -ss <start> -to <end> -i <path> \
  -vf "fps=<fps>,scale=<w>:-2[,crop=…]" -frames:v <n> \
  -f image2pipe -vcodec mjpeg -   # 多帧时用 -vsync 0 并逐帧切分
```
> 实现时用 `-frames:v N -f image2pipe`，按 JPEG 魔数（`FFD8…FFD9`）切出各帧；或写临时目录再读。

**进程约束**：
- 固定程序路径：优先系统 `ffmpeg`，回退语音运行时内的 `python/bin/ffmpeg`（release 内 `BaseDirectory::Resource` 或应用数据目录）。
- `Stdio::piped()` 读 stdout；`stderr` 截断保留用于报错。
- **超时 30s**：超时杀进程返回失败。
- 输出总字节上限（如 6 MB），超限即截断并标注。

**注册与策略**（`registry.rs` / `mod.rs`）：
- `Tool::{VideoProbe, VideoFrames}`，参数 `deny_unknown_fields`。
- 仅助手模式 + 策略≠关闭时注册（与其它操作工具一致）。
- `required_grant`：复用 `folder:<grantId>`；路径经 `PathPolicy` 与相对路径约束，禁止绝对路径与符号链接逃逸。
- 上限常量：`MAX_VIDEO_DURATION_PER_CALL`（如 120s）、`MAX_FRAMES`（sheet 用 tile 上限、frames ≤ 8）、
  `MAX_REGION_FPS`（60）、`MAX_SCALE_WIDTH`（1280）。

## 6. 预算与防爆

| 层级 | 限制 |
|---|---|
| 单次调用 | 时间跨度 ≤ 120s；独立帧 ≤ 8；网格 ≤ 8×8；分辨率宽度 ≤ 1280；总字节 ≤ 6 MB；超时 30s |
| 每轮对话 | Rust `ToolBroker` 记录本轮已返回图片数（如 ≤ 60 张），超限返回失败并提示“请缩小窗口或降低 fps” |
| 历史预算 | 图片属于工具结果，会进入上下文；配合现有 `pruneMessages` 裁剪与工具结果截断 |
| 提示 | 工具描述里告知模型这些上限，引导“先 sheet 后 frames、必要时裁剪” |

## 7. 提示与人设集成

工具描述给模型一套建议流程：
1. `video_probe` 拿时长/分辨率。
2. 用 `video_frames(mode:"sheet", fps:1)` 粗扫整段或用户给的范围。
3. 定位到具体窗口后，用 `video_frames(mode:"frames", fps:4–10, region:屏幕中心)` 细看。
4. 需要更强分析时，说明可请求更窄窗口 + 更高 fps（受上限约束）。

## 8. 安全边界

- 只读；绝不写文件、绝不改视频。
- 路径来自**已授权目录的相对路径**，绝对路径不进入模型上下文（沿用 Phase 2 设计）。
- 屏蔽敏感目录（`PathPolicy`）。
- ffmpeg 参数全部由 Rust 用**数值 clamp** 生成，无 shell、无表达式注入；`region` 为相对比例并做 0..1 边界校验。
- 图片仅在模型调用时外发到 DeepSeek；不落盘、不进聊天历史（除非用户明确要求保存）。

## 9. 分阶段落地

| 阶段 | 内容 | 交付 |
|---|---|---|
| **P1** | 协议 `tool_result` 图片支持；`video_probe` + `video_frames(mode:"sheet")`；预算上限 | 能"粗看"长视频 |
| **P2** | `mode:"frames"` 独立帧 + `region` 裁剪；每轮图片预算 | 能"细看"窗口/局部 |
| **P3** | 可选本地 CV：准星/目标检测 → 逐帧指标（准星-目标距离、甩枪速度、过冲），把**指标 + 少量标注图**交给模型讲解 | 对枪量化分析（路径 B 融合） |

## 10. 测试与验收

- **Rust 单测**：参数 clamp/校验、路径约束、`region` 边界、`ToolOutcome` 组装含图片、超时逻辑（可用假进程/超短 fixture）。
- **集成冒烟**：用 ffmpeg 生成一个 2 秒测试视频，断言 `extract` 返回预期帧数/尺寸。
- **Node 测试**：`tool_result` 含图片的解析与上限拒绝。
- **实机验收**：授权一个含视频的目录 → 让 Agent「分析某段对枪」→ 观察是否按 probe→sheet→frames 迭代、图片是否送达模型、上下文是否受控。

## 11. 待拍板

1. **审批粒度**：`video_probe` auto、`video_frames` 逐次确认（推荐）；或全部 auto。
2. **默认上限**：单次时间跨度（120s）、独立帧数（8）、每轮图片数（60）。
3. **ffmpeg 来源**：仅系统 ffmpeg，还是打包内置一份（更稳，但 +几十 MB）。
4. **是否做 P3 本地 CV**（对枪量化的关键，工作量大）。

---

参见：[改进计划](phoebe-improvement-plan.md) · [实施记录](phoebe-implementation.md) · [Windows 打包与验收清单](windows-release.md)
