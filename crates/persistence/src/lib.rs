use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationEntry {
    pub annotation: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationFile {
    #[serde(default)]
    pub workspaces: BTreeMap<String, AnnotationEntry>,
}

#[derive(Debug, Clone)]
pub struct AnnotationStore {
    path: PathBuf,
    file: AnnotationFile,
}

impl AnnotationStore {
    pub fn load_default() -> Result<Self> {
        Self::load(default_annotation_path())
    }

    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents)
                .with_context(|| format!("failed to parse {}", path.display()))?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => AnnotationFile::default(),
            Err(err) => {
                return Err(err).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        Ok(Self { path, file })
    }

    pub fn annotation_for_workspace(&self, workspace_id: u64) -> &str {
        self.file
            .workspaces
            .get(&services::niri::workspace_key(workspace_id))
            .map(|entry| entry.annotation.as_str())
            .unwrap_or("")
    }

    pub fn set_annotation(&mut self, workspace_id: u64, annotation: &str) -> Result<()> {
        let key = services::niri::workspace_key(workspace_id);
        let trimmed = annotation.trim();
        if trimmed.is_empty() {
            self.file.workspaces.remove(&key);
        } else {
            self.file.workspaces.insert(
                key,
                AnnotationEntry {
                    annotation: trimmed.to_string(),
                    updated_at: now_utc_string(),
                },
            );
        }
        self.save()
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let contents = serde_json::to_string_pretty(&self.file)?;
        fs::write(&self.path, contents)
            .with_context(|| format!("failed to write {}", self.path.display()))
    }
}

pub fn default_annotation_path() -> PathBuf {
    if let Some(state_home) = std::env::var_os("XDG_STATE_HOME") {
        Path::new(&state_home).join("lnx/workspaces.json")
    } else {
        let home = std::env::var_os("HOME").unwrap_or_else(|| "/home/jettc".into());
        Path::new(&home).join(".local/state/lnx/workspaces.json")
    }
}

fn now_utc_string() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_existing_qml_annotation_shape() {
        let file: AnnotationFile = serde_json::from_str(
            r#"{"workspaces":{"niri:workspace:7":{"annotation":"ship it","updatedAt":"2026-05-11T00:00:00Z"}}}"#,
        )
        .unwrap();

        assert_eq!(
            file.workspaces.get("niri:workspace:7").unwrap().annotation,
            "ship it"
        );
    }
}
