//! Self-update from GitHub Releases.
//!
//! Each release carries a `latest.json` manifest listing one archive per
//! platform plus its minisign signature (Tauri's updater key format). Archives
//! are verified against the public key compiled in here before anything on
//! disk is touched, then swapped in place: the `.app` bundle on macOS, the
//! `upskill-app` and `upskill` binaries on Linux. Paths stay stable, so MCP
//! registrations keep working across updates.

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::Paths;

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const MANIFEST_URL: &str = "https://github.com/galengreen/UpSkill/releases/latest/download/latest.json";
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEEwQzcyNTQ0MDVDMTFEMDQKUldRRUhjRUZSQ1hIb0V4eG9MWndQRzU0UjVidkI2NzZoU2ozS08ybEF0QzF5MFpPU09tL1piM1AK";
/// Linux installs replace these two files in the binary folder.
const LINUX_BINS: [&str; 2] = ["upskill-app", "upskill"];

#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    pub_date: String,
    platforms: BTreeMap<String, Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    url: String,
    signature: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct Check {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub date: String,
    /// An update already installed on disk, waiting for a restart.
    pub pending: Option<String>,
    /// Why this copy can't update itself (dev build, read-only location, no build for this platform).
    pub blocked: Option<String>,
}

pub fn platform() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

/// `1.2.10` > `1.2.9`; a leading `v` and pre-release suffixes are ignored.
pub fn newer(candidate: &str, than: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v').split(['-', '+']).next().unwrap_or("").split('.').map(|n| n.parse().unwrap_or(0)).collect()
    };
    parse(candidate) > parse(than)
}

fn manifest_url() -> String {
    std::env::var("UPSKILL_UPDATE_URL").unwrap_or_else(|_| MANIFEST_URL.into())
}

fn curl(url: &str, out: Option<&Path>, timeout: u32) -> Result<Vec<u8>> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--retry", "2", "--max-time", &timeout.to_string(), url]);
    if let Some(p) = out {
        cmd.arg("-o").arg(p);
    }
    let o = cmd.output().context("running curl")?;
    if !o.status.success() {
        bail!("download failed: {}", String::from_utf8_lossy(&o.stderr).trim());
    }
    Ok(o.stdout)
}

fn fetch_manifest() -> Result<Manifest> {
    let body = curl(&manifest_url(), None, 20)?;
    serde_json::from_slice(&body).context("reading the release manifest")
}

fn verify(data: &[u8], signature_b64: &str) -> Result<()> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let pk = String::from_utf8(b64.decode(PUBKEY)?)?;
    let pk = minisign_verify::PublicKey::decode(&pk).map_err(|e| anyhow!("bad public key: {e}"))?;
    let sig = String::from_utf8(b64.decode(signature_b64.trim()).context("decoding signature")?)?;
    let sig = minisign_verify::Signature::decode(&sig).map_err(|e| anyhow!("bad signature: {e}"))?;
    pk.verify(data, &sig, true).map_err(|_| anyhow!("signature check failed; the download was not published by UpSkill"))
}

/// What gets replaced: the `.app` bundle on macOS, the binary folder on Linux.
fn target() -> Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    if exe.components().any(|c| c.as_os_str() == "target") {
        bail!("this is a development build; update it with git pull and a rebuild");
    }
    let target = if cfg!(target_os = "macos") {
        exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf).ok_or_else(|| anyhow!("not running from an UpSkill.app bundle"))?
    } else {
        let dir = exe.parent().ok_or_else(|| anyhow!("no binary folder"))?.to_path_buf();
        if !LINUX_BINS.iter().all(|b| dir.join(b).is_file()) {
            bail!("expected upskill-app and upskill side by side in {}", dir.display());
        }
        dir
    };
    let parent = if cfg!(target_os = "macos") { target.parent().unwrap_or(&target) } else { &target };
    let probe = parent.join(".upskill-write-test");
    fs::write(&probe, b"").with_context(|| format!("{} isn't writable", parent.display()))?;
    let _ = fs::remove_file(probe);
    Ok(target)
}

/// An installed-but-not-yet-running version, recorded so a running app doesn't
/// install the same update again and can offer a restart.
fn pending_file(paths: &Paths) -> PathBuf {
    paths.root.join("update-pending")
}

pub fn pending(paths: &Paths) -> Option<String> {
    let v = fs::read_to_string(pending_file(paths)).ok()?.trim().to_string();
    if newer(&v, CURRENT) {
        Some(v)
    } else {
        let _ = fs::remove_file(pending_file(paths));
        None
    }
}

