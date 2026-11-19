//! Links på kunder og projekter (#41): et weblink (f.eks. til Salesforce), der
//! kan åbnes i standardbrowseren, Edge eller Chrome, eller en mappe, der åbnes
//! i Stifinder. Kun weblinks, mail-links og mapper, der findes, åbnes, så et
//! link aldrig kan starte et program.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Hvor et weblink åbnes.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Browser {
    #[default]
    Default,
    Edge,
    Chrome,
}

impl Browser {
    pub const ALL: [Browser; 3] = [Browser::Default, Browser::Edge, Browser::Chrome];

    pub fn label(self) -> String {
        match self {
            Browser::Default => tr!("link.browser_default"),
            Browser::Edge => "Microsoft Edge".to_owned(),
            Browser::Chrome => "Google Chrome".to_owned(),
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Browser::Default
    }
}

/// Et link og hvor det åbnes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Link<'a> {
    pub target: &'a str,
    pub browser: Browser,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Web,
    Mail,
    Folder,
    /// Hverken weblink eller mappe; åbnes ikke.
    Unknown,
}

/// Står foran et weblink for at åbne det i Edge (fra før indstillingen).
const EDGE: &str = "microsoft-edge:";

pub fn kind(link: &str) -> Kind {
    let web = |link: &str| {
        let lower = link.to_lowercase();
        lower.starts_with("https://") || lower.starts_with("http://")
    };
    if let Some(rest) = strip_prefix_ignore_case(link, EDGE) {
        return if web(rest) { Kind::Web } else { Kind::Unknown };
    }
    if web(link) {
        Kind::Web
    } else if link.to_lowercase().starts_with("mailto:") {
        Kind::Mail
    } else if looks_like_path(link) {
        Kind::Folder
    } else {
        Kind::Unknown
    }
}

/// C:\…, C:/…, \\server\… (Windows) eller /… (Linux).
fn looks_like_path(link: &str) -> bool {
    let bytes = link.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');
    drive || link.starts_with("\\\\") || link.starts_with('/')
}

