//! Browser service: manages the fixed Node browser sidecar process.
//!
//! The browser sidecar drives Chrome/Edge headless through playwright-core. The
//! Rust gateway only ever launches the fixed sidecar, sends one JSON command at
//! a time and reads the matching result; it never talks to Chrome directly and
//! never runs arbitrary shell commands.

use serde::Deserialize;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
#[cfg(not(debug_assertions))]
use tauri::path::BaseDirectory;
use tauri::AppHandle;
#[cfg(not(debug_assertions))]
use tauri::Manager;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

/// A decoded result line from the browser sidecar.
#[derive(Clone, Debug)]
pub struct BrowserResult {
    pub ok: bool,
    pub text: String,
    pub details: serde_json::Value,
}

#[derive(Deserialize)]
struct ResultLine {
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    ok: bool,
    text: String,
    #[serde(default)]
    details: serde_json::Value,
}

#[derive(Default)]
pub struct BrowserService {
    inner: Arc<Mutex<Option<BrowserProcess>>>,
    counter: AtomicU64,
}

struct BrowserProcess {
    child: Child,
    stdin: ChildStdin,
    pending: Arc<Mutex<HashMap<String, mpsc::Sender<BrowserResult>>>>,
}

impl BrowserService {
    /// Ensures the sidecar is running and forwards one command, blocking until
    /// the sidecar answers (or the timeout elapses).
    pub fn request(
        &self,
        app: &AppHandle,
        command: serde_json::Value,
    ) -> Result<BrowserResult, String> {
        self.ensure(app)?;
        let request_id = self.next_request_id();
        let mut payload = command;
        let line = match payload.as_object_mut() {
            Some(map) => {
                map.insert("requestId".to_owned(), serde_json::json!(request_id));
                payload.to_string()
            }
            None => return Err("浏览器命令格式无效".into()),
        };
        let (sender, receiver) = mpsc::channel();
        let pending = {
            let mut guard = self.inner.lock().map_err(|_| "浏览器服务状态不可用")?;
            let process = guard.as_mut().ok_or("浏览器服务未运行")?;
            process
                .pending
                .lock()
                .map_err(|_| "浏览器服务状态不可用")?
                .insert(request_id.clone(), sender);
            process
                .write_line(&line)
                .map_err(|_| "无法向浏览器服务发送命令".to_owned())?;
            Arc::clone(&process.pending)
        };
        let result = receiver
            .recv_timeout(REQUEST_TIMEOUT)
            .map_err(|_| "浏览器操作超时".to_owned())?;
        let _ = pending.lock().map(|mut map| map.remove(&request_id));
        Ok(result)
    }

    /// Shuts the sidecar down, closing the browser it owns.
    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(mut process) = guard.take() {
                let _ = process.child.kill();
                let _ = process.child.wait();
            }
        }
    }

    fn ensure(&self, app: &AppHandle) -> Result<(), String> {
        let mut guard = self.inner.lock().map_err(|_| "浏览器服务状态不可用")?;
        if let Some(process) = guard.as_mut() {
            if process.child.try_wait().ok().flatten().is_none() {
                return Ok(());
            }
        }
        *guard = Some(self.spawn(app)?);
        Ok(())
    }

    #[cfg_attr(debug_assertions, allow(unused_variables))]
    fn spawn(&self, app: &AppHandle) -> Result<BrowserProcess, String> {
        #[cfg(debug_assertions)]
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../packages/agent/src/browser-sidecar.mjs");
        #[cfg(debug_assertions)]
        let program = std::path::PathBuf::from("node");
        #[cfg(not(debug_assertions))]
        let script = app
            .path()
            .resolve("phoebe-browser.cjs", BaseDirectory::Resource)
            .map_err(|_| "无法定位打包的浏览器资源")?;
        #[cfg(not(debug_assertions))]
        let program = std::env::current_exe()
            .map_err(|_| "无法定位桌面应用程序")?
            .parent()
            .ok_or("桌面应用程序路径无效")?
            .join(if cfg!(windows) {
                "phoebe-agent-node.exe"
            } else {
                "phoebe-agent-node"
            });
        if !script.is_file() {
            return Err("打包的浏览器 Sidecar 不完整，请重新安装应用".into());
        }
        let mut command = Command::new(&program);
        command
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env_clear();
        for name in [
            "PATH",
            "SYSTEMROOT",
            "TEMP",
            "TMP",
            "TMPDIR",
            "https_proxy",
            "HTTPS_PROXY",
            "http_proxy",
            "HTTP_PROXY",
            "no_proxy",
            "NO_PROXY",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let mut child = command.spawn().map_err(|_| "无法启动浏览器 Sidecar")?;
        let stdin = child.stdin.take().ok_or("浏览器 Sidecar 输入不可用")?;
        let stdout = child.stdout.take().ok_or("浏览器 Sidecar 输出不可用")?;
        let pending: Arc<Mutex<HashMap<String, mpsc::Sender<BrowserResult>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let reader_pending = Arc::clone(&pending);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(event) = serde_json::from_str::<ResultLine>(&line) else {
                    continue;
                };
                let Some(request_id) = event.request_id else { continue };
                let sender = reader_pending
                    .lock()
                    .ok()
                    .and_then(|mut map| map.remove(&request_id));
                if let Some(sender) = sender {
                    let _ = sender.send(BrowserResult {
                        ok: event.ok,
                        text: event.text,
                        details: event.details,
                    });
                }
            }
        });
        Ok(BrowserProcess {
            child,
            stdin,
            pending,
        })
    }

    fn next_request_id(&self) -> String {
        let counter = self.counter.fetch_add(1, Ordering::Relaxed);
        format!(
            "b{}-{:x}",
            counter,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.subsec_nanos() as u64)
                .unwrap_or(0)
        )
    }
}

impl BrowserProcess {
    fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()
    }
}
