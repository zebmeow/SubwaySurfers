//! Where the game's files live at run time.
//!
//! * Portable (a distribution folder): `data/`, `assets/` and `site/` (the
//!   original game's files) next to the executable, saves in `save/`.
//! * Development: the repository (`ss_port/` and `oracle/site/`), found from
//!   the crate's build-time location.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Install {
    /// The original game's files (`oracle/site` in the repository).
    pub site: PathBuf,
    /// The port's own folder: `data/`, `assets/`, `save/`.
    pub port: PathBuf,
    /// The repository root (development only: oracle traces).
    pub repo: Option<PathBuf>,
}

impl Install {
    pub fn locate() -> Self {
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            if dir.join("data/anim_clips.json").is_file() && dir.join("site").is_dir() {
                return Self { site: dir.join("site"), port: dir, repo: None };
            }
        }
        Self::repo(&PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/..")))
    }
    pub fn repo(root: &Path) -> Self {
        Self { site: root.join("oracle/site"), port: root.join("ss_port"), repo: Some(root.to_path_buf()) }
    }

    /// The save file: the user's application data folder
    /// (`%APPDATA%\SubwaySurfers` on Windows, `~/Library/Application
    /// Support/SubwaySurfers` on macOS, `$XDG_DATA_HOME` or
    /// `~/.local/share/SubwaySurfers` elsewhere), else `save/` next to the
    /// game. A save left in `save/` by an earlier version moves over once.
    pub fn save_file(&self) -> PathBuf {
        let local = self.port.join("save/GameSettings.json");
        let Some(dir) = user_data_dir() else { return local };
        let file = dir.join("SubwaySurfers").join("GameSettings.json");
        if !file.is_file() && local.is_file() {
            if let Some(parent) = file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::copy(&local, &file).is_err() {
                return local;
            }
        }
        file
    }
}

fn user_data_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        env("APPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share")))
    }
}

/// Write `bytes` to `path` atomically: a `.tmp` sibling, flushed, then
/// renamed over the old file (a crash leaves the old or the new save, never
/// half of one).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn atomic_write_replaces_the_file() {
        let dir = std::env::temp_dir().join(format!("ss_port_save_test_{}", std::process::id()));
        let p = dir.join("GameSettings.json");
        super::write_atomic(&p, b"{\"a\":1}").unwrap();
        super::write_atomic(&p, b"{\"a\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{\"a\":2}");
        assert!(!dir.join("GameSettings.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