/// Et link, som brugeren har skrevet eller indsat: en mappe beholdes (uden
/// anførselstegn fra Stifinders "Kopiér som sti"), og et weblink uden
/// "https://" får det foran, så "firma.lightning.force.com/…" også virker.
pub fn normalize(link: &str) -> String {
    let link = link.trim();
    let unquoted = link.strip_prefix('"').and_then(|l| l.strip_suffix('"')).unwrap_or(link).trim();
    if looks_like_path(unquoted) {
        return unquoted.to_owned();
    }
    if let Some(rest) = strip_prefix_ignore_case(link, EDGE) {
        return format!("{EDGE}{}", normalize(rest));
    }
    if link.is_empty() || link.contains("://") || link.to_lowercase().starts_with("mailto:") {
        link.to_owned()
    } else {
        format!("https://{link}")
    }
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

/// Mappen findes og er en mappe (ikke en fil, der kunne køres).
pub fn folder_exists(link: &str) -> bool {
    Path::new(link).is_dir()
}

/// Åbner linket, hvis det må: weblinks i den valgte browser, mail-links i
/// mailprogrammet og mapper i Stifinder.
pub fn open(link: Link) {
    let target = link.target;
    match kind(target) {
        Kind::Web if strip_prefix_ignore_case(target, EDGE).is_some() => shell_open(target, None),
        Kind::Web => match link.browser {
            Browser::Default => shell_open(target, None),
            Browser::Edge => open_with(&["msedge.exe", "microsoft-edge", "microsoft-edge-stable"], target),
            Browser::Chrome => open_with(&["chrome.exe", "google-chrome", "google-chrome-stable", "chromium"], target),
        },
        Kind::Mail => shell_open(target, None),
        Kind::Folder if folder_exists(target) => shell_open(target, None),
        Kind::Folder | Kind::Unknown => {}
    }
}

/// Åbner en side eller mappe med systemets standardprogram.
pub fn open_default(target: &str) {
    shell_open(target, None);
}

/// Windows: programnavnet findes via "App Paths" (som i Kør), så chrome.exe og
/// msedge.exe virker uden fuld sti. Linux: det første program, der findes.
fn open_with(programs: &[&str], url: &str) {
    #[cfg(windows)]
    {
        let program = programs.iter().find(|p| p.ends_with(".exe")).copied().unwrap_or(programs[0]);
        shell_open(program, Some(&format!("\"{url}\"")));
    }
    #[cfg(not(windows))]
    {
        let started = programs
            .iter()
            .filter(|p| !p.ends_with(".exe"))
            .any(|p| std::process::Command::new(p).arg(url).spawn().is_ok());
        if !started {
            shell_open(url, None);
        }
    }
}

/// Som "Kør" i Windows: `ShellExecuteW` med "open". Før blev `explorer.exe
/// <url>` brugt, men explorer læser selv sine argumenter (fx komma som
/// skilletegn), så lange links fra Edge blev ikke åbnet (#41). Kører i en egen
/// tråd med COM, som Windows anbefaler, så brugerfladen ikke venter.
#[cfg(windows)]
fn shell_open(file: &str, parameters: Option<&str>) {
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (verb, file, parameters) = (wide("open"), wide(file), parameters.map(wide));
    std::thread::spawn(move || {
        // SAFETY: nul-terminerede strenge, der lever under kaldet; COM afsluttes
        // på samme tråd.
        unsafe {
            let com = CoInitializeEx(std::ptr::null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32);
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                parameters.as_ref().map_or(std::ptr::null(), |p| p.as_ptr()),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
            if com >= 0 {
                CoUninitialize();
            }
        }
    });
}

#[cfg(not(windows))]
fn shell_open(file: &str, _parameters: Option<&str>) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(file).spawn();
    #[cfg(not(target_os = "macos"))]
    let _ = std::process::Command::new("xdg-open").arg(file).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_links_get_https_and_folders_are_kept() {
        assert_eq!(normalize(" firma.my.salesforce.com/kunde "), "https://firma.my.salesforce.com/kunde");
        assert_eq!(normalize("https://x.dk/a?b=1&c=2,3"), "https://x.dk/a?b=1&c=2,3");
        assert_eq!(normalize("\"C:\\Kunder\\Firma A/S\""), "C:\\Kunder\\Firma A/S", "Kopiér som sti");
        assert_eq!(normalize("\\\\server\\deling\\Firma"), "\\\\server\\deling\\Firma");
        assert_eq!(normalize("/home/bruger/Kunder"), "/home/bruger/Kunder");
        assert_eq!(normalize(""), "");
        assert_eq!(normalize(" microsoft-edge:firma.my.salesforce.com/x "), "microsoft-edge:https://firma.my.salesforce.com/x");
    }

    #[test]
    fn only_web_mail_and_folders_can_be_opened() {
        assert_eq!(kind("HTTPS://x.dk"), Kind::Web);
        assert_eq!(kind("microsoft-edge:https://x.dk/a,b"), Kind::Web);
        assert_eq!(kind("mailto:a@b.dk"), Kind::Mail);
        assert_eq!(kind("C:\\Kunder"), Kind::Folder);
        assert_eq!(kind("D:/Kunder"), Kind::Folder);
        assert_eq!(kind("\\\\server\\deling"), Kind::Folder);
        assert_eq!(kind("file:///C:/Windows/system32/calc.exe"), Kind::Unknown);
        assert_eq!(kind("microsoft-edge:file:///C:/x.exe"), Kind::Unknown);
        assert_eq!(kind("calc.exe"), Kind::Unknown);
        // En "mappe", der er en fil, åbnes ikke.
        let exe = std::env::current_exe().unwrap();
        assert!(!folder_exists(&exe.to_string_lossy()));
        assert!(folder_exists(&exe.parent().unwrap().to_string_lossy()));
    }
}
