//! Den sammenfoldede visning: en lille bjælke med tid og ikonknapper, som en
//! mini-afspiller, plus en projektvælger der folder sig ud fra bjælken.
//!
//! Vinduet skifter størrelse efter hvad der skal vises (`WindowLayout`). Står
//! vinduet i den nederste halvdel af skærmen, vokser det opad, så vælgeren og
//! dialoger ikke havner uden for skærmen.

use super::*;
use crate::shortcuts::Action;

/// Hvad vinduet viser og dermed hvor stort det er.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum WindowLayout {
    /// Det fulde program.
    Normal,
    /// Kun bjælken.
    Bar,
    /// Bjælken med projektvælgeren under.
    Picker,
}

/// Projektvælgeren, der åbnes med 🔀 i bjælken (eller Ctrl+K).
#[derive(Default)]
pub(super) struct ProjectPicker {
    search: String,
    new_customer: String,
    new_project: String,
    focused: bool,
    /// Flyt den kørende tidstagning i stedet for at starte en ny (issue #10).
    moving: bool,
}

impl ProjectPicker {
    pub(super) fn moving() -> Self {
        Self { moving: true, ..Default::default() }
    }
}

/// En flytning af vinduet, som rettes til, hvis vinduesmanageren lægger
/// titellinjen oveni (se `restore_window_position`).
pub(super) struct PendingMove {
    target: Pos2,
    request: Pos2,
    at: Instant,
    tries: u8,
}

const BAR_SIZE: Vec2 = Vec2::new(400.0, 38.0);
/// Den lodrette bjælke: den vandrette drejet 90°.
const VERTICAL_BAR_SIZE: Vec2 = Vec2::new(38.0, 400.0);
const PICKER_HEIGHT: f32 = 420.0;
const NORMAL_SIZE: Vec2 = Vec2::new(320.0, 520.0);
const NORMAL_MIN_SIZE: Vec2 = Vec2::new(260.0, 300.0);

impl TimeSnapperApp {
    /// Hvilken visning der skal bruges lige nu. Dialoger kræver plads, så
    /// vinduet folder sig midlertidigt helt ud, mens de er åbne.
    pub(super) fn wanted_layout(&self) -> WindowLayout {
        if !self.data.compact {
            return WindowLayout::Normal;
        }
        if self.dialog_open() || self.settings_open {
            WindowLayout::Normal
        } else if self.picker.is_some() {
            WindowLayout::Picker
        } else {
            WindowLayout::Bar
        }
    }

    /// Giver vinduet den størrelse visningen kræver. Vinduet vokser opad, hvis
    /// det står i den nederste halvdel af skærmen, og holdes inden for skærmen.
    pub(super) fn apply_layout(&mut self, ctx: &egui::Context, frame: &eframe::Frame, layout: WindowLayout) {
        let Some(inner) = ctx.input(|i| i.viewport().inner_rect) else { return };
        // Husk den fulde størrelse, men kun når vinduet har nået at få den. Var
        // det kun foldet ud et øjeblik (en dialog, der lukkede med det samme),
        // har vinduet stadig bjælkens størrelse, og den må ikke gemmes (#25).
        let settled = self.layout_changed_at.elapsed() > StdDuration::from_secs(1);
        if self.layout == WindowLayout::Normal
            && layout != WindowLayout::Normal
            && settled
            && inner.width() >= NORMAL_MIN_SIZE.x
            && inner.height() >= NORMAL_MIN_SIZE.y
        {
            self.data.normal_size = Some(inner.size().into());
        }
        if layout != self.layout {
            self.layout_changed_at = Instant::now();
        }
        let (size, min_size) = match layout {
            WindowLayout::Normal => (self.data.normal_size.map_or(NORMAL_SIZE, Vec2::from), NORMAL_MIN_SIZE),
            WindowLayout::Bar if self.data.bar_vertical => (VERTICAL_BAR_SIZE, Vec2::new(VERTICAL_BAR_SIZE.x, 240.0)),
            WindowLayout::Bar => (BAR_SIZE, Vec2::new(240.0, BAR_SIZE.y)),
            WindowLayout::Picker => (Vec2::new(BAR_SIZE.x, PICKER_HEIGHT), Vec2::new(240.0, 200.0)),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(min_size));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        // Bjælken har ingen titellinje (den lodrette er for smal til en, og den
        // vandrette ser pænere ud uden, #23); den flyttes ved at trække i den.
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(layout == WindowLayout::Normal));

