use super::{ConfigError, ConfigSnapshot};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub enum ConfigSelection {
    Explicit {
        config: PathBuf,
        repository_root: PathBuf,
    },
    ExplicitFrom {
        config: PathBuf,
        source: PathBuf,
    },
    DiscoverFrom(PathBuf),
}

pub fn load(
    selection: ConfigSelection,
    require: bool,
) -> Result<Option<ConfigSnapshot>, ConfigError> {
    let (root, selected) = match selection {
        ConfigSelection::Explicit {
            config,
            repository_root,
        } => {
            let root = canonical(&repository_root)?;
            let config = canonical(&config)?;
            if !config.starts_with(&root) {
                return Err(ConfigError::new(
                    "CONFIG_OUTSIDE_REPOSITORY",
                    "explicit config is outside the repository",
                ));
            }
            (root, Some(config))
        }
        ConfigSelection::ExplicitFrom { config, source } => {
            let start = if source.is_dir() {
                source
            } else {
                source.parent().unwrap_or(Path::new(".")).to_path_buf()
            };
            let root = git_root(&start)?;
            let config = canonical(&config)?;
            if !config.starts_with(&root) {
                return Err(ConfigError::new(
                    "CONFIG_OUTSIDE_REPOSITORY",
                    "explicit config is outside the repository",
                ));
            }
            (root, Some(config))
        }
        ConfigSelection::DiscoverFrom(source) => {
            let start = if source.is_dir() {
                source
            } else {
                source.parent().unwrap_or(Path::new(".")).to_path_buf()
            };
            let root = git_root(&start)?;
            let mut cursor = canonical(&start)?;
            let mut found = None;
            loop {
                let candidate = cursor.join(".nexusmind.yaml");
                if candidate.is_file() {
                    found = Some(canonical(&candidate)?);
                    break;
                }
                if cursor == root {
                    break;
                }
                if !cursor.pop() {
                    break;
                }
            }
            (root, found)
        }
    };
    let Some(path) = selected else {
        if require {
            return Err(ConfigError::new(
                "CONFIG_NOT_FOUND",
                "no .nexusmind.yaml found inside the Git repository",
            ));
        }
        return Ok(None);
    };
    let bytes = fs::read(&path).map_err(|e| {
        ConfigError::new(
            "CONFIG_INVALID_SCHEMA",
            format!("could not read config: {e}"),
        )
    })?;
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| ConfigError::new("CONFIG_OUTSIDE_REPOSITORY", "config is outside repository"))?
        .to_string_lossy()
        .replace('\\', "/");
    ConfigSnapshot::from_bytes(&bytes, root, relative).map(Some)
}

fn canonical(path: &Path) -> Result<PathBuf, ConfigError> {
    path.canonicalize()
        .map_err(|e| ConfigError::new("CONFIG_NOT_FOUND", format!("could not resolve path: {e}")))
}

fn git_root(cwd: &Path) -> Result<PathBuf, ConfigError> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .map_err(|e| ConfigError::new("CONFIG_NOT_FOUND", format!("could not invoke git: {e}")))?;
    if !output.status.success() {
        return Err(ConfigError::new(
            "CONFIG_NOT_FOUND",
            "source path is not inside a Git repository",
        ));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| ConfigError::new("CONFIG_NOT_FOUND", "Git root is not UTF-8"))?;
    canonical(Path::new(text.trim()))
}
