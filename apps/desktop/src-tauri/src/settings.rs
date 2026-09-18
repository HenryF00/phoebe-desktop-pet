use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager};

pub const DEFAULT_MODEL: &str = "deepseek-v4-flash";
pub const DEFAULT_USER_ADDRESS: &str = "漂泊者";
pub const DEFAULT_PET_SCALE_PERCENT: u16 = 100;
pub const PET_SCALE_PRESETS: [u16; 5] = [0, 75, 100, 125, 150];

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    #[default]
    Assistant,
    Chat,
}

/// Controls whether the Agent may request operating-system actions at all.
/// The mode only decides in-app confirmation; it can never bypass macOS or
/// Windows system permissions.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionMode {
    /// No operation tools are registered for the Agent.
    Disabled,
    /// Sensitive operations are confirmed every time.
    #[default]
    Standard,
    /// Previously trusted folders, domains and applications are not confirmed again.
    Trust,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Settings {
    pub version: u8,
    pub model: String,
    pub privacy_mode: bool,
    #[serde(default)]
    pub search_proxy: String,
    #[serde(default = "default_voice_enabled")]
    pub voice_enabled: bool,
    #[serde(default)]
    pub interaction_mode: InteractionMode,
    #[serde(default)]
    pub action_mode: ActionMode,
    #[serde(default = "default_pet_scale_percent")]
    pub pet_scale_percent: u16,
    /// User's birthday as `MM-DD`, or `None` when unset.
    #[serde(default)]
    pub birthday: Option<String>,
    /// How Phoebe addresses the user. Defaults to 「漂泊者」.
    #[serde(default = "default_user_address")]
    pub user_address: String,
    /// Internal state: local date (`MM-DD`) of the last birthday greeting, so
    /// it is only sent once per day. Not shown in the settings UI.
    #[serde(default)]
    pub last_birthday_greeting: Option<String>,
}

fn default_voice_enabled() -> bool {
    true
}

fn default_user_address() -> String {
    DEFAULT_USER_ADDRESS.to_owned()
}

fn default_pet_scale_percent() -> u16 {
    DEFAULT_PET_SCALE_PERCENT
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            model: DEFAULT_MODEL.into(),
            privacy_mode: false,
            search_proxy: String::new(),
            voice_enabled: true,
            interaction_mode: InteractionMode::Assistant,
            action_mode: ActionMode::Standard,
            pet_scale_percent: DEFAULT_PET_SCALE_PERCENT,
            birthday: None,
            user_address: DEFAULT_USER_ADDRESS.to_owned(),
            last_birthday_greeting: None,
        }
    }
}

#[derive(Default)]
pub struct SettingsStore {
    current: Mutex<Settings>,
    load_error: Mutex<Option<String>>,
}

pub fn valid_model(model: &str) -> bool {
    matches!(
        model,
        "deepseek-v4-flash" | "deepseek-v4-pro" | "deepseek-v4-flash-vision-exp"
    )
}

pub fn valid_search_proxy(proxy: &str) -> bool {
    if proxy.is_empty() {
        return true;
    }
    ["http://127.0.0.1:", "http://localhost:"]
        .iter()
        .any(|prefix| {
            proxy
                .strip_prefix(prefix)
                .and_then(|port| port.parse::<u16>().ok())
                .is_some_and(|port| port > 0)
        })
}

pub fn valid_pet_scale_percent(percent: u16) -> bool {
    PET_SCALE_PRESETS.contains(&percent)
}

/// Validates a user-chosen address/nickname (1-24 chars, no control chars).
pub fn valid_user_address(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= 24 && !trimmed.chars().any(char::is_control)
}

/// Validates a `MM-DD` birthday string (month 1-12, day 1-31).
pub fn valid_birthday(value: &str) -> bool {
    if !value.is_ascii() || value.len() != 5 || &value[2..3] != "-" {
        return false;
    }
    let month = value[0..2].parse::<u32>().ok();
    let day = value[3..5].parse::<u32>().ok();
    matches!((month, day), (Some(m), Some(d)) if (1..=12).contains(&m) && (1..=31).contains(&d))
}

/// Today's local date as `MM-DD`.
pub fn today_month_day() -> String {
    chrono::Local::now().format("%m-%d").to_string()
}

fn normalize_birthday(value: Option<String>) -> Result<Option<String>, String> {
    match value {
        None => Ok(None),
        Some(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else if valid_birthday(trimmed) {
                Ok(Some(trimmed.to_owned()))
            } else {
                Err("生日格式应为 MM-DD，例如 03-15".into())
            }
        }
    }
}

fn validate(settings: &Settings) -> Result<(), String> {
    if settings.version != 1
        || !valid_model(&settings.model)
        || !valid_search_proxy(&settings.search_proxy)
        || !valid_pet_scale_percent(settings.pet_scale_percent)
        || !valid_user_address(&settings.user_address)
        || settings
            .birthday
            .as_deref()
            .is_some_and(|value| !valid_birthday(value))
    {
        return Err("设置文件包含不支持的版本、模型或搜索代理".into());
    }
    Ok(())
}

fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|_| "无法取得应用数据目录")?
        .join("settings.json"))
}

