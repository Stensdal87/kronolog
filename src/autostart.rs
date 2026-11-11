//! Start KronoLog automatisk, når brugeren logger på.
//!
//! - Windows: værdien "KronoLog" under HKCU\Software\Microsoft\Windows\CurrentVersion\Run
//!   (det samme sted som installeren bruger). Ældre installere lagde en genvej i
//!   Start-mappen; den regnes også med og ryddes op.
//! - Linux: en .desktop-fil i ~/.config/autostart (XDG autostart).
//!
//! Programmet hed tidligere "Time Snapper"; var automatisk start slået til under
//! det navn, flyttes den over til KronoLog ved start (`migrate_old_shortcut`).

/// Om programmet starter automatisk lige nu.
pub fn is_enabled() -> bool {
    platform::is_enabled()
}

/// Erstatter genvejen i Start-mappen fra installeren i version 0.1/0.2 og
/// automatisk start under navnet "Time Snapper" med den nuværende.
pub fn migrate_old_shortcut() {
    if platform::has_old_shortcut() {
        let _ = set_enabled(true);
    }
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| tr!("autostart.err_exe", error = e))?;
    platform::set_enabled(enabled, &exe)
}

#[cfg(windows)]
mod platform {
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE: &str = "KronoLog";
    /// Navnet før KronoLog.
    const OLD_VALUE: &str = "Time Snapper";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Genvejen som installeren i version 0.1 og 0.2 lagde i Start-mappen.
    fn old_shortcut() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup\Time Snapper.lnk"))
    }

    fn run_value_exists() -> bool {
        value_exists(VALUE)
    }

    fn value_exists(name: &str) -> bool {
        let (key, value) = (wide(RUN_KEY), wide(name));
        // SAFETY: nul-terminerede strenge; data og størrelse må være null, når vi kun spørger om værdien findes.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        result == ERROR_SUCCESS
    }

    pub fn has_old_shortcut() -> bool {
        old_shortcut().is_some_and(|p| p.exists()) || value_exists(OLD_VALUE)
    }

    pub fn is_enabled() -> bool {
        run_value_exists() || has_old_shortcut()
    }

    pub fn set_enabled(enabled: bool, exe: &Path) -> Result<(), String> {
        // Den gamle genvej fjernes altid, så programmet ikke startes to gange.
        if let Some(shortcut) = old_shortcut()
            && shortcut.exists()
        {
            std::fs::remove_file(&shortcut).map_err(|e| tr!("autostart.err_old_shortcut", error = e))?;
        }
        // Den gamle værdi under navnet "Time Snapper" fjernes altid af samme grund.
        if value_exists(OLD_VALUE) {
            let (key, old) = (wide(RUN_KEY), wide(OLD_VALUE));
            // SAFETY: nul-terminerede strenge.
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), old.as_ptr()) };
        }
        let (key, value) = (wide(RUN_KEY), wide(VALUE));
        let result = if enabled {
            let command = wide(&format!("\"{}\"", exe.display()));
            // SAFETY: nul-terminerede strenge; størrelsen er i bytes inkl. det afsluttende nul.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    command.as_ptr().cast(),
                    (command.len() * 2) as u32,
                )
            }
        } else if run_value_exists() {
            // SAFETY: nul-terminerede strenge.
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) }
        } else {
            ERROR_SUCCESS
        };
        if result == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(tr!("autostart.err_windows", code = result))
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::path::{Path, PathBuf};

    fn desktop_file() -> Option<PathBuf> {
        Some(directories::BaseDirs::new()?.config_dir().join("autostart").join("kronolog.desktop"))
    }

    /// Filen fra før navneskiftet til KronoLog.
    fn old_desktop_file() -> Option<PathBuf> {
        Some(directories::BaseDirs::new()?.config_dir().join("autostart").join("time-snapper.desktop"))
    }

    /// Citerer en sti til Exec-linjen i en .desktop-fil (freedesktop Desktop Entry).
    fn quote_exec(path: &Path) -> String {
        let mut quoted = String::from("\"");
        for c in path.display().to_string().chars() {
            if matches!(c, '"' | '`' | '$' | '\\') {
                quoted.push('\\');
            }
            quoted.push(c);
        }
        quoted.push('"');
        quoted
    }

    pub fn has_old_shortcut() -> bool {
        old_desktop_file().is_some_and(|p| p.exists())
    }

    pub fn is_enabled() -> bool {
        desktop_file().is_some_and(|p| p.exists()) || has_old_shortcut()
    }

    pub fn set_enabled(enabled: bool, exe: &Path) -> Result<(), String> {
        // Den gamle fil fjernes altid, så programmet ikke startes to gange.
        if let Some(old) = old_desktop_file() {
            let _ = std::fs::remove_file(old);
        }
        let path = desktop_file().ok_or_else(|| tr!("autostart.err_folder"))?;
        if !enabled {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(tr!("autostart.err_disable", error = e)),
                _ => Ok(()),
            };
        }
        let content = format!(
            "[Desktop Entry]\nType=Application\nName=KronoLog\nComment=Tidsregistrering\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            quote_exec(exe)
        );
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| tr!("autostart.err_create", path = dir.display(), error = e))?;
        }
        std::fs::write(&path, content).map_err(|e| tr!("autostart.err_enable", error = e))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn exec_path_is_quoted_and_escaped() {
            assert_eq!(
                quote_exec(Path::new("/home/a/Dev/Time Tracker/kronolog")),
                "\"/home/a/Dev/Time Tracker/kronolog\""
            );
            assert_eq!(quote_exec(Path::new("/x/$y\"z")), "\"/x/\\$y\\\"z\"");
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use std::path::Path;

    pub fn has_old_shortcut() -> bool {
        false
    }

    pub fn is_enabled() -> bool {
        false
    }

    pub fn set_enabled(_: bool, _: &Path) -> Result<(), String> {
        Err("Automatisk start understøttes ikke på dette system".to_owned())
    }
}
