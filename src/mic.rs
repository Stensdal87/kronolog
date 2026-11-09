//! Hvilke programmer der bruger mikrofonen lige nu (#27). Bruges til at tage
//! telefonen automatisk, når man er i et opkald i f.eks. Teams.
//!
//! Windows fører selv regnskab i registeret (det er det, der viser
//! mikrofonikonet i proceslinjen): en undernøgle pr. program under
//! `CapabilityAccessManager\ConsentStore\microphone`, hvor `LastUsedTimeStop` er 0,
//! mens programmet bruger mikrofonen. På Linux spørges PipeWire/PulseAudio med
//! `pactl`. Der spørges hvert andet sekund i en tråd.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use eframe::egui;

const POLL_EVERY: Duration = Duration::from_secs(2);

/// Hvad tråden har set.
#[derive(Clone, Default, PartialEq)]
struct Seen {
    /// Bruger mikrofonen nu.
    in_use: Vec<String>,
    /// Har brugt mikrofonen: på Windows alt, hvad Windows husker, på Linux hvad
    /// der er set, mens programmet har kørt. Til at vælge navnet i
    /// indstillingerne (#28).
    known: Vec<String>,
}

pub struct MicWatcher {
    rx: Receiver<Seen>,
    seen: Seen,
    /// Tråden spørger kun, når nogen har brug for svaret.
    wanted: Arc<AtomicBool>,
}

impl MicWatcher {
    pub fn start(ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let wanted = Arc::new(AtomicBool::new(false));
        let thread_wanted = wanted.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut last = Seen::default();
            loop {
                let mut seen = Seen { in_use: Vec::new(), known: last.known.clone() };
                if thread_wanted.load(Ordering::Relaxed) {
                    let (in_use, known) = microphone_apps();
                    seen.in_use = in_use;
                    seen.known.extend(known);
                    seen.known.extend(seen.in_use.iter().cloned());
                    seen.known.sort_by_key(|a| a.to_lowercase());
                    seen.known.dedup();
                }
                if seen != last {
                    if tx.send(seen.clone()).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                    last = seen;
                } else if !seen.in_use.is_empty() {
                    // Væk programmet, så længe mikrofonen er i brug: det skal nå
                    // forbi de 2 sekunder, før telefonen tages, og et
                    // `request_repaint_after` kommer ikke altid frem, når vinduet
                    // er minimeret eller skjult (#33).
                    ctx.request_repaint();
                }
                std::thread::sleep(POLL_EVERY);
            }
        });
        Self { rx, seen: Seen::default(), wanted }
    }

    pub fn set_wanted(&self, wanted: bool) {
        self.wanted.store(wanted, Ordering::Relaxed);
    }

    fn refresh(&mut self) {
        if let Some(seen) = self.rx.try_iter().last() {
            self.seen = seen;
        }
    }

    /// Programmerne, der bruger mikrofonen, som de sidst blev set.
    pub fn apps(&mut self) -> &[String] {
        self.refresh();
        &self.seen.in_use
    }

    /// Programmer, der har brugt mikrofonen (til at vælge navnet).
    pub fn known_apps(&mut self) -> &[String] {
        self.refresh();
        &self.seen.known
    }
}

/// Bruger et af programmerne i `wanted` (adskilt med komma, f.eks. "Teams, Zoom")
/// mikrofonen? Navnene sammenlignes som en del af programnavnet, uden hensyn til
/// store og små bogstaver.
pub fn matches(apps: &[String], wanted: &str) -> bool {
    let wanted: Vec<String> =
        wanted.split(',').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect();
    apps.iter().any(|app| {
        let app = app.to_lowercase();
        wanted.iter().any(|w| app.contains(w.as_str()))
    })
}

/// Et pænt navn ud fra Windows' nøglenavn: "MSTeams_8wekyb3d8bbwe" (app fra
/// butikken) bliver "MSTeams", "C:#Program Files#Zoom#bin#Zoom.exe" bliver "Zoom.exe".
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_app_name(key: &str) -> String {
    if key.contains('#') {
        key.rsplit('#').next().unwrap_or(key).to_owned()
    } else {
        key.split('_').next().unwrap_or(key).to_owned()
    }
}

/// Log over mikrofonen og automatisk telefon (#33), så man kan se, hvad
/// programmet så og besluttede, når et opkald ikke blev registreret. Filen
/// `mic-log.txt` ligger ved siden af datafilen og holdes under ca. 256 KB (den
/// forrige gemmes som `mic-log.old.txt`).
pub struct MicLog {
    path: std::path::PathBuf,
}

impl MicLog {
    const MAX_BYTES: u64 = 256 * 1024;

    pub fn new(data_path: &std::path::Path) -> Self {
        Self { path: data_path.with_file_name("mic-log.txt") }
    }

    pub fn folder(&self) -> Option<&std::path::Path> {
        self.path.parent()
    }

