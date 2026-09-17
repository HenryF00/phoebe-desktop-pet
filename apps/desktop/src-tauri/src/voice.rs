use reqwest::blocking::Client;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
#[cfg(not(debug_assertions))]
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};

const TTS_ENDPOINT: &str = "http://127.0.0.1:9880/tts";
const HEALTH_ENDPOINT: &str = "http://127.0.0.1:9880/openapi.json";
const GPT_WEIGHT: &str = "GPT_weights_v2/phoebe_zh_v2-e15.ckpt";
const SOVITS_WEIGHT: &str = "SoVITS_weights_v2/phoebe_zh_v2_e8_s912.pth";
const REFERENCE_NAME: &str = "021_自我介绍.wav";
const REFERENCE_TEXT: &str = "我是隐海修会的教士，菲比。岁主在上，愿你的旅途永远有爱与光明垂耀。";
const MAX_SPEECH_BYTES: usize = 12_000;
const MAX_WAV_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Serialize)]
pub struct VoiceEvent {
    pub generation: u64,
    #[serde(rename = "runId", skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub state: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gesture: Option<String>,
}

#[derive(Serialize)]
pub struct VoiceDiagnostic {
    pub status: &'static str,
    pub message: String,
    pub endpoint: &'static str,
    pub model: String,
    pub reference: &'static str,
}

#[derive(Serialize)]
struct TtsRequest<'a> {
    text: &'a str,
    text_lang: &'static str,
    ref_audio_path: String,
    prompt_lang: &'static str,
    prompt_text: &'static str,
    text_split_method: &'static str,
    speed_factor: f32,
    fragment_interval: f32,
    top_k: u8,
    top_p: f32,
    temperature: f32,
    seed: i32,
    media_type: &'static str,
    streaming_mode: bool,
    batch_size: u8,
}

struct Playback {
    generation: u64,
    child: Arc<Mutex<Child>>,
    path: PathBuf,
}

pub struct VoiceService {
    generation: AtomicU64,
    status: Mutex<&'static str>,
    playback: Mutex<Option<Playback>>,
}

impl Default for VoiceService {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            status: Mutex::new("not_configured"),
            playback: Mutex::new(None),
        }
    }
}

fn client(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .timeout(timeout)
        .build()
        .map_err(|_| "无法建立本机语音连接".into())
}

fn reference_audio(_app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../phoebe_voice_zh/wav")
        .join(REFERENCE_NAME);
    #[cfg(not(debug_assertions))]
    let path = _app
        .path()
        .resolve(format!("voice/{REFERENCE_NAME}"), BaseDirectory::Resource)
        .map_err(|_| "无法定位打包的菲比参考音频")?;
    if !path.is_file() {
        return Err("菲比参考音频缺失，请重新安装应用".into());
    }
    Ok(path)
}

fn validate_wav(data: &[u8]) -> Result<(), String> {
    if data.len() < 44 || data.len() > MAX_WAV_BYTES {
        return Err("语音服务返回的音频大小无效".into());
    }
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("语音服务没有返回有效 WAV 音频".into());
    }
    Ok(())
}

fn spawn_player(path: &Path) -> Result<Child, String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut value = Command::new("/usr/bin/afplay");
        value.arg(path);
        value
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut value = Command::new("powershell.exe");
        value
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(New-Object System.Media.SoundPlayer $env:PHOEBE_VOICE_WAV).PlaySync()",
            ])
            .env("PHOEBE_VOICE_WAV", path);
        value
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Err("当前平台尚未配置本机 WAV 播放器".into());

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "无法启动本机音频播放器".into())
}

