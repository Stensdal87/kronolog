//! Holder vinduets størrelse, når det trækkes mellem skærme med forskellig
//! skalering (#43).
//!
//! winit bevarer den logiske størrelse, når skaleringen skifter, men når
//! vinduet står over grænsen mellem to skærme, skifter Windows skaleringen frem
//! og tilbage mange gange under trækket. Indimellem går en af ændringerne tabt
//! (Windows' trækkeløkke sætter sin egen størrelse), og så skaleres den
//! allerede skalerede størrelse igen. Vinduet voksede derfor ved hvert forsøg
//! (målt: 320 × 520 → 404 × 659 efter tre ture over grænsen). Vinduet skubbes
//! også tilbage, som om det stritter imod; det kan vi ikke rette, men størrelsen
//! kan vi: når skaleringen har skiftet, og vinduet har stået stille et øjeblik,
//! får det den logiske størrelse, det havde før.

use std::time::{Duration, Instant};

use eframe::egui::{Rect, Vec2};

/// Hvor længe vinduet skal stå stille, før størrelsen regnes for at være på plads.
pub(super) const SETTLE: Duration = Duration::from_millis(400);

#[derive(Default)]
pub(super) struct ScaleGuard {
    ppp: Option<f32>,
    rect: Option<Rect>,
    /// Sidst vinduet flyttede sig, skiftede størrelse eller skalering.
    changed_at: Option<Instant>,
    /// Den størrelse vinduet havde, sidst det stod stille (logiske punkter).
    settled: Option<Vec2>,
    /// Under et skift af skalering: størrelsen fra før skiftet.
    switching: Option<Vec2>,
}

impl ScaleGuard {
    /// Kaldes hvert billede med skaleringen og vinduets indre rektangel (i
    /// punkter). Returnerer den størrelse, vinduet skal have igen, hvis et skift
    /// af skalering har ændret den.
    pub(super) fn observe(&mut self, now: Instant, ppp: f32, rect: Rect) -> Option<Vec2> {
        let Some(old_rect) = self.rect else {
            (self.ppp, self.rect, self.changed_at) = (Some(ppp), Some(rect), Some(now));
            return None;
        };
        let ppp_changed = self.ppp != Some(ppp);
        if ppp_changed && self.switching.is_none() {
            self.switching = Some(self.settled.unwrap_or(old_rect.size()));
        }
        if ppp_changed || rect != old_rect {
            self.changed_at = Some(now);
        }
        (self.ppp, self.rect) = (Some(ppp), Some(rect));

        if !self.changed_at.is_some_and(|at| now.duration_since(at) >= SETTLE) {
            return None;
        }
        let restore = self.switching.take().filter(|size| (rect.size() - *size).length() > 1.0);
        self.settled = Some(restore.unwrap_or(rect.size()));
        restore
    }

    /// Om der skal tegnes igen om lidt, så en ændring kan nå at falde på plads.
    pub(super) fn busy(&self, now: Instant) -> bool {
        self.changed_at.is_some_and(|at| now.duration_since(at) < SETTLE) || self.switching.is_some()
    }

    /// Programmet ændrer selv størrelsen (fuld visning/bjælke); glem den gamle.
    pub(super) fn reset(&mut self) {
        self.settled = None;
        self.switching = None;
    }
}

