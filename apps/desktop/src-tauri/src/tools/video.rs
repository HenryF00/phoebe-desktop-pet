//! Video analysis through DashScope's Qwen-VL-Max.
//!
//! Step 2 adds time-windowed analysis of long videos: `ffprobe` reads the
//! duration, `ffmpeg` clips the requested window to a small re-encoded temp
//! file (downscaled, no audio), and only that clip is uploaded as a base64
//! data URL. Small whole files without a window still take the direct path
//! from step 1 and never touch ffmpeg.
//!
//! Safety notes:
//! - Only the Rust gateway reads the file and the API key. The Node sidecar
//!   never sees video bytes or credentials, and never performs the upload.
//! - The video leaves the device, so this tool is approval-gated in the
//!   registry and is never registered in chat mode.
//! - Every subprocess argument is a fixed string; the only interpolated values
//!   are numeric (start/end/scale/crf) and the temp path, so there is no shell
//!   and no expression injection.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde_json::Value;

const ENDPOINT: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions";
const MODEL: &str = "qwen-vl-max";
/// Whole-file uploads take the direct base64 path only when this small.
const MAX_DIRECT_UPLOAD_BYTES: u64 = 16 * 1024 * 1024;
/// Largest single analysis window, in seconds. Long matches are analyzed one
/// window at a time.
const MAX_WINDOW_SEC: f64 = 300.0;
/// Re-encoded clip dimensions and quality, tuned to bound upload size while
/// keeping crosshair/enemy movement readable. Measured locally: ~2 MB per 60s
/// at these settings for synthetic footage; real gameplay runs a few times
/// larger but stays well under the clip cap for typical windows.
const CLIP_MAX_WIDTH: u32 = 960;
const CLIP_CRF: u32 = 30;
/// The clip itself must stay under this size before base64 (~33% more).
const CLIP_MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Cap the returned analysis so it cannot flood the DeepSeek context window.
const MAX_RESULT_BYTES: usize = 16 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const FFMPEG_TIMEOUT: Duration = Duration::from_secs(300);

/// Analyzes one video file with Qwen-VL-Max and returns its text analysis.
pub fn analyze_video(
    path: &Path,
    start_sec: Option<f64>,
    end_sec: Option<f64>,
    question: &str,
    api_key: &str,
) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "视频文件不存在或不可访问".to_owned())?;
    if !metadata.is_file() {
        return Err("目标不是文件".to_owned());
    }
    if metadata.len() == 0 {
        return Err("视频文件为空".to_owned());
    }

    // A window is needed when the caller asked for one, or when the whole file
    // is too large to upload directly. Small files without a window keep the
    // step 1 direct path and never require ffmpeg/ffprobe.
    let needs_window = start_sec.is_some() || end_sec.is_some() || metadata.len() > MAX_DIRECT_UPLOAD_BYTES;

    let upload_path: PathBuf;
    let _clip_guard: Option<TempFile>;
    if !needs_window {
        upload_path = path.to_path_buf();
        _clip_guard = None;
    } else {
        let duration = ffprobe_duration(path)?;
        let window = resolve_window(duration, start_sec, end_sec)?;
        let clip = clip_window(path, window.start, window.end)?;
        let guard = TempFile(clip.clone());
        let clip_bytes = std::fs::metadata(&clip)
            .map_err(|_| "无法读取裁剪片段".to_owned())?
            .len();
        if clip_bytes > CLIP_MAX_BYTES {
            return Err(format!(
                "裁剪片段 {} MB 仍超过上传上限 {} MB；请缩小时间范围或降低清晰度",
                clip_bytes / (1024 * 1024),
                CLIP_MAX_BYTES / (1024 * 1024)
            ));
        }
        upload_path = clip;
        _clip_guard = Some(guard);
    }

    let mime = video_mime(&upload_path).ok_or("仅支持 mp4/mov/m4v/webm/avi/mkv 视频".to_owned())?;
    let bytes = std::fs::read(&upload_path).map_err(|_| "无法读取视频文件".to_owned())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let data_url = format!("data:{mime};base64,{encoded}");

    let body = serde_json::json!({
        "model": MODEL,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "video_url", "video_url": { "url": data_url } },
                { "type": "text", "text": question }
            ]
        }]
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| "无法初始化网络客户端".to_owned())?;

    let response = client
        .post(ENDPOINT)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(describe_network_error)?;

    let status = response.status();
    let text = response.text().map_err(|_| "无法读取分析结果".to_owned())?;
    if !status.is_success() {
        return Err(format!(
            "视频分析服务返回错误（HTTP {}）：{}",
            status.as_u16(),
            extract_error(&text)
        ));
    }

    let parsed: Value = serde_json::from_str(&text).map_err(|_| "分析结果不是有效 JSON".to_owned())?;
    let content = extract_content(&parsed).ok_or("分析结果缺少内容".to_owned())?;
    Ok(truncate_result(content))
}