        let mut top_left = inner.min;
        // Første gang ved opstart står vinduet allerede, hvor det blev gemt; dets
        // midlertidige størrelse (minimum 300 px høj) må ikke flytte det.
        if self.layout_applied
            && let Some(screen) = current_screen(ctx, frame)
        {
            let in_lower_half = inner.center().y > screen.center().y;
            let in_right_half = inner.center().x > screen.center().x;
            let (anchor_bottom, anchor_right) = match (self.layout, layout) {
                // Bjælken folder sig ud (eller drejes): vælg kanter efter hvor
                // bjælken står, og husk dem.
                (WindowLayout::Bar, _) => {
                    (*self.bar_anchor_bottom.insert(in_lower_half), *self.bar_anchor_right.insert(in_right_half))
                }
                // Tilbage til bjælken: samme kanter som da den foldede sig ud.
                (_, WindowLayout::Bar) => (
                    self.bar_anchor_bottom.unwrap_or(in_lower_half),
                    self.bar_anchor_right.unwrap_or(in_right_half),
                ),
                _ => (in_lower_half, in_right_half),
            };
            if anchor_bottom {
                top_left.y = inner.max.y - size.y;
            }
            // Den lodrette bjælke står typisk i højre side: hold den ved kanten.
            if self.data.bar_vertical && anchor_right {
                top_left.x = inner.max.x - size.x;
            }
            top_left.x = top_left.x.clamp(screen.min.x, (screen.max.x - size.x).max(screen.min.x));
            top_left.y = top_left.y.clamp(screen.min.y, (screen.max.y - size.y).max(screen.min.y));
        }
        if (top_left - inner.min).length() > 1.0 {
            self.move_window_to(ctx, top_left);
        }
        self.layout = layout;
        self.layout_applied = true;
        self.applied_vertical = self.data.bar_vertical;
        self.save();
    }

    /// Holder øje med, om den lodrette bjælke står i venstre side af skærmen,
    /// så teksten kan vende den rigtige vej (#20). Følger med, mens den trækkes.
    pub(super) fn update_bar_side(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.layout != WindowLayout::Bar || !self.data.bar_vertical {
            return;
        }
        let Some(inner) = ctx.input(|i| i.viewport().inner_rect) else { return };
        if let Some(screen) = current_screen(ctx, frame) {
            self.bar_on_left = inner.center().x < screen.center().x;
        }
    }

    /// Flytter vinduet, så indholdet starter ved `target` (skærmpunkter).
    fn move_window_to(&mut self, ctx: &egui::Context, target: Pos2) {
        let (inner, outer) = ctx.input(|i| (i.viewport().inner_rect, i.viewport().outer_rect));
        let (Some(inner), Some(outer)) = (inner, outer) else { return };
        let request = outer.min + (target - inner.min) + self.move_correction;
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(request));
        self.pending_move = Some(PendingMove { target, request, at: Instant::now(), tries: 0 });
        ctx.request_repaint_after(StdDuration::from_millis(150));
    }

    /// Retter en flytning til, hvis vinduet ikke landede, hvor det skulle.
    pub(super) fn handle_pending_move(&mut self, ctx: &egui::Context) {
        let Some(pending) = &mut self.pending_move else { return };
        let wait = StdDuration::from_millis(150);
        if pending.at.elapsed() < wait {
            ctx.request_repaint_after(wait);
            return;
        }
        let Some(inner) = ctx.input(|i| i.viewport().inner_rect) else { return };
        let off = pending.target - inner.min;
        if off.length() <= 1.0 || pending.tries >= 3 {
            self.pending_move = None;
            return;
        }
        pending.request += off;
        pending.tries += 1;
        // Husk afvigelsen, så næste flytning lander rigtigt i første forsøg.
        self.move_correction += off;
        pending.at = Instant::now();
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pending.request));
        ctx.request_repaint_after(wait);
    }

    /// Bjælken: status, tid, projekt og små ikonknapper, vandret eller lodret.
    pub(super) fn compact_bar(&mut self, ui: &mut egui::Ui, vertical: bool) {
        let active = self.data.active.as_ref();
        let running = active.is_some();
        let on_call = active.is_some_and(|a| a.project_id.is_none());
        let on_pause = active.is_some_and(|a| a.is_note(NOTE_PAUSE));
        let (color, time, label) = match active {
            None => (ui.visuals().weak_text_color(), "0:00:00".to_owned(), tr!("common.no_timer")),
            Some(a) => {
                let color = if on_pause {
                    Color32::from_rgb(240, 170, 60)
                } else if on_call {
                    Color32::from_rgb(170, 120, 230)
                } else {
                    ACCENT
                };
                let label = match a.project_id {
                    Some(id) => self.data.project_label(id),
                    None => tr!("common.phone_call"),
                };
                (color, format_duration(a.elapsed()), label)
            }
        };
        let resume = self.data.resume_project().map(|id| self.data.project_label(id));
        let pause_hover = match &resume {
            Some(label) if on_pause => tr!("bar.resume_on", project = label),
            _ if on_pause => tr!("bar.end_break"),
            _ => tr!("bar.break"),
        };
        let call_hover = match &resume {
            Some(label) if on_call => tr!("bar.end_call_resume", project = label),
            _ if on_call => tr!("bar.end_call"),
            _ => tr!("bar.phone"),
        };

        let hint = |action| self.shortcut_hint(action);
        let buttons = [
            (BarButton::Fold, "🗖", format!("{}{}", tr!("bar.expand"), hint(Action::Fold)), true, false),
            (BarButton::Stop, "⏹", format!("{}{}", tr!("bar.stop"), hint(Action::Stop)), running, false),
            (BarButton::Pause, if on_pause { "▶" } else { "☕" }, format!("{pause_hover}{}", hint(Action::Pause)), true, on_pause),
            (BarButton::Breathing, "💨", tr!("breathing.window_title"), true, false),
            (BarButton::Call, "📞", format!("{call_hover}{}", hint(Action::Call)), true, on_call),
            (BarButton::Switch, "🔀", format!("{}{}", tr!("bar.switch_project"), hint(Action::SwitchProject)), true, false),
        ];
        let show_breathing = on_pause && self.breathing.is_none();
        let shown = buttons.iter().filter(|b| b.0 != BarButton::Breathing || show_breathing);
        // Teksten læses oppefra og ned i højre side af skærmen, nedefra og op i venstre (#20).
        let upward = self.bar_on_left;
        // Hele bjælken er et håndtag: træk hvor som helst (undtagen på
        // knapperne) for at flytte vinduet. Titellinjen er lille, især på den
        // lodrette bjælke. Fladen lægges først, så knapperne ligger ovenpå.
        let handle = ui.interact(ui.max_rect(), egui::Id::unique("bar_drag"), egui::Sense::drag());
        let mut button_rects = Vec::new();
        let mut clicked = None;
        let mut open_picker = false;
        if vertical {
            // Den vandrette bjælke drejet 90°: prik og tid øverst, projektnavnet
            // læses oppefra og ned, og knapperne står nederst (#19).
            ui.vertical_centered(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 5.0, color);
                let strong = ui.visuals().strong_text_color();
                rotated_text(ui, &time, egui::FontId::monospace(15.0), strong, f32::INFINITY, false, upward);
                ui.add_space(4.0);
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    for (button, icon, hover, enabled, selected) in shown {
                        let response = icon_button(ui, icon, hover, *enabled, *selected);
                        button_rects.push(response.rect);
                        if response.clicked() {
                            clicked = Some(*button);
                        }
                    }
                    // Projektnavnet fylder resten. Et klik på det gør ingenting, så
                    // man kan trække i vinduet dér (#22).
                    let room = (ui.available_height() - 6.0).max(0.0);
                    let weak = ui.visuals().weak_text_color();
                    rotated_text(ui, &label, egui::FontId::proportional(14.0), weak, room, true, upward).on_hover_text(&label);
                });
            });
        } else {
            ui.horizontal_centered(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 5.0, color);
                // Tekst, der kan markeres, tager trækket fra håndtaget; slå det fra,
                // så den vandrette bjælke kan trækkes overalt som den lodrette.
                ui.add(egui::Label::new(RichText::new(&time).monospace().size(15.0).strong()).selectable(false));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (button, icon, hover, enabled, selected) in shown {
                        let response = icon_button(ui, icon, hover, *enabled, *selected);
                        button_rects.push(response.rect);
                        if response.clicked() {
                            clicked = Some(*button);
                        }
                    }
                    // Projektnavnet fylder resten; man kan trække i vinduet dér (#22).
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(&label).weak()).truncate().selectable(false))
                            .on_hover_text(&label);
                    });
                });
            });
        }

        // Flyt først, når musen er trukket et stykke med knappen nede, og aldrig
        // fra en knap: ellers tager vinduesmanageren musen, før et klik på en
        // knap er nået igennem.
        // Efter et træk ser vi ikke altid, at knappen slippes (vinduesmanageren
        // har musen), så et nyt tryk starter forfra.
        if ui.input(|i| i.pointer.any_pressed()) {
            self.bar_drag_sent = false;
        }
        if handle.dragged() {
            let (origin, now) = ui.input(|i| (i.pointer.press_origin(), i.pointer.interact_pos()));
            if let (Some(origin), Some(now)) = (origin, now)
                && !self.bar_drag_sent
                && origin.distance(now) > 4.0
                && !button_rects.iter().any(|r| r.contains(origin))
            {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                self.bar_drag_sent = true;
            }
        } else {
            self.bar_drag_sent = false;
        }

        match clicked {
            Some(BarButton::Fold) => self.data.compact = false,
            Some(BarButton::Stop) => self.data.stop(),
            Some(BarButton::Pause) => self.data.toggle_pause(),
            Some(BarButton::Breathing) => self.start_breathing(),
            Some(BarButton::Call) => self.data.toggle_call(),
            Some(BarButton::Switch) => open_picker = true,
            None => {}
        }
        if clicked.is_some() {
            self.save();
        }
        if open_picker {
            self.picker = match self.picker {
                Some(_) => None,
                None => Some(ProjectPicker::default()),
            };
        }
    }

    /// Vælg et projekt at starte tid på, eller opret et nyt. Enter starter det
    /// eneste søgeresultat (eller opretter det nye projekt), Esc lukker.
    pub(super) fn project_picker(&mut self, ui: &mut egui::Ui) {
        let Some(mut picker) = self.picker.take() else { return };
        let active_project = self.data.active.as_ref().and_then(|a| a.project_id);
        let mut chosen = None;
        let mut create = false;
        let mut close = false;

        ui.horizontal(|ui| {
            ui.label(RichText::new(if picker.moving { tr!("picker.move_title") } else { tr!("picker.title") }).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close |= ui.small_button("×").on_hover_text(tr!("picker.close")).clicked();
            });
        });
        let response = ui.add(
            egui::TextEdit::singleline(&mut picker.search)
                .hint_text(tr!("common.search_hint"))
                .desired_width(f32::INFINITY),
        );
        if !picker.focused {
            response.request_focus();
            picker.focused = true;
        }

        let mut matches = Vec::new();
        let complete = !picker.new_customer.trim().is_empty() && !picker.new_project.trim().is_empty();
        // "Nyt projekt" nederst; søgeresultaterne fylder pladsen ovenover.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 90.0) / 2.0;
                ui.add(egui::TextEdit::singleline(&mut picker.new_customer).hint_text(tr!("common.customer")).desired_width(width));
                ui.add(egui::TextEdit::singleline(&mut picker.new_project).hint_text(tr!("common.project")).desired_width(width));
                create |= ui.add_enabled(complete, egui::Button::new(tr!("picker.create"))).on_hover_text(tr!("picker.create_hint")).clicked();
            });
            ui.label(RichText::new(tr!("picker.new_project")).weak().small());
            ui.separator();

            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    for customer in self.data.visible_customers() {
                        let Some(projects) = matching_projects(&self.data, customer, &picker.search) else {
                            continue;
                        };
                        if projects.is_empty() {
                            continue;
                        }
                        ui.label(RichText::new(&customer.name).strong());
                        for project in projects {
                            matches.push(project.id);
                            let is_active = active_project == Some(project.id);
                            let text =
                                if is_active { format!("   ▶ {}", project.name) } else { format!("   {}", project.name) };
                            if ui.selectable_label(is_active, text).clicked() && !is_active {
                                chosen = Some(project.id);
                            }
                        }
                    }
                    if matches.is_empty() {
                        ui.label(RichText::new(tr!("picker.no_match")).weak());
                    }
                });
            });
        });

        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if !picker.search.trim().is_empty() && matches.len() == 1 && active_project != Some(matches[0]) {
                chosen = Some(matches[0]);
            } else if complete {
                create = true;
            }
        }
        close |= ui.input(|i| i.key_pressed(egui::Key::Escape));

        if create && chosen.is_none() {
            let target = CallTarget {
                customer: picker.new_customer.clone(),
                project: picker.new_project.clone(),
                ..Default::default()
            };
            chosen = self.data.resolve_target(&target);
        }
        if let Some(id) = chosen {
            if picker.moving {
                self.data.move_active(id);
            } else {
                self.data.start(id);
            }
            self.save();
            close = true;
        }
        if !close {
            self.picker = Some(picker);
        }
    }
}