/// Selve årsagen på Windows: ved `WM_DPICHANGED` regner winit sin egen størrelse
/// og placering ud i stedet for at bruge det rektangel, Windows foreslår, og
/// flytter vinduet dertil. Ender størstedelen af vinduet derved på den gamle
/// skærm, sender Windows straks en ny `WM_DPICHANGED` midt i den første. Den
/// indre gør vinduet småt, den ydre gør det stort igen, og vinduet står så med
/// den ene skærms skalering og den andens størrelse, som næste skift skalerer
/// en gang til. Derfor voksede (eller skrumpede) det og sakkede bagud for musen.
///
/// Her lægges en vinduesprocedure foran winits. Mens `WM_DPICHANGED` behandles,
/// rettes winits flytning (`WM_WINDOWPOSCHANGING`) til det foreslåede
/// rektangel, som Microsoft anbefaler; Windows har valgt det, så vinduet står på
/// den nye skærm og beholder musens plads i titellinjen.
#[cfg(windows)]
pub(super) fn use_suggested_rect_on_scale_change(frame: &eframe::Frame) {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, GWLP_WNDPROC, SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW, WINDOWPOS, WM_DPICHANGED,
        WM_WINDOWPOSCHANGING, WNDPROC,
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    /// winits vinduesprocedure; 0 indtil vores er sat ind.
    static WINIT_PROC: AtomicIsize = AtomicIsize::new(0);
    thread_local! {
        /// Det foreslåede rektangel, mens `WM_DPICHANGED` behandles.
        static SUGGESTED: Cell<Option<RECT>> = const { Cell::new(None) };
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: WINIT_PROC er sat til den tidligere vinduesprocedure, før vores blev sat ind.
        let winit_proc: WNDPROC = unsafe { std::mem::transmute(WINIT_PROC.load(Ordering::Relaxed)) };
        match msg {
            WM_DPICHANGED => {
                // SAFETY: lparam peger på et RECT, så længe beskeden behandles.
                let suggested = unsafe { *(lparam as *const RECT) };
                let outer = SUGGESTED.replace(Some(suggested));
                let result = unsafe { CallWindowProcW(winit_proc, hwnd, msg, wparam, lparam) };
                SUGGESTED.set(outer);
                result
            }
            WM_WINDOWPOSCHANGING => {
                if let Some(r) = SUGGESTED.get() {
                    // SAFETY: lparam peger på et WINDOWPOS, som må ændres.
                    let pos = unsafe { &mut *(lparam as *mut WINDOWPOS) };
                    if pos.flags & SWP_NOMOVE == 0 {
                        (pos.x, pos.y) = (r.left, r.top);
                    }
                    if pos.flags & SWP_NOSIZE == 0 {
                        (pos.cx, pos.cy) = (r.right - r.left, r.bottom - r.top);
                    }
                }
                unsafe { CallWindowProcW(winit_proc, hwnd, msg, wparam, lparam) }
            }
            _ => unsafe { CallWindowProcW(winit_proc, hwnd, msg, wparam, lparam) },
        }
    }

    if WINIT_PROC.load(Ordering::Relaxed) != 0 {
        return;
    }
    let Some(window) = frame.winit_window() else { return };
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else { return };
    let hwnd = handle.hwnd.get() as HWND;
    // SAFETY: vinduet tilhører denne tråd og lever, så længe programmet kører.
    let old = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, proc as *const () as isize) };
    WINIT_PROC.store(old, Ordering::Relaxed);
}

#[cfg(not(windows))]
pub(super) fn use_suggested_rect_on_scale_change(_frame: &eframe::Frame) {}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Pos2, vec2};

    fn rect(x: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, 100.0), vec2(w, h))
    }

    #[test]
    fn size_grown_while_crossing_monitors_is_restored() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut guard = ScaleGuard::default();
        assert_eq!(guard.observe(at(0), 1.0, rect(2100.0, 320.0, 520.0)), None);
        assert_eq!(guard.observe(at(500), 1.0, rect(2100.0, 320.0, 520.0)), None);
        // Trækket over grænsen: skaleringen skifter frem og tilbage, og
        // størrelsen vokser (de logiske mål er her de forkerte).
        assert_eq!(guard.observe(at(600), 1.25, rect(2300.0, 320.0, 520.0)), None);
        assert_eq!(guard.observe(at(620), 1.0, rect(2310.0, 400.0, 650.0)), None);
        assert_eq!(guard.observe(at(640), 1.25, rect(2320.0, 404.0, 659.0)), None);
        assert_eq!(guard.observe(at(700), 1.0, rect(2150.0, 404.0, 659.0)), None);
        assert!(guard.busy(at(800)));
        // Stået stille: størrelsen fra før trækket.
        assert_eq!(guard.observe(at(1200), 1.0, rect(2150.0, 404.0, 659.0)), Some(vec2(320.0, 520.0)));
        // Når vinduet har fået den igen, sker der ikke mere.
        assert_eq!(guard.observe(at(1250), 1.0, rect(2150.0, 320.0, 520.0)), None);
        assert_eq!(guard.observe(at(1700), 1.0, rect(2150.0, 320.0, 520.0)), None);
        assert!(!guard.busy(at(1700)));
    }

    #[test]
    fn resizing_by_hand_is_kept() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let mut guard = ScaleGuard::default();
        guard.observe(at(0), 1.0, rect(0.0, 320.0, 520.0));
        guard.observe(at(100), 1.0, rect(0.0, 360.0, 600.0));
        assert_eq!(guard.observe(at(600), 1.0, rect(0.0, 360.0, 600.0)), None);
        // Flyttet til en anden skalering uden at vokse: intet at rette, og den
        // nye størrelse er den, der huskes.
        guard.observe(at(700), 1.5, rect(3000.0, 360.0, 600.0));
        assert_eq!(guard.observe(at(1200), 1.5, rect(3000.0, 360.0, 600.0)), None);
        guard.observe(at(1300), 1.0, rect(100.0, 380.0, 600.0));
        assert_eq!(guard.observe(at(1800), 1.0, rect(100.0, 380.0, 600.0)), Some(vec2(360.0, 600.0)));
    }
}