pub fn check(paths: &Paths) -> Result<Check> {
    let m = fetch_manifest()?;
    let pending = pending(paths);
    let have = pending.clone().unwrap_or_else(|| CURRENT.to_string());
    let blocked = match (m.platforms.contains_key(&platform()), target()) {
        (false, _) => Some(format!("no build for {} in this release", platform())),
        (_, Err(e)) => Some(format!("{e:#}")),
        _ => None,
    };
    Ok(Check {
        current: CURRENT.into(),
        available: newer(&m.version, &have),
        latest: m.version.trim_start_matches('v').into(),
        notes: m.notes,
        date: m.pub_date,
        pending,
        blocked,
    })
}

/// Download, verify and swap in the latest release. Returns the installed
/// version; the running process keeps the old code until it restarts.
pub fn install(paths: &Paths) -> Result<String> {
    let m = fetch_manifest()?;
    let version = m.version.trim_start_matches('v').to_string();
    let have = pending(paths).unwrap_or_else(|| CURRENT.to_string());
    if !newer(&version, &have) {
        bail!("already up to date ({have})");
    }
    let asset = m.platforms.get(&platform()).cloned().ok_or_else(|| anyhow!("no build for {} in release {version}", platform()))?;
    let target = target()?;

    let archive = paths.tmp().join(format!("update-{version}.tar.gz"));
    curl(&asset.url, Some(&archive), 600)?;
    let data = fs::read(&archive)?;
    let verified = verify(&data, &asset.signature);
    if verified.is_err() {
        let _ = fs::remove_file(&archive);
    }
    verified?;

    // Unpack next to the target so the final swap is a same-volume rename.
    let parent = if cfg!(target_os = "macos") { target.parent().unwrap().to_path_buf() } else { target.clone() };
    let staging = parent.join(format!(".upskill-update-{version}"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let result = (|| -> Result<()> {
        let o = Command::new("tar").arg("-xzf").arg(&archive).arg("-C").arg(&staging).output()?;
        if !o.status.success() {
            bail!("unpacking failed: {}", String::from_utf8_lossy(&o.stderr).trim());
        }
        if cfg!(target_os = "macos") {
            swap_app(&staging.join("UpSkill.app"), &target)
        } else {
            swap_bins(&staging, &target)
        }
    })();
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_file(&archive);
    result?;
    fs::write(pending_file(paths), &version)?;
    Ok(version)
}

fn swap_app(new_app: &Path, app: &Path) -> Result<()> {
    if !new_app.join("Contents/MacOS/upskill-app").is_file() {
        bail!("the update archive doesn't contain UpSkill.app");
    }
    let old = app.with_file_name(".UpSkill.app.old");
    let _ = fs::remove_dir_all(&old);
    fs::rename(app, &old).context("moving the current app aside")?;
    if let Err(e) = fs::rename(new_app, app) {
        let _ = fs::rename(&old, app);
        return Err(e).context("moving the new app into place");
    }
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

fn swap_bins(staging: &Path, dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for b in LINUX_BINS {
        if !staging.join(b).is_file() {
            bail!("the update archive doesn't contain {b}");
        }
    }
    for b in LINUX_BINS {
        let src = staging.join(b);
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755))?;
        // Renaming over a running binary is safe: the old inode lives on until exit.
        fs::rename(&src, dir.join(b)).with_context(|| format!("replacing {b}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("v0.10.0", "0.9.0"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.2.0-beta", "0.2.0"));
        assert!(!newer("0.1.0", "0.2.0"));
    }

    /// Signed with the release key via `tauri signer sign`.
    const HELLO_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRRUhjRUZSQ1hIb0Iwd2NUZUhLTkhUejdpN0NPbjgreHN0VWpIUUdZY0MzMUJsK0FHSEpDWTJYSjlZV2ZsOHRSZ2x1ZEFCRVJSV3pKZ3RneXMvL25NTUt1citxL3E3NEFnPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkzMTczNTc4CWZpbGU6c2lndGVzdC50eHQKMXJGb0dzd3lqQ0xzUmtWaE5RTlI5cGVsZFhzQ0lnMHhnSUlQOVBkSWh3TGdlZ0NBK1BEMC9pV0I4N2VpM0lTamV0cS9MYzhMVUEzRVB6MlErK1RWQ3c9PQo=";

    #[test]
    fn verifies_release_signatures() {
        assert!(verify(b"hello\n", HELLO_SIG).is_ok());
        assert!(verify(b"hellO\n", HELLO_SIG).is_err());
        assert!(verify(b"hello\n", "AAAA").is_err());
    }
}
