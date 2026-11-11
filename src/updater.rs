//! Søger efter nye versioner på GitHub og installerer dem (#26).
//!
//! Søgningen kører i en tråd, så brugerfladen ikke venter på nettet. Er
//! programmet installeret med installeren på Windows, hentes den nye
//! `KronoLog-Setup-x.y.z.exe` og køres uden spørgsmål; ellers (bærbar .exe,
//! Linux) åbnes udgivelsessiden.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui;

const LATEST_API: &str = "https://api.github.com/repos/Stensdal87/kronolog/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/Stensdal87/kronolog/releases/latest";
/// Første automatiske søgning lidt efter opstart, så den ikke forsinker den.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(15);
/// Derefter én gang i døgnet, hvis programmet står og kører.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// En udgivelse på GitHub.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// F.eks. "0.11.0".
    pub version: String,
    /// Siden på GitHub med udgivelsen.
    pub page: String,
    /// Windows-installeren, hvis den er med.
    pub setup_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Downloading(Release),
    Failed(String),
}

enum Message {
    Checked(Result<Option<Release>, String>),
    Downloaded(Result<PathBuf, String>),
}

pub struct Updater {
    pub status: Status,
    started: Instant,
    last_check: Option<Instant>,
    /// Søgningen blev startet fra indstillingerne; så vises fejl også.
    manual: bool,
    /// Banneret er lukket for denne version.
    pub dismissed: bool,
    rx: Option<Receiver<Message>>,
}

impl Updater {
    pub fn new() -> Self {
        Self {
            status: Status::Idle,
            started: Instant::now(),
            last_check: None,
            manual: false,
            dismissed: false,
            rx: None,
        }
    }

    /// Kaldes hver frame. Starter automatiske søgninger og samler svar op.
    /// Giver stien til en hentet installer, som nu skal køres.
    pub fn poll(&mut self, ctx: &egui::Context, automatic: bool) -> Option<PathBuf> {
        if let Some(rx) = &self.rx
            && let Ok(message) = rx.try_recv()
        {
            self.rx = None;
            match message {
                Message::Checked(Ok(Some(release))) => {
                    if self.status != Status::Available(release.clone()) {
                        self.dismissed = false;
                    }
                    self.status = Status::Available(release);
                }
                Message::Checked(Ok(None)) => self.status = Status::UpToDate,
                // En automatisk søgning uden net skal ikke forstyrre.
                Message::Checked(Err(e)) => self.status = if self.manual { Status::Failed(e) } else { Status::Idle },
                Message::Downloaded(Ok(path)) => return Some(path),
                Message::Downloaded(Err(e)) => self.status = Status::Failed(e),
            }
        }
        if automatic && self.rx.is_none() && !matches!(self.status, Status::Downloading(_)) {
            let due = match self.last_check {
                None => self.started.elapsed() >= FIRST_CHECK_AFTER,
                Some(at) => at.elapsed() >= CHECK_EVERY,
            };
            if due {
                self.check(ctx, false);
            } else {
                let wait = match self.last_check {
                    None => FIRST_CHECK_AFTER.saturating_sub(self.started.elapsed()),
                    Some(at) => CHECK_EVERY.saturating_sub(at.elapsed()),
                };
                ctx.request_repaint_after(wait.max(Duration::from_secs(1)));
            }
        }
        None
    }

    /// Spørg GitHub om den nyeste version.
    pub fn check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.rx.is_some() {
            return;
        }
        self.manual = manual;
        self.last_check = Some(Instant::now());
        if manual || !matches!(self.status, Status::Available(_)) {
            self.status = Status::Checking;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = fetch_latest().map(|r| is_newer(&r.version, env!("CARGO_PKG_VERSION")).then_some(r));
            let _ = tx.send(Message::Checked(result));
            ctx.request_repaint();
        });
    }

    /// Hent installeren til den nye version. Svaret kommer fra `poll`.
    pub fn download(&mut self, ctx: &egui::Context) {
        let Status::Available(release) = &self.status else { return };
        let Some(url) = release.setup_url.clone() else { return };
        let release = release.clone();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        let target = std::env::temp_dir().join(format!("KronoLog-Setup-{}.exe", release.version));
        self.status = Status::Downloading(release);
        std::thread::spawn(move || {
            let result = download(&url, &target).map(|()| target);
            let _ = tx.send(Message::Downloaded(result));
            ctx.request_repaint();
        });
    }

    /// Den ledige nye version, hvis der er en.
    pub fn available(&self) -> Option<&Release> {
        match &self.status {
            Status::Available(r) | Status::Downloading(r) => Some(r),
            _ => None,
        }
    }
}