    pub fn write(&self, line: &str) {
        use std::io::Write;
        if std::fs::metadata(&self.path).is_ok_and(|m| m.len() > Self::MAX_BYTES) {
            let _ = std::fs::rename(&self.path, self.path.with_file_name("mic-log.old.txt"));
        }
        let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) else { return };
        let _ = writeln!(file, "{}  {line}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

/// Tilføjer `app` til listen i indstillingsfeltet, hvis den ikke er der.
pub fn add_to_list(list: &str, app: &str) -> String {
    let mut names: Vec<&str> = list.split(',').map(str::trim).filter(|n| !n.is_empty()).collect();
    if !names.iter().any(|n| n.eq_ignore_ascii_case(app)) {
        names.push(app);
    }
    names.join(", ")
}

/// Programmerne, der bruger mikrofonen nu, og dem der har brugt den.
#[cfg(windows)]
fn microphone_apps() -> (Vec<String>, Vec<String>) {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_QWORD, RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
    };

    const ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn qword(key: HKEY, subkey: &[u16], value: &str) -> Option<u64> {
        let value = wide(value);
        let mut data = 0u64;
        let mut size = size_of::<u64>() as u32;
        // SAFETY: nul-terminerede strenge og en buffer på 8 byte til en QWORD.
        let result = unsafe {
            RegGetValueW(
                key,
                subkey.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_QWORD,
                std::ptr::null_mut(),
                (&raw mut data).cast(),
                &mut size,
            )
        };
        (result == ERROR_SUCCESS).then_some(data)
    }

    /// Navnene på nøglens undernøgler.
    fn subkeys(key: HKEY) -> Vec<Vec<u16>> {
        let mut names = Vec::new();
        for index in 0.. {
            let mut name = [0u16; 512];
            let mut len = name.len() as u32;
            // SAFETY: bufferen og dens længde passer sammen; resten må være null.
            let result = unsafe {
                RegEnumKeyExW(
                    key,
                    index,
                    name.as_mut_ptr(),
                    &mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if result != ERROR_SUCCESS {
                break;
            }
            let mut name = name[..len as usize].to_vec();
            name.push(0);
            names.push(name);
        }
        names
    }

    fn open(parent: HKEY, path: &[u16]) -> Option<HKEY> {
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: nul-termineret sti; nøglen lukkes af den, der kalder.
        let result = unsafe { RegOpenKeyExW(parent, path.as_ptr(), 0, KEY_READ, &mut key) };
        (result == ERROR_SUCCESS).then_some(key)
    }

    /// Programmerne under `key`: dem der bruger mikrofonen nu, og alle der har.
    fn collect(key: HKEY, in_use: &mut Vec<String>, known: &mut Vec<String>) {
        for name in subkeys(key) {
            let text = String::from_utf16_lossy(&name[..name.len() - 1]);
            if text == "NonPackaged" {
                if let Some(sub) = open(key, &name) {
                    collect(sub, in_use, known);
                    // SAFETY: åbnet ovenfor.
                    unsafe { RegCloseKey(sub) };
                }
                continue;
            }
            let started = qword(key, &name, "LastUsedTimeStart").unwrap_or(0);
            let stopped = qword(key, &name, "LastUsedTimeStop");
            if started != 0 {
                known.push(windows_app_name(&text));
                if stopped == Some(0) {
                    in_use.push(windows_app_name(&text));
                }
            }
        }
    }

    let (mut in_use, mut known) = (Vec::new(), Vec::new());
    if let Some(root) = open(HKEY_CURRENT_USER, &wide(ROOT)) {
        collect(root, &mut in_use, &mut known);
        // SAFETY: åbnet ovenfor.
        unsafe { RegCloseKey(root) };
    }
    in_use.sort();
    in_use.dedup();
    (in_use, known)
}

#[cfg(target_os = "linux")]
fn microphone_apps() -> (Vec<String>, Vec<String>) {
    let Ok(output) = std::process::Command::new("pactl").args(["-f", "json", "list", "source-outputs"]).output() else {
        return Default::default();
    };
    (pactl_apps(&String::from_utf8_lossy(&output.stdout)), Vec::new())
}

/// Programmerne i `pactl -f json list source-outputs`. Lydmålere (f.eks. KDE's
/// lydstyrke-applet) er markeret `stream.monitor` og tæller ikke med.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn pactl_apps(json: &str) -> Vec<String> {
    let Ok(serde_json::Value::Array(outputs)) = serde_json::from_str(json) else { return Vec::new() };
    let mut apps: Vec<String> = outputs
        .iter()
        .filter_map(|o| {
            let p = &o["properties"];
            if p["stream.monitor"].as_str() == Some("true") {
                return None;
            }
            [&p["application.name"], &p["application.process.binary"]]
                .into_iter()
                .filter_map(|v| v.as_str())
                .find(|s| !s.is_empty())
                .map(str::to_owned)
        })
        .collect();
    apps.sort();
    apps.dedup();
    apps
}

#[cfg(not(any(windows, target_os = "linux")))]
fn microphone_apps() -> (Vec<String>, Vec<String>) {
    Default::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_part_of_the_name_ignoring_case() {
        let apps = vec!["MSTeams".to_owned(), "chrome.exe".to_owned()];
        assert!(matches(&apps, "Teams, Zoom"));
        assert!(matches(&apps, " zoom ,CHROME"));
        assert!(!matches(&apps, "Zoom"));
        assert!(!matches(&apps, ""));
        assert!(!matches(&[], "Teams"));
    }

    #[test]
    fn picked_app_is_added_once() {
        assert_eq!(add_to_list("Teams, Zoom", "ms-teams.exe"), "Teams, Zoom, ms-teams.exe");
        assert_eq!(add_to_list("Teams, Zoom", "zoom"), "Teams, Zoom");
        assert_eq!(add_to_list(" ", "Teams"), "Teams");
    }

    #[test]
    fn windows_key_names_become_app_names() {
        assert_eq!(windows_app_name("MSTeams_8wekyb3d8bbwe"), "MSTeams");
        assert_eq!(windows_app_name("C:#Program Files#Zoom#bin#Zoom.exe"), "Zoom.exe");
    }

    #[test]
    fn volume_meters_are_not_calls() {
        let json = r#"[
            {"properties":{"application.name":"","application.process.binary":"plasmashell","stream.monitor":"true"}},
            {"properties":{"application.name":"teams-for-linux","application.process.binary":"electron"}},
            {"properties":{"application.name":"","application.process.binary":"zoom"}}]"#;
        assert_eq!(pactl_apps(json), vec!["teams-for-linux", "zoom"]);
    }
}