impl VoiceService {
    pub fn status(&self) -> &'static str {
        self.status.lock().map(|value| *value).unwrap_or("failed")
    }

    fn set_status(&self, value: &'static str) {
        if let Ok(mut status) = self.status.lock() {
            *status = value;
        }
    }

    fn emit(
        app: &AppHandle,
        generation: u64,
        run_id: Option<String>,
        state: &'static str,
        message: impl Into<String>,
    ) {
        Self::emit_with_cue(app, generation, run_id, state, message, None, None);
    }

    fn emit_with_cue(
        app: &AppHandle,
        generation: u64,
        run_id: Option<String>,
        state: &'static str,
        message: impl Into<String>,
        emotion: Option<&str>,
        gesture: Option<&str>,
    ) {
        let _ = app.emit(
            "voice-event",
            VoiceEvent {
                generation,
                run_id,
                state,
                message: message.into(),
                emotion: emotion.map(str::to_owned),
                gesture: gesture.map(str::to_owned),
            },
        );
    }

    fn stop_playback(&self) {
        let playback = self.playback.lock().ok().and_then(|mut value| value.take());
        if let Some(playback) = playback {
            if let Ok(mut child) = playback.child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
            let _ = fs::remove_file(playback.path);
        }
    }

    pub fn stop(&self, app: &AppHandle) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let had_playback = self
            .playback
            .lock()
            .map(|value| value.is_some())
            .unwrap_or(false);
        self.stop_playback();
        if had_playback {
            Self::emit(app, generation, None, "stopped", "已停止菲比语音");
        }
    }

    pub fn check(&self, app: &AppHandle) -> VoiceDiagnostic {
        let reference_ok = reference_audio(app).is_ok();
        let reachable = client(Duration::from_secs(4)).and_then(|client| {
            client
                .get(HEALTH_ENDPOINT)
                .send()
                .map_err(|_| "无法连接 127.0.0.1:9880；请先启动 api_v2.py".to_owned())?
                .error_for_status()
                .map_err(|_| "本机语音 API 返回错误".to_owned())?;
            Ok(())
        });
        let (status, message) = match (reachable, reference_ok) {
            (Ok(()), true) => (
                "ready",
                "本机语音 API 与参考音频可用；请确认独立 API 已加载 GPT e15 和 SoVITS e8。"
                    .to_owned(),
            ),
            (Err(error), _) => ("failed", error),
            (_, false) => ("failed", "菲比参考音频缺失，请重新构建应用".to_owned()),
        };
        self.set_status(status);
        VoiceDiagnostic {
            status,
            message,
            endpoint: TTS_ENDPOINT,
            model: format!("GPT e15 · SoVITS e8 ({GPT_WEIGHT} · {SOVITS_WEIGHT})"),
            reference: REFERENCE_NAME,
        }
    }

    pub fn synthesize(
        &self,
        app: &AppHandle,
        run_id: String,
        text: String,
        emotion: String,
        gesture: String,
    ) {
        let speech = text.trim().to_owned();
        if speech.is_empty() || speech.len() > MAX_SPEECH_BYTES {
            let generation = self.generation.load(Ordering::SeqCst);
            Self::emit(
                app,
                generation,
                Some(run_id),
                "failed",
                "本轮语音文本为空或过长，已保留文字回复",
            );
            return;
        }
        self.stop_playback();
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.set_status("ready");
        Self::emit_with_cue(
            app,
            generation,
            Some(run_id.clone()),
            "synthesizing",
            "正在用菲比声音合成回复…",
            Some(&emotion),
            Some(&gesture),
        );
        let handle = app.clone();
        thread::spawn(move || {
            let result = synthesize_wav(&handle, &speech);
            let service = handle.state::<VoiceService>();
            if service.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let data = match result {
                Ok(data) => data,
                Err(message) => {
                    service.set_status("failed");
                    Self::emit(&handle, generation, Some(run_id), "failed", message);
                    return;
                }
            };
            let cache_dir = match handle.path().app_cache_dir() {
                Ok(path) => path.join("voice"),
                Err(_) => {
                    Self::emit(
                        &handle,
                        generation,
                        Some(run_id),
                        "failed",
                        "无法取得语音缓存目录",
                    );
                    return;
                }
            };
            if fs::create_dir_all(&cache_dir).is_err() {
                Self::emit(
                    &handle,
                    generation,
                    Some(run_id),
                    "failed",
                    "无法建立语音缓存目录",
                );
                return;
            }
            let path = cache_dir.join(format!("reply-{generation}.wav"));
            if fs::write(&path, data).is_err() {
                Self::emit(
                    &handle,
                    generation,
                    Some(run_id),
                    "failed",
                    "无法写入临时语音文件",
                );
                return;
            }
            let child = match spawn_player(&path) {
                Ok(child) => Arc::new(Mutex::new(child)),
                Err(message) => {
                    let _ = fs::remove_file(&path);
                    Self::emit(&handle, generation, Some(run_id), "failed", message);
                    return;
                }
            };
            if service.generation.load(Ordering::SeqCst) != generation {
                if let Ok(mut player) = child.lock() {
                    let _ = player.kill();
                }
                let _ = fs::remove_file(path);
                return;
            }
            if let Ok(mut playback) = service.playback.lock() {
                *playback = Some(Playback {
                    generation,
                    child: Arc::clone(&child),
                    path: path.clone(),
                });
            }
            Self::emit_with_cue(
                &handle,
                generation,
                Some(run_id.clone()),
                "speaking",
                "菲比正在播放语音回复",
                Some(&emotion),
                Some(&gesture),
            );
            loop {
                if service.generation.load(Ordering::SeqCst) != generation {
                    break;
                }
                let finished = child
                    .lock()
                    .ok()
                    .and_then(|mut player| player.try_wait().ok().flatten())
                    .is_some();
                if finished {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            let _ = fs::remove_file(&path);
            if service.generation.load(Ordering::SeqCst) == generation {
                if let Ok(mut playback) = service.playback.lock() {
                    if playback.as_ref().map(|value| value.generation) == Some(generation) {
                        *playback = None;
                    }
                }
                Self::emit(
                    &handle,
                    generation,
                    Some(run_id),
                    "idle",
                    "菲比语音播放完成",
                );
            }
        });
    }
}

impl Drop for VoiceService {
    fn drop(&mut self) {
        self.stop_playback();
    }
}

fn synthesize_wav(app: &AppHandle, text: &str) -> Result<Vec<u8>, String> {
    let reference = reference_audio(app)?;
    let request = TtsRequest {
        text,
        text_lang: "zh",
        ref_audio_path: reference.to_string_lossy().into_owned(),
        prompt_lang: "zh",
        prompt_text: REFERENCE_TEXT,
        text_split_method: "cut1",
        speed_factor: 1.0,
        fragment_interval: 0.32,
        top_k: 15,
        top_p: 1.0,
        temperature: 1.0,
        seed: -1,
        media_type: "wav",
        streaming_mode: false,
        batch_size: 1,
    };
    let response = client(Duration::from_secs(240))?
        .post(TTS_ENDPOINT)
        .json(&request)
        .send()
        .map_err(|_| "无法连接本机 GPT-SoVITS；请确认 9880 API 与模型已加载".to_owned())?;
    if !response.status().is_success() {
        return Err("GPT-SoVITS 合成失败；请检查 API 终端中的模型与参考音频错误".into());
    }
    let data = response
        .bytes()
        .map_err(|_| "无法读取 GPT-SoVITS 返回的音频".to_owned())?
        .to_vec();
    validate_wav(&data)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::{validate_wav, GPT_WEIGHT, REFERENCE_TEXT, SOVITS_WEIGHT};

    #[test]
    fn baseline_voice_configuration_is_fixed() {
        assert!(GPT_WEIGHT.ends_with("phoebe_zh_v2-e15.ckpt"));
        assert!(SOVITS_WEIGHT.ends_with("phoebe_zh_v2_e8_s912.pth"));
        assert_eq!(
            REFERENCE_TEXT,
            "我是隐海修会的教士，菲比。岁主在上，愿你的旅途永远有爱与光明垂耀。"
        );
    }

    #[test]
    fn rejects_non_wav_and_accepts_bounded_riff_wave() {
        assert!(validate_wav(b"not audio").is_err());
        let mut wav = vec![0_u8; 44];
        wav[0..4].copy_from_slice(b"RIFF");
        wav[8..12].copy_from_slice(b"WAVE");
        assert!(validate_wav(&wav).is_ok());
    }
}