/// Knapperne i bjælken.
#[derive(Clone, Copy, PartialEq)]
enum BarButton {
    Fold,
    Stop,
    Pause,
    Breathing,
    Call,
    Switch,
}

/// En lille ikonknap til bjælken. `selected` viser den som trykket ned.
fn icon_button(ui: &mut egui::Ui, icon: &str, hover: &str, enabled: bool, selected: bool) -> egui::Response {
    let text = RichText::new(icon).size(15.0);
    let button = if selected { egui::Button::selectable(true, text) } else { egui::Button::new(text) };
    ui.add_enabled(enabled, button.min_size(Vec2::new(28.0, 24.0))).on_hover_text(hover)
}

/// Tekst drejet 90°, så den læses oppefra og ned, eller nedefra og op med
/// `upward`. Teksten afkortes med … til `max_len`; med `fill` optager den hele
/// `max_len` i højden.
fn rotated_text(
    ui: &mut egui::Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    max_len: f32,
    fill: bool,
    upward: bool,
) -> egui::Response {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_len);
    let galley = ui.painter().layout_job(job);
    let size = galley.size();
    let length = if fill { max_len.max(size.x) } else { size.x };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(size.y, length), egui::Sense::hover());
    // Teksten drejes om sit øverste venstre hjørne. Med uret havner den til
    // venstre for punktet, så det skal være rektanglets øverste højre hjørne;
    // mod uret går den opad og til højre fra nederste venstre hjørne.
    let (corner, angle) = if upward {
        (rect.left_bottom(), -std::f32::consts::FRAC_PI_2)
    } else {
        (rect.right_top(), std::f32::consts::FRAC_PI_2)
    };
    let shape = egui::epaint::TextShape::new(corner, galley, color).with_angle(angle);
    ui.painter().add(shape);
    response
}

/// Den skærm vinduet står på, i skærmpunkter.
fn current_screen(ctx: &egui::Context, frame: &eframe::Frame) -> Option<egui::Rect> {
    let monitor = frame.winit_window()?.current_monitor()?;
    let rect = monitor_rect(&monitor);
    let ppp = ctx.pixels_per_point();
    Some(egui::Rect::from_min_max(rect.min / ppp, rect.max / ppp))
}