/// Removes the temp clip when it goes out of scope.
struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct Window {
    start: f64,
    end: f64,
}

/// Resolves and validates a time window against the probed duration.
fn resolve_window(duration: f64, start_sec: Option<f64>, end_sec: Option<f64>) -> Result<Window, String> {
    if !duration.is_finite() || duration <= 0.0 {
        return Err("无法读取视频时长".to_owned());
    }
    let whole_file = start_sec.is_none() && end_sec.is_none();
    let start = start_sec.unwrap_or(0.0).max(0.0);
    if start >= duration {
        return Err("开始时间超出视频时长".to_owned());
    }
    let end = end_sec.unwrap_or(duration).min(duration);
    if end <= start {
        return Err("结束时间必须大于开始时间".to_owned());
    }
    if end - start > MAX_WINDOW_SEC {
        if whole_file {
            return Err(format!(
                "视频总时长 {:.0} 秒超过单次分析上限（{} 秒，约 {} 分钟）；请用 startSec/endSec 指定要分析的片段",
                duration,
                MAX_WINDOW_SEC,
                MAX_WINDOW_SEC / 60.0
            ));
        }
        return Err(format!(
            "分析窗口 {:.0} 秒超过上限 {} 秒，请缩小范围",
            end - start,
            MAX_WINDOW_SEC
        ));
    }
    Ok(Window { start, end })
}

fn ffprobe_duration(path: &Path) -> Result<f64, String> {
    let ffprobe = find_tool(
        "ffprobe",
        "PHOEBE_FFPROBE",
        &["/opt/homebrew/bin/ffprobe", "/usr/local/bin/ffprobe", "/usr/bin/ffprobe"],
    )?;
    let input = path.to_str().ok_or("视频路径不是有效 UTF-8")?;
    let output = run_tool(
        &ffprobe,
        &[
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            input,
        ],
        FFMPEG_TIMEOUT,
    )?;
    output
        .trim()
        .parse::<f64>()
        .map_err(|_| "无法解析视频时长".to_owned())
}

fn clip_window(path: &Path, start: f64, end: f64) -> Result<PathBuf, String> {
    let ffmpeg = find_tool(
        "ffmpeg",
        "PHOEBE_FFMPEG",
        &["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/usr/bin/ffmpeg"],
    )?;
    let input = path.to_str().ok_or("视频路径不是有效 UTF-8")?;
    let output = std::env::temp_dir().join(format!(
        "phoebe-video-{}-{}.mp4",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let output_str = output.to_str().ok_or("临时路径不是有效 UTF-8")?;
    let start_arg = format!("{start:.3}");
    let end_arg = format!("{end:.3}");
    let scale_arg = format!("scale={CLIP_MAX_WIDTH}:-2");
    let crf_arg = CLIP_CRF.to_string();
    let args = [
        "-nostdin", "-y",
        "-ss", start_arg.as_str(),
        "-to", end_arg.as_str(),
        "-i", input,
        "-vf", scale_arg.as_str(),
        "-c:v", "libx264",
        "-preset", "veryfast",
        "-crf", crf_arg.as_str(),
        "-an",
        output_str,
    ];
    if let Err(error) = run_tool(&ffmpeg, &args, FFMPEG_TIMEOUT) {
        let _ = std::fs::remove_file(&output);
        return Err(error);
    }
    Ok(output)
}

/// Locates ffmpeg/ffprobe: an explicit env override wins, then known Homebrew
/// paths, so the app still works when launched from Finder without a shell PATH.
fn find_tool(name: &str, env_var: &str, candidates: &[&str]) -> Result<PathBuf, String> {
    if let Ok(custom) = std::env::var(env_var) {
        let custom = custom.trim();
        if !custom.is_empty() {
            let path = Path::new(custom);
            if path.is_file() {
                return Ok(path.to_path_buf());
            }
        }
    }
    for candidate in candidates {
        let path = Path::new(candidate);
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
    }
    Err(format!("未找到 {name}；请安装或设置 {env_var} 环境变量"))
}

/// Runs a fixed binary, draining both pipes in background threads (ffmpeg
/// writes a lot to stderr and would otherwise deadlock a full pipe), then
/// returns stdout on success.
fn run_tool(program: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| format!("无法启动 {}", program.display()))?;
    let stdout = child.stdout.take().ok_or("无法读取进程输出")?;
    let stderr = child.stderr.take().ok_or("无法读取进程输出")?;
    let out_handle = std::thread::spawn(move || {
        let mut reader = stdout;
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        buffer
    });
    let err_handle = std::thread::spawn(move || {
        let mut reader = stderr;
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        buffer
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("无法读取进程状态".to_owned());
            }
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{} 执行超时", program.display()));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stderr_bytes = err_handle.join().unwrap_or_default();
    let stdout_bytes = out_handle.join().unwrap_or_default();
    if !status.success() {
        let stderr_text = String::from_utf8_lossy(&stderr_bytes);
        let last = stderr_text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .last()
            .unwrap_or("未知错误");
        return Err(format!("{} 失败：{}", program.display(), last));
    }
    Ok(String::from_utf8_lossy(&stdout_bytes).trim().to_owned())
}

fn video_mime(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        _ => return None,
    })
}

