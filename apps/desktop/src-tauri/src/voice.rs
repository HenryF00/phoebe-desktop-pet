use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(not(debug_assertions))]
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};

const TTS_ENDPOINT: &str = "http://127.0.0.1:9880/tts";
const HEALTH_ENDPOINT: &str = "http://127.0.0.1:9880/openapi.json";
const GPT_WEIGHT: &str = "GPT_weights_v2/phoebe_zh_v2-e15.ckpt";
const SOVITS_WEIGHT: &str = "SoVITS_weights_v2/phoebe_zh_v2_e8_s912.pth";
const REFERENCE_NAME: &str = "021_自我介绍.wav";
const BIRTHDAY_NAME: &str = "017_生日祝福.wav";
const REFERENCE_TEXT: &str = "我是隐海修会的教士，菲比。岁主在上，愿你的旅途永远有爱与光明垂耀。";
const MAX_SPEECH_BYTES: usize = 12_000;
const MAX_WAV_BYTES: usize = 64 * 1024 * 1024;
#[cfg_attr(debug_assertions, allow(dead_code))]
const VOICE_RUNTIME_FORMAT: u8 = 1;
/// Budget for loading the model after the process is spawned. The runtime has
/// already been expanded by then, so this is the CPU model-load budget.
const SERVER_LOAD_TIMEOUT: Duration = Duration::from_secs(180);

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
    /// Temporary synthesized files are deleted on stop; bundled clips are not.
    remove_on_stop: bool,
}

struct ManagedServer {
    child: Child,
    log_path: PathBuf,
}

#[cfg_attr(debug_assertions, allow(dead_code))]
#[derive(Deserialize)]
struct ArchiveResource {
    file: String,
    sha256: String,
}

#[cfg_attr(debug_assertions, allow(dead_code))]
#[derive(Deserialize)]
struct VoiceRuntimeManifest {
    format: u8,
    runtime_id: String,
    platform: String,
    architecture: String,
    python_archive: ArchiveResource,
    project_archive: ArchiveResource,
    api_script: String,
    config: String,
    gpt_weight: String,
    sovits_weight: String,
}

struct RuntimeLayout {
    root: PathBuf,
    project: PathBuf,
    python: PathBuf,
    api_script: PathBuf,
    config: PathBuf,
}

pub struct VoiceService {
    generation: AtomicU64,
    status: Mutex<&'static str>,
    playback: Mutex<Option<Playback>>,
    server: Mutex<Option<ManagedServer>>,
    startup: Mutex<()>,
}

