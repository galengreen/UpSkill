use std::path::PathBuf;

/// Every location UpSkill reads or writes. `UPSKILL_HOME_DIR` swaps the user's
/// home (handy for tests); `UPSKILL_ROOT` moves the UpSkill data directory.
#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    pub root: PathBuf,
}

impl Paths {
    pub fn detect() -> Self {
        let home = std::env::var_os("UPSKILL_HOME_DIR")
            .map(PathBuf::from)
            .or_else(dirs::home_dir)
            .expect("could not determine home directory");
        let root = std::env::var_os("UPSKILL_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".upskill"));
        Paths { home, root }
    }

    pub fn library(&self) -> PathBuf {
        self.root.join("library")
    }
    pub fn objects(&self) -> PathBuf {
        self.root.join("objects")
    }
    pub fn blobs(&self) -> PathBuf {
        self.objects().join("blobs")
    }
    pub fn versions(&self) -> PathBuf {
        self.objects().join("versions")
    }
    pub fn state_file(&self) -> PathBuf {
        self.root.join("state.json")
    }
    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.json")
    }
    pub fn machines_file(&self) -> PathBuf {
        self.root.join("machines.json")
    }
    pub fn db(&self) -> PathBuf {
        self.root.join("usage.db")
    }
    pub fn lock_file(&self) -> PathBuf {
        self.root.join(".lock")
    }
    pub fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }
    pub fn remote(&self) -> PathBuf {
        self.root.join("remote")
    }
    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// Expand a leading `~/` against the (possibly overridden) home.
    pub fn expand(&self, p: &str) -> PathBuf {
        if let Some(rest) = p.strip_prefix("~/") {
            self.home.join(rest)
        } else if p == "~" {
            self.home.clone()
        } else {
            PathBuf::from(p)
        }
    }

    /// Render a path with `~` for display.
    pub fn display(&self, p: &std::path::Path) -> String {
        match p.strip_prefix(&self.home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => p.display().to_string(),
        }
    }
}
