//! Hvilke programmer der bruger mikrofonen lige nu (#27). Bruges til at tage
//! telefonen automatisk, når man er i et opkald i f.eks. Teams.
//!
//! Windows fører selv regnskab i registeret (det er det, der viser
//! mikrofonikonet i proceslinjen): en undernøgle pr. program under
//! `CapabilityAccessManager\ConsentStore\microphone`, hvor `LastUsedTimeStop` er 0,
//! mens programmet bruger mikrofonen. På Linux spørges PipeWire/PulseAudio med
//! `pactl`. Der spørges hvert andet sekund i en tråd.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
    /// Opkaldsprogrammernes synlige vinduer, mens mikrofonen er i brug (kun
    /// Windows). `new` er vinduer, der ikke fandtes, før mikrofonen blev taget:
    /// typisk selve opkaldsvinduet, hvis titel ofte er navnet på den, man
    /// taler med.
    windows: Vec<CallWindow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallWindow {
    pub title: String,
    pub new: bool,
}

pub struct MicWatcher {
    rx: Receiver<Seen>,
    seen: Seen,
    /// Tråden spørger kun, når nogen har brug for svaret.
    wanted: Arc<AtomicBool>,
    /// Programmerne fra indstillingerne; deres vinduer følges under opkald.
    call_apps: Arc<Mutex<String>>,
}

impl MicWatcher {
    pub fn start(ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let wanted = Arc::new(AtomicBool::new(false));
        let thread_wanted = wanted.clone();
        let call_apps = Arc::new(Mutex::new(String::new()));
        let thread_apps = call_apps.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut last = Seen::default();
            // Opkaldsprogrammernes vinduer, sidst mikrofonen var fri.
            let mut before: Vec<isize> = Vec::new();
            loop {
                let mut seen = Seen { in_use: Vec::new(), known: last.known.clone(), windows: Vec::new() };
                if thread_wanted.load(Ordering::Relaxed) {
                    let (in_use, known) = microphone_apps();
                    seen.in_use = in_use;
                    seen.known.extend(known);
                    seen.known.extend(seen.in_use.iter().cloned());
                    seen.known.sort_by_key(|a| a.to_lowercase());
                    seen.known.dedup();
                    let apps = thread_apps.lock().map(|a| a.clone()).unwrap_or_default();
                    let windows = app_windows(&apps);
                    if seen.in_use.is_empty() {
                        before = windows.into_iter().map(|(id, _)| id).collect();
                    } else {
                        seen.windows = windows
                            .into_iter()
                            .map(|(id, title)| CallWindow { title, new: !before.contains(&id) })
                            .collect();
                    }
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
        Self { rx, seen: Seen::default(), wanted, call_apps }
    }

    pub fn set_wanted(&self, wanted: bool, call_apps: &str) {
        self.wanted.store(wanted, Ordering::Relaxed);
        if let Ok(mut apps) = self.call_apps.lock()
            && *apps != call_apps
        {
            call_apps.clone_into(&mut apps);
        }
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

    /// Opkaldsprogrammernes vinduer, mens mikrofonen er i brug.
    pub fn windows(&mut self) -> &[CallWindow] {
        self.refresh();
        &self.seen.windows
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

/// Gæt på, hvem man taler med: titlen på det første vindue, der kom, da
/// opkaldet startede, uden programnavnet. "Anna Jensen | Microsoft Teams" giver
/// "Anna Jensen". Er der intet nyt vindue, gættes der ikke.
pub fn caller_name(windows: &[CallWindow]) -> Option<String> {
    windows.iter().filter(|w| w.new).find_map(|w| clean_title(&w.title))
}

fn clean_title(title: &str) -> Option<String> {
    let generic = ["microsoft teams", "teams", "zoom", "zoom meeting", "zoom workplace"];
    title
        .split(['|', '\u{2013}', '\u{2014}'])
        .map(str::trim)
        .find(|part| !part.is_empty() && !generic.contains(&part.to_lowercase().as_str()))
        .map(str::to_owned)
}

/// Synlige vinduer med titel (id og titel) fra programmer, hvis exe-navn
/// matcher `apps` (som i indstillingerne, f.eks. "Teams" matcher ms-teams.exe).
#[cfg(windows)]
fn app_windows(apps: &str) -> Vec<(isize, String)> {
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    if apps.trim().is_empty() {
        return Vec::new();
    }

    fn exe_name(pid: u32) -> Option<String> {
        // SAFETY: håndtaget lukkes igen; bufferen og længden passer sammen.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buffer = [0u16; 1024];
            let mut len = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len);
            CloseHandle(process);
            (ok != 0).then(|| {
                let path = String::from_utf16_lossy(&buffer[..len as usize]);
                path.rsplit('\\').next().unwrap_or(&path).to_owned()
            })
        }
    }

    unsafe extern "system" fn each(hwnd: HWND, list: LPARAM) -> i32 {
        // SAFETY: `list` er pegeren til vektoren nedenfor, som lever under EnumWindows.
        let list = unsafe { &mut *(list as *mut Vec<(HWND, u32)>) };
        // SAFETY: hwnd kommer fra EnumWindows.
        if unsafe { IsWindowVisible(hwnd) } != 0 {
            let mut pid = 0u32;
            // SAFETY: som ovenfor.
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            list.push((hwnd, pid));
        }
        1
    }

    let mut all: Vec<(HWND, u32)> = Vec::new();
    // SAFETY: tilbagekaldet bruger kun vektoren, mens EnumWindows kører.
    unsafe { EnumWindows(Some(each), &raw mut all as LPARAM) };
    let mut names: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
    let mut windows = Vec::new();
    for (hwnd, pid) in all {
        let wanted = *names.entry(pid).or_insert_with(|| exe_name(pid).is_some_and(|exe| matches(&[exe], apps)));
        if !wanted {
            continue;
        }
        // SAFETY: bufferen er stor nok til titlen og nul-tegnet.
        let title = unsafe {
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                continue;
            }
            let mut buffer = vec![0u16; len as usize + 1];
            let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
        };
        if !title.trim().is_empty() {
            windows.push((hwnd as isize, title));
        }
    }
    windows
}

#[cfg(not(windows))]
fn app_windows(_apps: &str) -> Vec<(isize, String)> {
    Vec::new()
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
    fn caller_name_comes_from_the_new_window_without_the_app_name() {
        let window = |title: &str, new| CallWindow { title: title.to_owned(), new };
        let windows = [window("Chat | Bo Hansen | Microsoft Teams", false), window("Anna Jensen | Microsoft Teams", true)];
        assert_eq!(caller_name(&windows).as_deref(), Some("Anna Jensen"));
        assert_eq!(caller_name(&windows[..1]), None, "hovedvinduet var der i forvejen");
        assert_eq!(caller_name(&[window("Microsoft Teams", true)]), None);
        assert_eq!(caller_name(&[window("Ugemøde – Microsoft Teams", true)]).as_deref(), Some("Ugemøde"));
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