impl Default for VoiceService {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            status: Mutex::new("not_configured"),
            playback: Mutex::new(None),
            server: Mutex::new(None),
            startup: Mutex::new(()),
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

fn health_ready(timeout: Duration) -> bool {
    client(timeout)
        .and_then(|client| {
            client
                .get(HEALTH_ENDPOINT)
                .send()
                .map_err(|_| "unreachable".to_owned())?
                .error_for_status()
                .map_err(|_| "unhealthy".to_owned())?;
            Ok(())
        })
        .is_ok()
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn valid_runtime_component(value: &str) -> bool {
    !value.is_empty()
        && !Path::new(value).is_absolute()
        && Path::new(value).components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn validate_manifest(manifest: &VoiceRuntimeManifest) -> Result<(), String> {
    if manifest.format != VOICE_RUNTIME_FORMAT
        || manifest.platform != std::env::consts::OS
        || manifest.architecture != std::env::consts::ARCH
        || manifest.runtime_id.len() > 160
        || !valid_runtime_component(&manifest.runtime_id)
        || !valid_runtime_component(&manifest.python_archive.file)
        || !valid_runtime_component(&manifest.project_archive.file)
        || !valid_runtime_component(&manifest.api_script)
        || !valid_runtime_component(&manifest.config)
        || !valid_runtime_component(&manifest.gpt_weight)
        || !valid_runtime_component(&manifest.sovits_weight)
        || manifest.python_archive.sha256.len() != 64
        || manifest.project_archive.sha256.len() != 64
    {
        return Err("内置语音运行包与当前系统不兼容，请重新安装正确版本".into());
    }
    Ok(())
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
    let mut file = File::open(path).map_err(|_| "无法读取内置语音运行包")?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "无法校验内置语音运行包")?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if format!("{:x}", digest.finalize()) != expected {
        return Err("内置语音运行包校验失败，请重新安装应用".into());
    }
    Ok(())
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn extract_targz(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|_| "无法打开内置语音运行包")?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive.entries().map_err(|_| "内置语音运行包格式无效")?;
    for entry in entries {
        let mut entry = entry.map_err(|_| "内置语音运行包包含损坏条目")?;
        let unpacked = entry
            .unpack_in(destination)
            .map_err(|_| "无法解压内置语音运行包")?;
        if !unpacked {
            return Err("内置语音运行包包含不安全路径".into());
        }
    }
    Ok(())
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn python_executable(runtime_root: &Path) -> PathBuf {
    if cfg!(windows) {
        runtime_root.join("python/python.exe")
    } else {
        runtime_root.join("python/bin/python")
    }
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn runtime_layout(root: PathBuf, manifest: &VoiceRuntimeManifest) -> RuntimeLayout {
    let project = root.join("gpt-sovits");
    RuntimeLayout {
        python: python_executable(&root),
        api_script: root.join(&manifest.api_script),
        config: root.join(&manifest.config),
        project,
        root,
    }
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn validate_layout(layout: &RuntimeLayout, manifest: &VoiceRuntimeManifest) -> Result<(), String> {
    let required = [
        &layout.python,
        &layout.api_script,
        &layout.config,
        &layout.project.join(&manifest.gpt_weight),
        &layout.project.join(&manifest.sovits_weight),
    ];
    if required.iter().any(|path| !path.is_file()) {
        return Err("内置语音运行包缺少 Python、模型或推理文件".into());
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
fn prepare_bundled_runtime(app: &AppHandle, generation: u64) -> Result<RuntimeLayout, String> {
    let resource_dir = app
        .path()
        .resolve("voice-runtime", BaseDirectory::Resource)
        .map_err(|_| "无法定位内置语音运行包")?;
    let manifest_path = resource_dir.join("manifest.json");
    let manifest: VoiceRuntimeManifest =
        serde_json::from_slice(&fs::read(&manifest_path).map_err(|_| "内置语音运行清单缺失")?)
            .map_err(|_| "内置语音运行清单格式无效")?;
    validate_manifest(&manifest)?;

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法取得本机应用数据目录")?
        .join("voice-runtime");
    fs::create_dir_all(&data_dir).map_err(|_| "无法建立本机语音运行目录")?;
    let destination = data_dir.join(&manifest.runtime_id);
    let ready_marker = destination.join(".ready");
    if ready_marker.is_file() {
        let layout = runtime_layout(destination, &manifest);
        validate_layout(&layout, &manifest)?;
        cleanup_stale_runtimes(&data_dir, &manifest.runtime_id);
        return Ok(layout);
    }

    VoiceService::emit(
        app,
        generation,
        None,
        "preparing",
        "首次准备菲比声音：正在校验语音运行包…",
    );
    let python_archive = resource_dir.join(&manifest.python_archive.file);
    let project_archive = resource_dir.join(&manifest.project_archive.file);
    verify_sha256(&python_archive, &manifest.python_archive.sha256)?;
    verify_sha256(&project_archive, &manifest.project_archive.sha256)?;

    let staging = data_dir.join(format!(
        ".installing-{}-{}",
        std::process::id(),
        manifest.runtime_id
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|_| "无法清理未完成的语音安装目录")?;
    }
    fs::create_dir_all(staging.join("python")).map_err(|_| "无法建立语音安装暂存目录")?;
    VoiceService::emit(
        app,
        generation,
        None,
        "preparing",
        "首次准备菲比声音：正在展开语音运行包（数 GB，可能需要几分钟）…",
    );
    let install_result = (|| {
        extract_targz(&python_archive, &staging.join("python"))?;
        extract_targz(&project_archive, &staging)?;
        let layout = runtime_layout(staging.clone(), &manifest);
        validate_layout(&layout, &manifest)?;
        run_conda_unpack(&layout)?;
        fs::write(staging.join(".ready"), manifest.runtime_id.as_bytes())
            .map_err(|_| "无法写入语音安装完成标记")?;
        Ok::<_, String>(())
    })();
    if let Err(error) = install_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if destination.exists() {
        fs::remove_dir_all(&destination).map_err(|_| "无法替换损坏的语音运行目录")?;
    }
    fs::rename(&staging, &destination).map_err(|_| "无法完成内置语音运行环境安装")?;
    let layout = runtime_layout(destination, &manifest);
    validate_layout(&layout, &manifest)?;
    cleanup_stale_runtimes(&data_dir, &manifest.runtime_id);
    Ok(layout)
}

/// Removes previous runtime expansions and stale partial installs, so old
/// `runtime_id` directories do not accumulate (each is several GB).
#[cfg_attr(debug_assertions, allow(dead_code))]
fn cleanup_stale_runtimes(root: &Path, keep_id: &str) {
    let Ok(reader) = fs::read_dir(root) else {
        return;
    };
    for entry in reader.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name == keep_id {
            continue;
        }
        // Runtime ids look like `v1-macos-aarch64-<hash>-<hash>`; partial
        // installs are `.installing-...`. Leave anything else untouched.
        let known = (name.starts_with('v') && name.contains('-')) || name.starts_with(".installing-");
        if !known {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

#[cfg(not(debug_assertions))]
fn run_conda_unpack(layout: &RuntimeLayout) -> Result<(), String> {
    clear_development_path_file(&layout.root)?;
    #[cfg(target_os = "windows")]
    let mut command = Command::new(layout.root.join("python/Scripts/conda-unpack.exe"));
    #[cfg(not(target_os = "windows"))]
    let mut command = {
        let mut value = Command::new(&layout.python);
        value
            .arg("-S")
            .arg(layout.root.join("python/bin/conda-unpack"));
        value
    };
    let output = command
        .current_dir(&layout.root)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "无法重定位内置 Python 环境")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim().chars().take(400).collect::<String>();
        return Err(if detail.is_empty() {
            "内置 Python 环境重定位失败，请重新安装应用".into()
        } else {
            format!("内置 Python 环境重定位失败：{detail}")
        });
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
fn clear_development_path_file(runtime_root: &Path) -> Result<(), String> {
    let windows_path = runtime_root.join("python/Lib/site-packages/users.pth");
    if windows_path.exists() {
        fs::write(windows_path, b"").map_err(|_| "无法清理 Python 开发环境路径")?;
    }
    let unix_lib = runtime_root.join("python/lib");
    if unix_lib.is_dir() {
        for entry in fs::read_dir(unix_lib).map_err(|_| "无法检查 Python 库目录")? {
            let path = entry.map_err(|_| "无法检查 Python 库目录")?.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("python"))
            {
                let users_path = path.join("site-packages/users.pth");
                if users_path.exists() {
                    fs::write(users_path, b"").map_err(|_| "无法清理 Python 开发环境路径")?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn prepare_bundled_runtime(_app: &AppHandle, _generation: u64) -> Result<RuntimeLayout, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let project = env::var_os("PHOEBE_GPTSOVITS_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("../../../../GPT-SoVITS"));
    let environment = env::var_os("PHOEBE_GPTSOVITS_ENV")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join("miniconda3/envs/GPTSoVits"))
        })
        .ok_or_else(|| "未配置开发环境 PHOEBE_GPTSOVITS_ENV".to_owned())?;
    let root = project.parent().unwrap_or(manifest_dir).to_path_buf();
    let layout = RuntimeLayout {
        python: if cfg!(windows) {
            environment.join("python.exe")
        } else {
            environment.join("bin/python")
        },
        api_script: project.join("api_v2.py"),
        config: project.join("GPT_SoVITS/configs/tts_infer.yaml"),
        project,
        root,
    };
    if [&layout.python, &layout.api_script, &layout.config]
        .iter()
        .any(|path| !path.is_file())
    {
        return Err(
            "开发版未找到 GPT-SoVITS 环境；请设置 PHOEBE_GPTSOVITS_ROOT 与 PHOEBE_GPTSOVITS_ENV"
                .into(),
        );
    }
    Ok(layout)
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

/// The pre-recorded Phoebe birthday line, played directly (no TTS).
fn birthday_audio(_app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../phoebe_voice_zh/wav")
        .join(BIRTHDAY_NAME);
    #[cfg(not(debug_assertions))]
    let path = _app
        .path()
        .resolve(format!("voice/{BIRTHDAY_NAME}"), BaseDirectory::Resource)
        .map_err(|_| "无法定位打包的生日祝福音频")?;
    if !path.is_file() {
        return Err("生日祝福音频缺失，请重新安装应用".into());
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

    fn stop_managed_server(&self) {
        let server = self.server.lock().ok().and_then(|mut value| value.take());
        if let Some(mut server) = server {
            let _ = server.child.kill();
            let _ = server.child.wait();
        }
    }

    fn spawn_server(&self, app: &AppHandle, layout: &RuntimeLayout) -> Result<(), String> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|_| "无法取得语音日志目录")?;
        let cache_dir = app
            .path()
            .app_cache_dir()
            .map_err(|_| "无法取得语音缓存目录")?
            .join("gpt-sovits");
        let log_dir = data_dir.join("logs");
        let directories = [
            cache_dir.clone(),
            cache_dir.join("numba"),
            cache_dir.join("matplotlib"),
            cache_dir.join("huggingface"),
            cache_dir.join("pycache"),
            log_dir.clone(),
        ];
        for directory in directories {
            fs::create_dir_all(directory).map_err(|_| "无法建立语音缓存或日志目录")?;
        }
        let log_path = log_dir.join("gpt-sovits.log");
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|_| "无法打开 GPT-SoVITS 日志")?;
        let error_log = log
            .try_clone()
            .map_err(|_| "无法准备 GPT-SoVITS 错误日志")?;

        let mut command = Command::new(&layout.python);
        command
            .arg(&layout.api_script)
            .args(["-a", "127.0.0.1", "-p", "9880", "-c"])
            .arg(&layout.config)
            .current_dir(&layout.project)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(error_log))
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONPYCACHEPREFIX", cache_dir.join("pycache"))
            .env("NUMBA_CACHE_DIR", cache_dir.join("numba"))
            .env("MPLCONFIGDIR", cache_dir.join("matplotlib"))
            .env("HF_HOME", cache_dir.join("huggingface"))
            .env("XDG_CACHE_HOME", &cache_dir)
            .env("TOKENIZERS_PARALLELISM", "false");
        let old_path = env::var_os("PATH").unwrap_or_default();
        let mut paths = if cfg!(windows) {
            vec![
                layout.root.join("python"),
                layout.root.join("python/Scripts"),
                layout.root.join("python/Library/bin"),
            ]
        } else {
            vec![layout.root.join("python/bin")]
        };
        paths.extend(env::split_paths(&old_path));
        if let Ok(path) = env::join_paths(paths) {
            command.env("PATH", path);
        }
        #[cfg(target_os = "windows")]
        command.creation_flags(0x08000000);

        let child = command
            .spawn()
            .map_err(|_| "无法启动内置 GPT-SoVITS 推理服务")?;
        let mut server = self.server.lock().map_err(|_| "语音服务进程状态不可用")?;
        *server = Some(ManagedServer { child, log_path });
        Ok(())
    }

    fn ensure_server(&self, app: &AppHandle) -> Result<bool, String> {
        if health_ready(Duration::from_secs(2)) {
            self.set_status("ready");
            return Ok(false);
        }
        let _startup = self.startup.lock().map_err(|_| "语音服务启动锁不可用")?;
        if health_ready(Duration::from_secs(1)) {
            self.set_status("ready");
            return Ok(false);
        }
        if let Ok(mut server) = self.server.lock() {
            let exited = server
                .as_mut()
                .and_then(|value| value.child.try_wait().ok().flatten())
                .is_some();
            if exited {
                *server = None;
            }
        }
        self.set_status("starting");
        let generation = self.generation.load(Ordering::SeqCst);
        // Phase 1: make sure the runtime is expanded (long, first run only).
        let layout = prepare_bundled_runtime(app, generation)?;
        // Phase 2: spawn and wait for the model to load (short budget).
        Self::emit(
            app,
            generation,
            None,
            "preparing",
            "正在加载菲比语音模型（本机 CPU 推理，约需半分钟）…",
        );
        self.spawn_server(app, &layout)?;
        let log_path = self
            .server
            .lock()
            .ok()
            .and_then(|server| server.as_ref().map(|value| value.log_path.clone()));
        let deadline = Instant::now() + SERVER_LOAD_TIMEOUT;
        while Instant::now() < deadline {
            if health_ready(Duration::from_secs(1)) {
                self.set_status("ready");
                return Ok(true);
            }
            let exited = self.server.lock().ok().and_then(|mut server| {
                server.as_mut().and_then(|value| {
                    value
                        .child
                        .try_wait()
                        .ok()
                        .flatten()
                        .map(|_| value.log_path.clone())
                })
            });
            if let Some(log_path) = exited {
                if let Ok(mut server) = self.server.lock() {
                    *server = None;
                }
                self.set_status("failed");
                return Err(format!(
                    "内置 GPT-SoVITS 启动失败，请查看日志：{}",
                    log_path.display()
                ));
            }
            thread::sleep(Duration::from_millis(350));
        }
        self.stop_managed_server();
        self.set_status("failed");
        match log_path {
            Some(log_path) => Err(format!(
                "菲比语音模型加载超时；请查看日志：{}",
                log_path.display()
            )),
            None => Err("菲比语音模型加载超时；文字回复仍可正常使用".into()),
        }
    }

    pub fn prewarm(app: &AppHandle) {
        let handle = app.clone();
        thread::spawn(move || {
            let service = handle.state::<VoiceService>();
            let generation = service.generation.load(Ordering::SeqCst);
            let status = match service.ensure_server(&handle) {
                Ok(_) => {
                    Self::emit(&handle, generation, None, "idle", "菲比语音已就绪");
                    "ready"
                }
                Err(error) => {
                    Self::emit(&handle, generation, None, "failed", error);
                    "failed"
                }
            };
            let _ = handle.emit("voice-status", status);
        });
    }

    pub fn shutdown(&self) {
        self.stop_playback();
        self.stop_managed_server();
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
            if playback.remove_on_stop {
                let _ = fs::remove_file(playback.path);
            }
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
        let reachable = self.ensure_server(app);
        let (status, message) = match (reachable, reference_ok) {
            (Ok(started), true) => (
                "ready",
                if started {
                    "内置 GPT-SoVITS 已自动启动，GPT e15、SoVITS e8 与参考音频均可用。"
                } else {
                    "GPT-SoVITS 与参考音频可用；已复用正在运行的本机服务。"
                }
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

    /// Plays the bundled birthday line directly, bypassing TTS synthesis.
    pub fn play_clip(&self, app: &AppHandle, run_id: String, emotion: String, gesture: String) {
        let path = match birthday_audio(app) {
            Ok(path) => path,
            Err(message) => {
                let generation = self.generation.load(Ordering::SeqCst);
                Self::emit(app, generation, Some(run_id), "failed", message);
                return;
            }
        };
        self.stop_playback();
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        Self::emit_with_cue(
            app,
            generation,
            Some(run_id.clone()),
            "speaking",
            "菲比正在播放生日祝福",
            Some(&emotion),
            Some(&gesture),
        );
        let handle = app.clone();
        thread::spawn(move || {
            let service = handle.state::<VoiceService>();
            let child = match spawn_player(&path) {
                Ok(child) => Arc::new(Mutex::new(child)),
                Err(message) => {
                    Self::emit(&handle, generation, Some(run_id), "failed", message);
                    return;
                }
            };
            if service.generation.load(Ordering::SeqCst) != generation {
                if let Ok(mut player) = child.lock() {
                    let _ = player.kill();
                }
                return;
            }
            if let Ok(mut playback) = service.playback.lock() {
                *playback = Some(Playback {
                    generation,
                    child: Arc::clone(&child),
                    path: path.clone(),
                    remove_on_stop: false,
                });
            }
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
            if service.generation.load(Ordering::SeqCst) == generation {
                if let Ok(mut playback) = service.playback.lock() {
                    if playback.as_ref().map(|value| value.generation) == Some(generation) {
                        *playback = None;
                    }
                }
                Self::emit(&handle, generation, Some(run_id), "idle", "生日祝福播放完成");
            }
        });
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
        self.set_status("starting");
        Self::emit_with_cue(
            app,
            generation,
            Some(run_id.clone()),
            "synthesizing",
            "正在准备菲比声音并合成回复…",
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
                    remove_on_stop: true,
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
        self.shutdown();
    }
}

fn synthesize_wav(app: &AppHandle, text: &str) -> Result<Vec<u8>, String> {
    app.state::<VoiceService>().ensure_server(app)?;
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
        .map_err(|_| "无法连接自动管理的 GPT-SoVITS；文字回复仍已保留".to_owned())?;
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
    use super::{
        cleanup_stale_runtimes, valid_runtime_component, validate_manifest, validate_wav,
        ArchiveResource, VoiceRuntimeManifest, GPT_WEIGHT, REFERENCE_TEXT, SOVITS_WEIGHT,
        VOICE_RUNTIME_FORMAT,
    };

    #[test]
    fn stale_runtime_dirs_are_removed_but_the_current_one_is_kept() {
        let base = std::env::temp_dir().join(format!("phoebe-voice-clean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("v1-macos-aarch64-old")).unwrap();
        std::fs::create_dir_all(base.join("v1-macos-aarch64-new")).unwrap();
        std::fs::create_dir_all(base.join(".installing-999-v1")).unwrap();
        std::fs::create_dir_all(base.join("unrelated-notes")).unwrap();
        cleanup_stale_runtimes(&base, "v1-macos-aarch64-new");
        assert!(base.join("v1-macos-aarch64-new").is_dir());
        assert!(!base.join("v1-macos-aarch64-old").exists());
        assert!(!base.join(".installing-999-v1").exists());
        // Unrelated directories are left alone.
        assert!(base.join("unrelated-notes").is_dir());
        let _ = std::fs::remove_dir_all(&base);
    }

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

    #[test]
    fn rejects_unsafe_or_foreign_voice_runtime_manifests() {
        let manifest = VoiceRuntimeManifest {
            format: VOICE_RUNTIME_FORMAT,
            runtime_id: "v1-test".into(),
            platform: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            python_archive: ArchiveResource {
                file: "python-env.tar.gz".into(),
                sha256: "a".repeat(64),
            },
            project_archive: ArchiveResource {
                file: "gpt-sovits.tar.gz".into(),
                sha256: "b".repeat(64),
            },
            api_script: "gpt-sovits/api_v2.py".into(),
            config: "gpt-sovits/phoebe_tts.yaml".into(),
            gpt_weight: GPT_WEIGHT.into(),
            sovits_weight: SOVITS_WEIGHT.into(),
        };
        assert!(validate_manifest(&manifest).is_ok());
        assert!(!valid_runtime_component("../escape"));
        assert!(!valid_runtime_component("/absolute"));

        let foreign = VoiceRuntimeManifest {
            platform: "foreign-os".into(),
            ..manifest
        };
        assert!(validate_manifest(&foreign).is_err());
    }
}
