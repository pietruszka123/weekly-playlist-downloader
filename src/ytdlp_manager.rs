use std::{
    fs::OpenOptions,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::SystemTime,
};

use reqwest::header::USER_AGENT;
use serde::{ Deserialize, Serialize };
use tokio::io::AsyncWriteExt;

use crate::create_data_dir;

enum OsType {
    Linux,
    Windows,
    Macos,
    Other,
}
impl Default for OsType {
    fn default() -> Self {
        let os = std::env::consts::OS;
        match os {
            "windows" => Self::Windows,
            "linux" => Self::Linux,
            "macos" => Self::Macos,
            _ => Self::Other,
        }
    }
}
enum Arch {
    X64,
    X86,
    Arm,
    Aarch64,
    Other,
}
impl Default for Arch {
    fn default() -> Self {
        let arch = std::env::consts::ARCH;
        match arch {
            "x86" => Self::X86,
            "x86_64" => Self::X64,
            "arm" => Self::Arm,
            "aarch64" => Self::Aarch64,
            _ => Self::Other,
        }
    }
}

#[derive(Deserialize, Debug)]
struct GithubeRelease {
    name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize, Debug)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct YtdlpManagerData {
    last_version: String,
    last_checked: Option<SystemTime>,
    path: Option<String>,
}

pub struct YtdlpManager {
    pub ytdlp_path: Option<PathBuf>,
    pub last_checked: Option<SystemTime>,
    pub version: String,
}
impl YtdlpManager {
    pub fn load_data(&mut self, path: Option<PathBuf>) -> anyhow::Result<()> {
        let path = path.unwrap_or(
            dirs::cache_dir().unwrap().join("music_player").join("ytdlp.json")
        );
        if path.exists() {
            let f = OpenOptions::new().read(true).open(path)?;
            let s: YtdlpManagerData = serde_json::from_reader(f)?;
            self.version = s.last_version;
            self.last_checked = s.last_checked;
            self.ytdlp_path = s.path.as_ref().map(|s| s.into());
        }
        Ok(())
    }
    pub fn save_data(&mut self, path: Option<PathBuf>) -> anyhow::Result<()> {
        let path = path.unwrap_or(
            dirs::cache_dir().unwrap().join("music_player").join("ytdlp.json")
        );

        let mut p = path.clone();
        p.pop();
        if !p.exists() {
            std::fs::create_dir(&p)?;
        }
        let mut f = OpenOptions::new().write(true).create(true).truncate(true).open(path)?;
        let s = serde_json::to_vec(
            &(YtdlpManagerData {
                last_version: self.version.clone(),
                path: self.ytdlp_path.clone().map(|s| s.to_str().unwrap().to_string()),
                last_checked: self.last_checked,
            })
        )?;
        // dbg!(&self.last_checked);
        f.write_all(&s)?;

        Ok(())
    }
    async fn download_exe(&self, asset: &GithubAsset) -> anyhow::Result<PathBuf> {
        let data_dir = create_data_dir()?;

        let mut req = reqwest::Client
            ::new()
            .get(&asset.browser_download_url)
            .header(USER_AGENT, "rust-reqwest")
            .send().await?
            .error_for_status()?;

        let mut options = tokio::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        let path = data_dir.join(&asset.name);
        let mut file = options.open(&path).await?;
        while let Some(bytes) = req.chunk().await? {
            let _ = file.write(&bytes[..]).await?;
        }
        file.sync_all().await?;
        if cfg!(target_os = "linux") {
            let mut perms = file.metadata().await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&path, perms).await?;
        }

        Ok(path)
    }
    fn query_asset<'a>(&self, assets: &'a [GithubAsset]) -> Option<&'a GithubAsset> {
        let os = OsType::default();
        let arch = Arch::default();
        let suffix = match (os, arch) {
            (OsType::Linux, Arch::Arm) => "linux_armv7l",
            (OsType::Linux, Arch::Aarch64) => "linux_aarch64",
            (OsType::Windows, Arch::X86) => "x86.exe",
            (OsType::Linux, _) => "linux",
            (OsType::Windows, _) => ".exe",
            (OsType::Macos, _) => "macos_legacy",
            (OsType::Other, _) => {
                panic!("Platform unsuported");
            }
        };

        let final_name = format!("yt-dlp_{suffix}");

        assets.iter().find(|a| a.name == final_name)
    }
    async fn fetch_latest_release(&self) -> anyhow::Result<GithubeRelease> {
        let url = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";

        let req = reqwest::Client::new().get(url).header(USER_AGENT, "rust-reqwest").send().await?;

        dbg!(&req);

        Ok(req.json::<GithubeRelease>().await?)
    }

    pub async fn update(&mut self) -> anyhow::Result<()> {
        if let Some(time) = self.last_checked {
            let now = SystemTime::now();
            if let Ok(diff) = now.duration_since(time) && diff.as_secs() < 60 * 60 * 24 {
                println!("Skipping Update");
                return Ok(());
            }
        }
        let release = self.fetch_latest_release().await?;
        self.last_checked = Some(SystemTime::now());
        if release.name == self.version {
            return Ok(());
        }

        let asset = self
            .query_asset(&release.assets)
            .ok_or(anyhow::anyhow!("Asset for current platform was not found"))?;
        let path = self.download_exe(asset).await?;
        self.ytdlp_path = Some(path);
        self.version = release.name.replace("yt-dlp_", "");

        self.save_data(None)?;
        Ok(())
    }
}