impl SettingsStore {
    pub fn load(&self, app: &AppHandle) -> Result<(), String> {
        let path = settings_file(app)?;
        let settings = match fs::read(path) {
            Ok(bytes) => {
                if bytes.len() > 4096 {
                    return Err("设置文件过大".into());
                }
                let value: Settings =
                    serde_json::from_slice(&bytes).map_err(|_| "设置文件不可读取")?;
                validate(&value)?;
                value
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(_) => return Err("无法读取设置文件".into()),
        };
        *self.current.lock().map_err(|_| "设置状态不可用")? = settings;
        *self.load_error.lock().map_err(|_| "设置状态不可用")? = None;
        Ok(())
    }

    pub fn record_load_error(&self, error: String) {
        if let Ok(mut value) = self.load_error.lock() {
            *value = Some(error);
        }
    }

    pub fn get(&self) -> Result<Settings, String> {
        if let Some(error) = self
            .load_error
            .lock()
            .map_err(|_| "设置状态不可用")?
            .clone()
        {
            return Err(error);
        }
        self.current
            .lock()
            .map(|value| value.clone())
            .map_err(|_| "设置状态不可用".into())
    }

    pub fn update(
        &self,
        app: &AppHandle,
        model: String,
        privacy_mode: bool,
        search_proxy: String,
        voice_enabled: bool,
        interaction_mode: InteractionMode,
        action_mode: ActionMode,
        birthday: Option<String>,
        user_address: String,
    ) -> Result<Settings, String> {
        let mut next = self.get()?;
        next.model = model;
        next.privacy_mode = privacy_mode;
        next.search_proxy = search_proxy;
        next.voice_enabled = voice_enabled;
        next.interaction_mode = interaction_mode;
        next.action_mode = action_mode;
        next.birthday = normalize_birthday(birthday)?;
        let address = user_address.trim();
        next.user_address = if address.is_empty() {
            DEFAULT_USER_ADDRESS.to_owned()
        } else {
            address.to_owned()
        };
        self.persist(app, next)
    }

    /// Records that today's birthday greeting has been sent.
    pub fn mark_birthday_greeted(&self, app: &AppHandle, date: String) -> Result<(), String> {
        let mut next = self.get()?;
        next.last_birthday_greeting = Some(date);
        self.persist(app, next).map(|_| ())
    }

    pub fn update_quick_preferences(
        &self,
        app: &AppHandle,
        interaction_mode: InteractionMode,
        voice_enabled: bool,
    ) -> Result<Settings, String> {
        let mut next = self.get()?;
        next.interaction_mode = interaction_mode;
        next.voice_enabled = voice_enabled;
        self.persist(app, next)
    }

    pub fn update_pet_scale(
        &self,
        app: &AppHandle,
        pet_scale_percent: u16,
    ) -> Result<Settings, String> {
        let mut next = self.get()?;
        next.pet_scale_percent = pet_scale_percent;
        self.persist(app, next)
    }

    fn persist(&self, app: &AppHandle, next: Settings) -> Result<Settings, String> {
        validate(&next)?;
        let path = settings_file(app)?;
        fs::create_dir_all(path.parent().ok_or("应用数据目录不可用")?)
            .map_err(|_| "无法建立应用数据目录")?;
        let data = serde_json::to_vec(&next).map_err(|_| "无法编码设置")?;
        let temp = path.with_extension("json.tmp");
        let mut current = self.current.lock().map_err(|_| "设置状态不可用")?;
        fs::write(&temp, data).map_err(|_| "无法写入设置")?;
        if let Err(_) = fs::rename(&temp, &path) {
            let _ = fs::remove_file(&temp);
            return Err("无法保存设置".into());
        }
        *current = next.clone();
        *self.load_error.lock().map_err(|_| "设置状态不可用")? = None;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        valid_birthday, valid_model, valid_pet_scale_percent, valid_search_proxy, Settings,
        DEFAULT_MODEL, DEFAULT_PET_SCALE_PERCENT,
    };

    #[test]
    fn defaults_are_explicit_and_model_allowlisted() {
        let settings = Settings::default();
        assert_eq!(settings.model, DEFAULT_MODEL);
        assert!(!settings.privacy_mode);
        assert!(settings.search_proxy.is_empty());
        assert!(settings.voice_enabled);
        assert_eq!(settings.interaction_mode, super::InteractionMode::Assistant);
        assert_eq!(settings.pet_scale_percent, DEFAULT_PET_SCALE_PERCENT);
        assert!(valid_model("deepseek-v4-pro"));
        assert!(valid_model("deepseek-v4-flash-vision-exp"));
        assert!(!valid_model("../../../agent"));
        assert!(valid_search_proxy("http://127.0.0.1:7897"));
        assert!(!valid_search_proxy("http://example.com:7897"));
        assert!(!valid_search_proxy("http://127.0.0.1:7897/other"));
        assert!(valid_pet_scale_percent(75));
        assert!(valid_pet_scale_percent(150));
        assert!(valid_pet_scale_percent(0));
        assert!(!valid_pet_scale_percent(50));
        assert!(!valid_pet_scale_percent(110));

        assert!(valid_birthday("03-15"));
        assert!(valid_birthday("12-31"));
        assert!(!valid_birthday("13-01"));
        assert!(!valid_birthday("00-10"));
        assert!(!valid_birthday("3-5"));
        assert!(!valid_birthday("03/15"));

        assert_eq!(Settings::default().user_address, super::DEFAULT_USER_ADDRESS);
        assert!(super::valid_user_address("漂泊者"));
        assert!(super::valid_user_address("小芳"));
        assert!(!super::valid_user_address("   "));
        assert!(!super::valid_user_address(&"x".repeat(25)));
        assert!(!super::valid_user_address("a\nb"));

        let migrated: Settings = serde_json::from_str(
            r#"{"version":1,"model":"deepseek-v4-flash","privacy_mode":false,"search_proxy":""}"#,
        )
        .expect("version one settings should remain readable");
        assert!(migrated.voice_enabled);
        assert_eq!(migrated.interaction_mode, super::InteractionMode::Assistant);
        assert_eq!(migrated.action_mode, super::ActionMode::Standard);
        assert_eq!(migrated.pet_scale_percent, DEFAULT_PET_SCALE_PERCENT);
    }
}