fn describe_network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "视频分析请求超时，请重试或改用更短的片段".to_owned()
    } else if error.is_connect() {
        "无法连接 DashScope，请检查网络后重试".to_owned()
    } else {
        format!("视频分析请求失败：{error}")
    }
}

/// Extracts a human-readable message from a DashScope error body. DashScope may
/// return `{"error": {"message": "..."}}` or `{"message": "..."}`; the raw body
/// is never trusted, only surfaced to help the user.
fn extract_error(text: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
        {
            return message.chars().take(300).collect();
        }
        if let Some(message) = value.get("error").and_then(Value::as_str) {
            return message.chars().take(300).collect();
        }
        if let Some(message) = value.get("message").and_then(Value::as_str) {
            return message.chars().take(300).collect();
        }
    }
    let cleaned: String = text.chars().filter(|character| !character.is_control()).take(300).collect();
    if cleaned.trim().is_empty() {
        "未知错误".to_owned()
    } else {
        cleaned
    }
}

/// Walks `choices[0].message.content`, which is either a plain string or an
/// array of `{type: "text", text: "..."}` parts.
fn extract_content(value: &Value) -> Option<String> {
    let content = value.get("choices")?.get(0)?.get("message")?.get("content")?;
    match content {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => {
            let mut out = String::new();
            for part in parts {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(text);
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        _ => None,
    }
}

fn truncate_result(mut text: String) -> String {
    if text.len() <= MAX_RESULT_BYTES {
        return text;
    }
    let mut result = String::with_capacity(MAX_RESULT_BYTES + 32);
    for character in text.drain(..) {
        if result.len() + character.len_utf8() > MAX_RESULT_BYTES {
            break;
        }
        result.push(character);
    }
    result.push_str("\n…（分析结果过长已截断）");
    result
}

#[cfg(test)]
mod tests {
    use super::{extract_content, resolve_window, truncate_result, video_mime, MAX_RESULT_BYTES, MAX_WINDOW_SEC};
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn mime_is_derived_from_supported_extensions() {
        assert_eq!(video_mime(Path::new("clip.mp4")), Some("video/mp4"));
        assert_eq!(video_mime(Path::new("clip.MOV")), Some("video/quicktime"));
        assert_eq!(video_mime(Path::new("clip.m4v")), Some("video/mp4"));
        assert!(video_mime(Path::new("clip.exe")).is_none());
        assert!(video_mime(Path::new("clip")).is_none());
    }

    #[test]
    fn content_is_extracted_from_string_or_parts() {
        let string_form = extract_content(&json!({ "choices": [{ "message": { "content": "分析文本" } }] }));
        assert_eq!(string_form.as_deref(), Some("分析文本"));
        let parts_form = extract_content(&json!({
            "choices": [{ "message": { "content": [
                { "type": "text", "text": "第一段" },
                { "type": "text", "text": "第二段" }
            ] } }]
        }));
        assert_eq!(parts_form.as_deref(), Some("第一段\n第二段"));
        assert!(extract_content(&json!({})).is_none());
    }

    #[test]
    fn oversized_results_are_truncated_on_char_boundaries() {
        let long = "对".repeat(20_000);
        let cut = truncate_result(long);
        assert!(cut.len() > MAX_RESULT_BYTES && cut.len() <= MAX_RESULT_BYTES + 64);
        assert!(cut.ends_with("已截断）"));
    }

    #[test]
    fn window_resolution_clamps_and_rejects_bad_ranges() {
        let whole = resolve_window(60.0, None, None).unwrap();
        assert_eq!((whole.start, whole.end), (0.0, 60.0));

        let clipped = resolve_window(2400.0, Some(750.0), Some(780.0)).unwrap();
        assert_eq!((clipped.start, clipped.end), (750.0, 780.0));

        let clamped = resolve_window(60.0, None, Some(1000.0)).unwrap();
        assert_eq!(clamped.end, 60.0);

        assert!(resolve_window(60.0, Some(70.0), None).is_err());
        assert!(resolve_window(60.0, Some(30.0), Some(20.0)).is_err());
        // A whole file longer than the window cap must tell the caller to narrow.
        assert!(resolve_window(2400.0, None, None).is_err());
        assert!(resolve_window(1000.0, Some(0.0), Some(MAX_WINDOW_SEC + 1.0)).is_err());
    }
}
