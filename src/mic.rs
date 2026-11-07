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

pub struct MicWatcher {
    rx: Receiver<Vec<String>>,
    apps: Vec<String>,
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
            let mut last = Vec::new();
            loop {
                let apps = if thread_wanted.load(Ordering::Relaxed) { apps_using_microphone() } else { Vec::new() };
                if apps != last {
                    if tx.send(apps.clone()).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                    last = apps;
                }
                std::thread::sleep(POLL_EVERY);
            }
        });
        Self { rx, apps: Vec::new(), wanted }
    }

    pub fn set_wanted(&self, wanted: bool) {
        self.wanted.store(wanted, Ordering::Relaxed);
    }

    /// Programmerne, der bruger mikrofonen, som de sidst blev set.
    pub fn apps(&mut self) -> &[String] {
        if let Some(apps) = self.rx.try_iter().last() {
            self.apps = apps;
        }
        &self.apps
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

#[cfg(windows)]
fn apps_using_microphone() -> Vec<String> {
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

    /// Programmerne under `key`, der bruger mikrofonen nu.
    fn in_use(key: HKEY, apps: &mut Vec<String>) {
        for name in subkeys(key) {
            let text = String::from_utf16_lossy(&name[..name.len() - 1]);
            if text == "NonPackaged" {
                if let Some(sub) = open(key, &name) {
                    in_use(sub, apps);
                    // SAFETY: åbnet ovenfor.
                    unsafe { RegCloseKey(sub) };
                }
                continue;
            }
            let started = qword(key, &name, "LastUsedTimeStart").unwrap_or(0);
            let stopped = qword(key, &name, "LastUsedTimeStop");
            if started != 0 && stopped == Some(0) {
                apps.push(windows_app_name(&text));
            }
        }
    }

    let mut apps = Vec::new();
    if let Some(root) = open(HKEY_CURRENT_USER, &wide(ROOT)) {
        in_use(root, &mut apps);
        // SAFETY: åbnet ovenfor.
        unsafe { RegCloseKey(root) };
    }
    apps.sort();
    apps.dedup();
    apps
}

#[cfg(target_os = "linux")]
fn apps_using_microphone() -> Vec<String> {
    let Ok(output) = std::process::Command::new("pactl").args(["-f", "json", "list", "source-outputs"]).output() else {
        return Vec::new();
    };
    pactl_apps(&String::from_utf8_lossy(&output.stdout))
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
fn apps_using_microphone() -> Vec<String> {
    Vec::new()
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
