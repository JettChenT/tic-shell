use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub daemon: DaemonConfig,
    pub heartbeat: HeartbeatConfig,
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub data_root: PathBuf,
    pub memory_root: PathBuf,
    pub workdir_root: PathBuf,
    pub adapter_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HeartbeatConfig {
    #[serde(rename = "window-l1")]
    pub window_l1: WindowL1Config,
    #[serde(rename = "workspace-l2")]
    pub workspace_l2: WorkspaceL2Config,
    #[serde(rename = "screenshot-diff")]
    pub screenshot_diff: ScreenshotDiffConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowL1Config {
    pub enabled: bool,
    pub screenshot_interval: String,
    pub initial_screenshot_count: usize,
    pub initial_timeout: String,
    pub ongoing_screenshot_count: usize,
    pub ongoing: OngoingConfig,
    pub buffer_max: usize,
    pub summary_duration_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OngoingConfig {
    pub interval: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceL2Config {
    pub enabled: bool,
    pub interval: String,
    pub summary_duration_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScreenshotDiffConfig {
    pub enabled: bool,
    pub algorithm: String,
    pub threshold: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    pub enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        let home = home_dir();
        let tic = home.join(".tic");
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let workdir_root = std::env::var_os("TIC_CODEX_WORKDIR_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| runtime.join("tic-shell").join("codex-workspaces"));
        Self {
            daemon: DaemonConfig {
                data_root: tic.clone(),
                memory_root: tic.join("memory"),
                workdir_root,
                adapter_command: std::env::var("TIC_CODEX_ACP_COMMAND")
                    .ok()
                    .filter(|s| !s.trim().is_empty()),
            },
            heartbeat: HeartbeatConfig::default(),
            mcp: McpConfig::default(),
        }
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Config::default().daemon
    }
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            window_l1: WindowL1Config::default(),
            workspace_l2: WorkspaceL2Config::default(),
            screenshot_diff: ScreenshotDiffConfig::default(),
        }
    }
}

impl Default for WindowL1Config {
    fn default() -> Self {
        Self {
            enabled: true,
            screenshot_interval: "1s".to_string(),
            initial_screenshot_count: 10,
            initial_timeout: "20s".to_string(),
            ongoing_screenshot_count: 30,
            ongoing: OngoingConfig::default(),
            buffer_max: 100,
            summary_duration_label: "10min".to_string(),
        }
    }
}

impl Default for OngoingConfig {
    fn default() -> Self {
        Self {
            interval: "60s".to_string(),
        }
    }
}

impl Default for WorkspaceL2Config {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: "60min".to_string(),
            summary_duration_label: "60min".to_string(),
        }
    }
}

impl Default for ScreenshotDiffConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            algorithm: "gradient-hash".to_string(),
            threshold: 8,
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Config {
    pub fn load_or_create() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            let config = Self::default();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            fs::write(&path, toml::to_string_pretty(&config)?)
                .with_context(|| format!("write {}", path.display()))?;
            return Ok(config);
        }

        let content =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        toml::from_str(&content).with_context(|| format!("parse {}", path.display()))
    }

    pub fn history_dir(&self) -> PathBuf {
        self.daemon.memory_root.join("history")
    }
}

pub fn config_path() -> PathBuf {
    std::env::var_os("TIC_DAEMON_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".tic").join("config.toml"))
}

pub fn parse_duration(value: &str) -> Duration {
    let trimmed = value.trim();
    let split = trimmed
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (num, unit) = trimmed.split_at(split);
    let amount = num.parse::<u64>().unwrap_or(0);
    match unit.trim() {
        "ms" => Duration::from_millis(amount),
        "m" | "min" | "mins" => Duration::from_secs(amount * 60),
        "h" | "hr" | "hrs" => Duration::from_secs(amount * 60 * 60),
        _ => Duration::from_secs(amount),
    }
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new("/tmp").to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_duration_units() {
        assert_eq!(parse_duration("20s"), Duration::from_secs(20));
        assert_eq!(parse_duration("60min"), Duration::from_secs(3600));
        assert_eq!(parse_duration("250ms"), Duration::from_millis(250));
    }

    #[test]
    fn default_config_contains_requested_ongoing_key() {
        let config = Config::default();
        assert_eq!(config.heartbeat.window_l1.ongoing.interval, "60s");
    }
}
