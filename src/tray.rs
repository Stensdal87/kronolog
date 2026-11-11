//! Ikon i systembakken med en menu til at styre tiden uden at vise vinduet.
//!
//! Brugerfladen beskriver menuen med en `TrayState`; klik i menuen kommer
//! tilbage som `TrayAction`s. Linux bruger StatusNotifierItem over D-Bus (ksni),
//! Windows bruger tray-icon.

use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;

use crate::data::Id;

pub enum TrayAction {
    /// Venstreklik på ikonet: vis eller skjul vinduet.
    Toggle,
    Start(Id),
    Call,
    Pause,
    Stop,
    Quit,
}

/// Et projekt i menuen "Skift projekt".
#[derive(Clone, PartialEq)]
pub struct TrayProject {
    pub id: Id,
    pub name: String,
    pub active: bool,
}

/// Alt hvad menuen viser. Menuen bygges kun om, når den ændrer sig.
#[derive(Clone, PartialEq, Default)]
pub struct TrayState {
    /// Første linje i menuen og tooltip, f.eks. "▶ Kunde / Projekt · 0:45:12".
    /// Ændres hvert sekund, mens tiden kører.
    pub status: String,
    pub running: bool,
    /// Teksten på telefon-punktet ("📞 Telefon", "📞 Afslut opkald …").
    pub call_label: String,
    /// Teksten på pause-punktet ("☕ Pause", "▶ Fortsæt" …).
    pub pause_label: String,
    pub window_visible: bool,
    /// Vis den røde prik på ikonet: tiden kører (#21). Skifter hvert sekund,
    /// så prikken blinker.
    pub dot: bool,
    /// Sproget, så menuen bygges om, når det skiftes.
    pub language: String,
    /// Kunder med deres projekter.
    pub customers: Vec<(String, Vec<TrayProject>)>,
}

pub struct Tray {
    rx: Receiver<TrayAction>,
    /// Programikonet med og uden den røde prik.
    icons: [egui::IconData; 2],
    last: Option<TrayState>,
    inner: platform::Inner,
}

impl Tray {
    /// Opretter ikonet. `None` hvis systemet ikke har en systembakke.
    pub fn start(ctx: &egui::Context) -> Option<Self> {
        let (tx, rx) = mpsc::channel();
        let icon = app_icon();
        let icons = [icon.clone(), with_recording_dot(&icon)];
        let sender = ActionSender { tx, ctx: ctx.clone() };
        let inner = platform::Inner::start(sender, icon)?;
        Some(Self { rx, icons, last: None, inner })
    }

    pub fn update(&mut self, state: TrayState) {
        if self.last.as_ref().is_some_and(|last| last.dot != state.dot) {
            self.inner.set_icon(&self.icons[state.dot as usize]);
        }
        if self.last.as_ref() != Some(&state) {
            self.inner.update(&state);
            self.last = Some(state);
        }
    }

    pub fn poll(&self) -> Vec<TrayAction> {
        self.rx.try_iter().collect()
    }
}

/// Programikonet (assets/icon.png).
pub fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).unwrap_or_default()
}

/// Ikonet med en rød prik midt i cirklen, der viser at tiden kører (#21).
/// Cirklens indre er mørkt (radius ca. 0,28 af ikonet), så prikken står klart.
pub fn with_recording_dot(icon: &egui::IconData) -> egui::IconData {
    let mut icon = icon.clone();
    let size = icon.width.min(icon.height) as f32;
    let center = (icon.width as f32 / 2.0, icon.height as f32 / 2.0);
    let (radius, ring) = (size * 0.2, 0.0);
    for (i, pixel) in icon.rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = (i as u32 % icon.width) as f32 + 0.5;
        let y = (i as u32 / icon.width) as f32 + 0.5;
        let distance = ((x - center.0).powi(2) + (y - center.1).powi(2)).sqrt();
        // Bløde kanter: dækningen går fra 1 til 0 over en pixel.
        let cover = |r: f32| (r - distance + 0.5).clamp(0.0, 1.0);
        let (outer, inner) = (cover(radius + ring), cover(radius));
        if outer <= 0.0 {
            continue;
        }
        let blend = |under: [u8; 4], color: [u8; 3], amount: f32| {
            let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).round() as u8;
            [mix(under[0], color[0]), mix(under[1], color[1]), mix(under[2], color[2]), mix(under[3], 255)]
        };
        *pixel = blend(*pixel, [255, 255, 255], outer);
        *pixel = blend(*pixel, [220, 40, 40], inner);
    }
    icon
}

