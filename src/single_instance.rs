//! Kun én kørende KronoLog ad gangen (issue #11).
//!
//! Den første kopi låser `instance.lock` ved siden af datafilen, så længe den
//! kører (styresystemet slipper låsen igen, også efter et nedbrud). Den lytter
//! på en lokal TCP-port (kun 127.0.0.1) og skriver portnummeret i
//! `instance.port`. Starter man programmet igen, er filen låst: den nye kopi
//! beder den kørende om at vise sit vindue og afslutter selv.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;

/// Besked fra en anden kopi af programmet.
pub enum Command {
    /// Vis vinduet og læg det forrest.
    Show,
}

pub struct Instance {
    rx: Receiver<Command>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
    /// Låsen holdes, så længe programmet kører.
    _lock: Option<File>,
}

impl Instance {
    /// Brugerfladen vækkes, når der kommer en besked (også når vinduet er skjult).
    pub fn set_context(&self, ctx: &egui::Context) {
        if let Ok(mut slot) = self.ctx.lock() {
            *slot = Some(ctx.clone());
        }
    }

    pub fn poll(&self) -> Vec<Command> {
        self.rx.try_iter().collect()
    }
}

/// `None` hvis programmet allerede kører: så er den kørende kopi bedt om at
/// vise sig, og denne kopi skal afslutte.
pub fn start(port_file: &Path) -> Option<Instance> {
    if let Some(dir) = port_file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let lock_path = port_file.with_file_name("instance.lock");
    // Kan låsen slet ikke bruges (f.eks. et drev uden låse), kører vi bare.
    let lock = OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path).ok();
    if let Some(file) = &lock
        && let Err(TryLockError::WouldBlock) = file.try_lock()
    {
        // Den anden kopi er måske lige startet og har ikke skrevet porten endnu.
        for _ in 0..20 {
            if notify_running(port_file) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        return None;
    }
    // Kan vi ikke lytte, kører vi bare videre som eneste kopi.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok();
    let (tx, rx) = mpsc::channel();
    let ctx: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(None));
    if let Some(listener) = listener {
        if let Ok(addr) = listener.local_addr() {
            let _ = std::fs::write(port_file, addr.port().to_string());
        }
        let wake = ctx.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                if BufReader::new(stream).read_line(&mut line).is_ok() && line.trim() == "show" {
                    let _ = tx.send(Command::Show);
                    if let Ok(slot) = wake.lock()
                        && let Some(ctx) = slot.as_ref()
                    {
                        ctx.request_repaint();
                    }
                }
            }
        });
    }
    Some(Instance { rx, ctx, _lock: lock })
}

/// Beder den kørende kopi om at vise sig. `true` hvis den svarede.
fn notify_running(port_file: &Path) -> bool {
    let Some(port) = std::fs::read_to_string(port_file).ok().and_then(|p| p.trim().parse::<u16>().ok()) else {
        return false;
    };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return false;
    };
    allow_foreground();
    stream.write_all(b"show\n").is_ok()
}

/// Windows lader kun det program, brugeren lige har startet, hente et vindue
/// frem; giv den kørende kopi lov til det.
fn allow_foreground() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
        // SAFETY: ASFW_ANY (u32::MAX) giver alle processer lov; ingen andre forudsætninger.
        unsafe { AllowSetForegroundWindow(u32::MAX) };
    }
}

/// Filen med portnummeret, ved siden af datafilen.
pub fn port_file(data_path: &Path) -> PathBuf {
    data_path.with_file_name("instance.port")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_start_asks_the_first_to_show_itself() {
        let dir = std::env::temp_dir().join(format!("ts-instance-{}", std::process::id()));
        let file = dir.join("instance.port");
        let first = start(&file).expect("første kopi starter");
        assert!(start(&file).is_none(), "anden kopi skal afslutte");
        // Beskeden når frem til den første kopi.
        let mut got = false;
        for _ in 0..50 {
            if first.poll().into_iter().any(|c| matches!(c, Command::Show)) {
                got = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(got, "den første kopi fik besked om at vise sig");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn leftover_files_after_a_crash_are_ignored() {
        let dir = std::env::temp_dir().join(format!("ts-instance-stale-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("instance.port");
        // En port, som ingen lytter på (lukket med det samme).
        let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port();
        std::fs::write(&file, port.to_string()).unwrap();
        std::fs::write(dir.join("instance.lock"), "").unwrap();
        assert!(start(&file).is_some(), "låsen er fri, så denne kopi bliver den kørende");
        std::fs::remove_dir_all(&dir).ok();
    }
}