/// Kan opdateringen installeres direkte? Kun når programmet er installeret med
/// installeren (Inno Setup lægger `unins000.exe` ved siden af programmet).
pub fn can_install(release: &Release) -> bool {
    cfg!(windows)
        && release.setup_url.is_some()
        && std::env::current_exe().is_ok_and(|exe| exe.with_file_name("unins000.exe").exists())
}

/// Starter installeren stille. Den lukker ikke selv programmet; det skal afslutte
/// bagefter. `/AUTOUPDATE=1` får installeren til at starte programmet igen.
pub fn run_installer(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new(path)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-", "/AUTOUPDATE=1"])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Åbner en side i browseren.
pub fn open_page(url: &str) {
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    let _ = result;
}

fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(120)));
    // Windows: Windows' egen TLS og certifikatlager (se Cargo.toml).
    #[cfg(windows)]
    let config = config.tls_config(
        ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build(),
    );
    config.build().new_agent()
}

fn fetch_latest() -> Result<Release, String> {
    let text = agent()
        .get(LATEST_API)
        .header("User-Agent", concat!("KronoLog/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    parse_release(&text)
}

fn parse_release(json: &str) -> Result<Release, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tag = value["tag_name"].as_str().ok_or("tag_name mangler")?;
    let setup_url = value["assets"].as_array().and_then(|assets| {
        assets.iter().find_map(|a| {
            let name = a["name"].as_str()?;
            // Det gamle navn tages også, hvis en udgivelse kun har det.
            let setup = (name.starts_with("KronoLog-Setup-") || name.starts_with("TimeSnapper-Setup-")) && name.ends_with(".exe");
            setup.then(|| a["browser_download_url"].as_str().map(str::to_owned)).flatten()
        })
    });
    Ok(Release {
        version: tag.trim_start_matches('v').to_owned(),
        page: value["html_url"].as_str().unwrap_or(RELEASES_PAGE).to_owned(),
        setup_url,
    })
}

fn download(url: &str, target: &std::path::Path) -> Result<(), String> {
    let mut response = agent()
        .get(url)
        .header("User-Agent", concat!("KronoLog/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?;
    // Først til en midlertidig fil, så en afbrudt hentning ikke køres.
    let partial = target.with_extension("part");
    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    std::io::copy(&mut response.body_mut().as_reader(), &mut file).map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&partial, target).map_err(|e| e.to_string())
}

/// "0.11.0" er nyere end "0.10.0" (tal for tal, ikke som tekst).
fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split(['.', '-']).map_while(|p| p.parse().ok()).collect() };
    parse(candidate) > parse(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("0.11.0", "0.10.0"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0.0", "0.10.3"));
        assert!(!is_newer("0.10.0", "0.10.0"));
        assert!(!is_newer("0.9.9", "0.10.0"));
    }

    #[test]
    fn reads_version_and_installer_from_github_answer() {
        let json = r#"{"tag_name":"v0.11.0","html_url":"https://github.com/x/releases/tag/v0.11.0","assets":[
            {"name":"KronoLog-0.11.0-linux-x86_64.tar.gz","browser_download_url":"https://x/linux.tar.gz"},
            {"name":"KronoLog-portable-0.11.0.exe","browser_download_url":"https://x/portable.exe"},
            {"name":"KronoLog-Setup-0.11.0.exe","browser_download_url":"https://x/setup.exe"}]}"#;
        let release = parse_release(json).unwrap();
        assert_eq!(release.version, "0.11.0");
        assert_eq!(release.page, "https://github.com/x/releases/tag/v0.11.0");
        assert_eq!(release.setup_url.as_deref(), Some("https://x/setup.exe"));

        // En gammel udgivelse med det tidligere navn findes også.
        let old = r#"{"tag_name":"v0.14.0","assets":[{"name":"TimeSnapper-Setup-0.14.0.exe","browser_download_url":"https://x/old.exe"}]}"#;
        assert_eq!(parse_release(old).unwrap().setup_url.as_deref(), Some("https://x/old.exe"));
    }
}