/// Sender handlinger til brugerfladen og vækker den, også når vinduet er skjult.
#[derive(Clone)]
struct ActionSender {
    tx: Sender<TrayAction>,
    ctx: egui::Context,
}

impl ActionSender {
    fn send(&self, action: TrayAction) {
        let _ = self.tx.send(action);
        self.ctx.request_repaint();
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use ksni::blocking::{Handle, TrayMethods};
    use ksni::menu::{MenuItem, StandardItem, SubMenu};

    struct KsniTray {
        state: TrayState,
        sender: ActionSender,
        icon: ksni::Icon,
    }

    pub struct Inner {
        handle: Handle<KsniTray>,
    }

    impl Inner {
        pub fn start(sender: ActionSender, icon: egui::IconData) -> Option<Self> {
            let icon = to_ksni(&icon);
            let tray = KsniTray { state: TrayState::default(), sender, icon };
            // Fejler hvis skrivebordet ikke har en systembakke (StatusNotifierWatcher).
            let handle = tray.spawn().ok()?;
            Some(Self { handle })
        }

        pub fn update(&self, state: &TrayState) {
            let state = state.clone();
            self.handle.update(|tray| tray.state = state);
        }

        pub fn set_icon(&self, icon: &egui::IconData) {
            let icon = to_ksni(icon);
            self.handle.update(|tray| tray.icon = icon);
        }
    }

    /// ksni vil have ARGB i netværksbyte-orden; egui giver RGBA.
    fn to_ksni(icon: &egui::IconData) -> ksni::Icon {
        let data = icon.rgba.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, a]| [a, r, g, b]).collect();
        ksni::Icon { width: icon.width as i32, height: icon.height as i32, data }
    }

    fn item(label: impl Into<String>, enabled: bool, action: fn(&KsniTray)) -> MenuItem<KsniTray> {
        StandardItem {
            label: label.into(),
            enabled,
            activate: Box::new(move |tray: &mut KsniTray| action(tray)),
            ..Default::default()
        }
        .into()
    }

    impl ksni::Tray for KsniTray {
        fn id(&self) -> String {
            "kronolog".into()
        }

        fn title(&self) -> String {
            "KronoLog".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            vec![self.icon.clone()]
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip { title: "KronoLog".into(), description: self.state.status.clone(), ..Default::default() }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.sender.send(TrayAction::Toggle);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            let s = &self.state;
            let projects: Vec<MenuItem<Self>> = s
                .customers
                .iter()
                .map(|(customer, projects)| {
                    SubMenu {
                        label: customer.clone(),
                        submenu: projects
                            .iter()
                            .map(|p| {
                                let id = p.id;
                                StandardItem {
                                    label: if p.active { format!("▶ {}", p.name) } else { p.name.clone() },
                                    enabled: !p.active,
                                    activate: Box::new(move |tray: &mut Self| tray.sender.send(TrayAction::Start(id))),
                                    ..Default::default()
                                }
                                .into()
                            })
                            .collect(),
                        ..Default::default()
                    }
                    .into()
                })
                .collect();

            vec![
                item(s.status.clone(), false, |_| {}),
                MenuItem::Separator,
                SubMenu { label: tr!("tray.switch_project"), submenu: projects, ..Default::default() }.into(),
                item(s.call_label.clone(), true, |t| t.sender.send(TrayAction::Call)),
                item(s.pause_label.clone(), true, |t| t.sender.send(TrayAction::Pause)),
                item(tr!("tray.stop"), s.running, |t| t.sender.send(TrayAction::Stop)),
                MenuItem::Separator,
                item(
                    if s.window_visible { tr!("tray.hide_window") } else { tr!("tray.show_window") },
                    true,
                    |t| t.sender.send(TrayAction::Toggle),
                ),
                item(tr!("tray.quit"), true, |t| t.sender.send(TrayAction::Quit)),
            ]
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    pub struct Inner {
        icon: TrayIcon,
        /// Statuslinjen, så dens tekst kan opdateres uden at bygge menuen om.
        status_item: MenuItem,
        /// Menuen som den sidst blev bygget, uden statusteksten.
        built: TrayState,
    }

    impl Inner {
        pub fn start(sender: ActionSender, icon: egui::IconData) -> Option<Self> {
            let clicks = sender.clone();
            TrayIconEvent::set_event_handler(Some(move |event| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    clicks.send(TrayAction::Toggle);
                }
            }));
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let id = event.id.0.as_str();
                let action = match id {
                    "call" => TrayAction::Call,
                    "pause" => TrayAction::Pause,
                    "stop" => TrayAction::Stop,
                    "toggle" => TrayAction::Toggle,
                    "quit" => TrayAction::Quit,
                    _ => match id.strip_prefix("start:").and_then(|n| n.parse().ok()) {
                        Some(project_id) => TrayAction::Start(project_id),
                        None => return,
                    },
                };
                sender.send(action);
            }));

            let icon = tray_icon::Icon::from_rgba(icon.rgba, icon.width, icon.height).ok()?;
            let built = TrayState::default();
            let (menu, status_item) = build_menu(&built);
            let icon = TrayIconBuilder::new()
                .with_icon(icon)
                .with_tooltip("KronoLog")
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .ok()?;
            Some(Self { icon, status_item, built })
        }

        pub fn update(&mut self, state: &TrayState) {
            // Statusteksten ændres hvert sekund. At skifte hele menuen ud kan
            // lukke en åben menu, så den bygges kun om, når andet end teksten
            // (og prikken på ikonet) ændres.
            let structure = TrayState { status: String::new(), dot: false, ..state.clone() };
            if structure != self.built {
                let (menu, status_item) = build_menu(state);
                self.icon.set_menu(Some(Box::new(menu)));
                self.status_item = status_item;
                self.built = structure;
            } else {
                self.status_item.set_text(&state.status);
            }
            let _ = self.icon.set_tooltip(Some(format!("KronoLog\n{}", state.status)));
        }

        pub fn set_icon(&self, icon: &egui::IconData) {
            if let Ok(icon) = tray_icon::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height) {
                let _ = self.icon.set_icon(Some(icon));
            }
        }
    }

    fn build_menu(s: &TrayState) -> (Menu, MenuItem) {
        let menu = Menu::new();
        let switch = Submenu::new(tr!("tray.switch_project"), true);
        for (customer, projects) in &s.customers {
            let sub = Submenu::new(customer, true);
            for p in projects {
                let label = if p.active { format!("▶ {}", p.name) } else { p.name.clone() };
                let _ = sub.append(&MenuItem::with_id(format!("start:{}", p.id), label, !p.active, None));
            }
            let _ = switch.append(&sub);
        }
        let toggle = if s.window_visible { tr!("tray.hide_window") } else { tr!("tray.show_window") };
        let status_item = MenuItem::with_id("status", &s.status, false, None);
        let _ = menu.append(&status_item);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&switch);
        let _ = menu.append(&MenuItem::with_id("call", &s.call_label, true, None));
        let _ = menu.append(&MenuItem::with_id("pause", &s.pause_label, true, None));
        let _ = menu.append(&MenuItem::with_id("stop", tr!("tray.stop"), s.running, None));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id("toggle", toggle, true, None));
        let _ = menu.append(&MenuItem::with_id("quit", tr!("tray.quit"), true, None));
        (menu, status_item)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use super::*;

    pub struct Inner;

    impl Inner {
        pub fn start(_: ActionSender, _: egui::IconData) -> Option<Self> {
            None
        }

        pub fn update(&mut self, _: &TrayState) {}

        pub fn set_icon(&self, _: &egui::IconData) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_dot_is_red_in_the_middle_and_leaves_the_ring() {
        let icon = app_icon();
        let dotted = with_recording_dot(&icon);
        let pixel = |icon: &egui::IconData, x: u32, y: u32| {
            let i = ((y * icon.width + x) * 4) as usize;
            [icon.rgba[i], icon.rgba[i + 1], icon.rgba[i + 2], icon.rgba[i + 3]]
        };
        assert_eq!(pixel(&dotted, icon.width / 2, icon.height / 2), [220, 40, 40, 255]);
        assert_eq!(pixel(&dotted, 10, 10), pixel(&icon, 10, 10));
        // Den blå ring om prikken er urørt.
        assert_eq!(pixel(&dotted, icon.width * 82 / 100, icon.height / 2), pixel(&icon, icon.width * 82 / 100, icon.height / 2));
    }
}
