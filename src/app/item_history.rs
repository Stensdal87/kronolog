//! Vindue med registreringerne for én kunde eller ét projekt (højreklik i
//! projektlisten). Som historikken, men over en dag, uge, måned eller altid, og
//! med rettelse og eksport.

use chrono::{Datelike, Months};

use super::*;
use crate::data::Scope;
use crate::export_dialog::{this_month, this_week};

#[derive(Clone, Copy, PartialEq)]
enum Period {
    Day,
    Week,
    Month,
    All,
}

pub(super) struct ItemHistory {
    scope: Scope,
    period: Period,
    /// En dag i den viste periode.
    anchor: NaiveDate,
    view: HistoryView,
    /// Placering (midt på hovedvinduets skærm), fundet når vinduet åbner.
    position: Option<Option<Pos2>>,
    /// Rettelse og eksport vises i dette vindue, ikke i hovedvinduet.
    editor: Option<EntryEditor>,
    export: Option<ExportDialog>,
}

impl ItemHistory {
    pub(super) fn new(scope: Scope) -> Self {
        Self {
            scope,
            period: Period::Month,
            anchor: Local::now().date_naive(),
            view: HistoryView::Summary,
            position: None,
            editor: None,
            export: None,
        }
    }

    /// Første og sidste dag; `None` er altid.
    fn range(&self) -> Option<(NaiveDate, NaiveDate)> {
        match self.period {
            Period::Day => Some((self.anchor, self.anchor)),
            Period::Week => Some(this_week(self.anchor)),
            Period::Month => Some(this_month(self.anchor)),
            Period::All => None,
        }
    }

    /// Én periode frem (`forward`) eller tilbage.
    fn step(&mut self, forward: bool) {
        let days = |n: i64| Duration::days(if forward { n } else { -n });
        self.anchor = match self.period {
            Period::Day => self.anchor + days(1),
            Period::Week => self.anchor + days(7),
            Period::Month => {
                let first = self.anchor.with_day(1).unwrap_or(self.anchor);
                let moved = if forward {
                    first.checked_add_months(Months::new(1))
                } else {
                    first.checked_sub_months(Months::new(1))
                };
                moved.unwrap_or(first)
            }
            Period::All => self.anchor,
        };
    }

    fn period_label(&self) -> String {
        let a = self.anchor;
        match self.period {
            Period::Day => a.format("%d-%m-%Y").to_string(),
            Period::Week => {
                let week = a.iso_week();
                tr!("item.week_label", week = week.week(), year = week.year())
            }
            Period::Month => {
                let key = format!("month.{}", a.month());
                tr!("item.month_label", month = tr!(&key), year = a.year())
            }
            Period::All => String::new(),
        }
    }
}

impl TimeSnapperApp {
    pub(super) fn open_item_history(&mut self, scope: Scope) {
        self.item_history = Some(ItemHistory::new(scope));
    }

    /// Viser vinduet, mens det er åbent.
    pub(super) fn item_history_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let Some(scope) = self.item_history.as_ref().map(|h| h.scope) else { return };
        // Kunden eller projektet er slettet.
        let exists = match scope {
            Scope::Customer(id) => self.data.customer(id).is_some(),
            Scope::Project(id) => self.data.project(id).is_some(),
        };
        if !exists {
            self.item_history = None;
            return;
        }
        let level = if self.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
        let size = Vec2::new(460.0, 540.0);
        let position = self
            .item_history
            .as_mut()
            .and_then(|h| *h.position.get_or_insert_with(|| centered_on_screen(frame, size)));
        let mut builder = egui::ViewportBuilder::default()
            .with_title(tr!("item.window_title", name = self.data.scope_label(scope)))
            .with_inner_size(size)
            .with_min_inner_size([340.0, 300.0])
            .with_window_level(level);
        // Ellers vælger Windows selv et sted (#13). Samme placering hver frame,
        // så vinduet stadig kan flyttes.
        if let Some(position) = position {
            builder = builder.with_position(position);
        }
        let mut close = false;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("item_history"), builder, |ui, _class| {
            close |= ui.input(|i| i.viewport().close_requested());
            let child = ui.ctx().clone();
            egui::CentralPanel::default().show(ui, |ui| self.item_history_ui(ui));
            self.item_history_dialogs(&child, frame);
        });
        if close {
            self.item_history = None;
        }
    }

    fn item_history_dialogs(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let always_on_top = self.always_on_top;
        let Some(history) = self.item_history.as_mut() else { return };
        if let Some(editor) = history.editor.as_mut() {
            let done = match editor.show(ctx, &self.data) {
                EditorAction::None => false,
                EditorAction::Cancel => true,
                EditorAction::Save(Some(index), entry) => {
                    self.data.update_entry(index, entry);
                    true
                }
                EditorAction::Save(None, entry) => {
                    self.data.add_entry(entry);
                    true
                }
                EditorAction::Delete(index) => {
                    self.data.delete_entry(index);
                    true
                }
            };
            if done {
                history.editor = None;
                self.save();
                return;
            }
        }
        if let Some(export) = history.export.as_mut()
            && !export.show(ctx, &self.data, frame, always_on_top)
        {
            history.export = None;
        }
    }

    fn item_history_ui(&mut self, ui: &mut egui::Ui) {
        let Some(history) = self.item_history.as_mut() else { return };
        let scope = history.scope;
        let today = Local::now().date_naive();
        ui.heading(self.data.scope_label(scope));
        ui.add_space(2.0);

        ui.horizontal(|ui| {
            for (period, label) in [
                (Period::Day, tr!("item.day")),
                (Period::Week, tr!("item.week")),
                (Period::Month, tr!("item.month")),
                (Period::All, tr!("item.all_time")),
            ] {
                ui.selectable_value(&mut history.period, period, label);
            }
        });
        let range = history.range();
        let mut add = false;
        let mut export = false;
        ui.horizontal(|ui| {
            if range.is_some() {
                if ui.button("◀").on_hover_text(tr!("item.earlier")).clicked() {
                    history.step(false);
                }
                ui.label(RichText::new(history.period_label()).strong());
                if ui.button("▶").on_hover_text(tr!("item.later")).clicked() {
                    history.step(true);
                }
                if range.is_some_and(|(from, to)| !(from..=to).contains(&today))
                    && ui.small_button(tr!("history.today")).on_hover_text(tr!("item.back_to_now")).clicked()
                {
                    history.anchor = today;
                }
            } else {
                let first = self.data.entries_in(scope, None, None).first().map(|e| e.entry.start.date_naive());
                if let Some(first) = first {
                    ui.label(RichText::new(tr!("item.since", date = first.format("%d-%m-%Y"))).strong());
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                export = ui.button("📤").on_hover_text(tr!("history.export_hint")).clicked();
                add = ui.button("➕").on_hover_text(tr!("history.add_hint")).clicked();
            });
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut history.view, HistoryView::Summary, tr!("history.summary"));
            ui.selectable_value(&mut history.view, HistoryView::All, tr!("history.all"));
        });
        ui.separator();

        // Perioden kan være skiftet ovenfor.
        let range = history.range();
        let entries = self.data.entries_in(scope, range.map(|r| r.0), range.map(|r| r.1));
        if add {
            let date = match range {
                Some((from, to)) if !(from..=to).contains(&today) => from,
                _ => today,
            };
            let last_end = entries.iter().filter(|e| e.index.is_some() && e.entry.start.date_naive() == date).map(|e| e.entry.end).max();
            let project = match scope {
                Scope::Project(id) => Some(id),
                Scope::Customer(_) => None,
            };
            history.editor = Some(EntryEditor::new(date, last_end).with_project(project));
        }
        if export {
            let (from, to) = range.unwrap_or_else(|| {
                let first = entries.first().map_or(today, |e| e.entry.start.date_naive());
                (first, today.max(first))
            });
            history.export = Some(ExportDialog::for_scope(scope, from, to));
        }

        if entries.is_empty() {
            ui.label(RichText::new(tr!("item.none")).weak());
            return;
        }

        // Dagene med registreringer, nyeste først, og pr. dag tiden pr. projekt.
        let mut days: Vec<(NaiveDate, Vec<&DayEntry>)> = Vec::new();
        for e in &entries {
            let day = e.entry.start.date_naive();
            match days.iter_mut().find(|(d, _)| *d == day) {
                Some((_, list)) => list.push(e),
                None => days.push((day, vec![e])),
            }
        }
        days.sort_by_key(|(d, _)| std::cmp::Reverse(*d));
        let per_project = |list: &[&DayEntry]| {
            let mut totals: Vec<(Id, usize, Duration)> = Vec::new();
            for e in list {
                match totals.iter_mut().find(|(id, ..)| *id == e.entry.project_id) {
                    Some((_, n, d)) => {
                        *n += 1;
                        *d += e.entry.duration();
                    }
                    None => totals.push((e.entry.project_id, 1, e.entry.duration())),
                }
            }
            totals.sort_by_key(|(.., d)| std::cmp::Reverse(*d));
            totals
        };
        let data = &self.data;
        let show_project = matches!(scope, Scope::Customer(_));
        let project_name = |id: Id| data.project(id).map_or_else(|| tr!("data.deleted_project"), |p| p.name.clone());
        let mut total = Duration::zero();
        let mut rounded_total = Duration::zero();
        let mut open_editor = None;

        egui::ScrollArea::both().auto_shrink([false, true]).show(ui, |ui| {
            // Lange navne må ikke brydes over flere linjer i de smalle kolonner.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            egui::Grid::new("item_history").striped(true).num_columns(4).show(ui, |ui| {
                ui.label("");
                ui.label("");
                ui.label(RichText::new(tr!("history.col_time")).weak().small());
                ui.label(RichText::new(tr!("history.col_rounded")).weak().small())
                    .on_hover_text(tr!("history.col_rounded_hint"));
                ui.end_row();
                for (day, list) in &days {
                    let projects = per_project(list);
                    let day_time = projects.iter().fold(Duration::zero(), |sum, (.., d)| sum + *d);
                    let rounded: Vec<_> = projects.iter().map(|(id, _, d)| data.rounded(*day, *id, *d)).collect();
                    let day_rounded = rounded.iter().fold(Duration::zero(), |sum, r| sum + r.time);
                    total += day_time;
                    rounded_total += day_rounded;

                    ui.label(RichText::new(day.format("%d-%m-%Y").to_string()).strong());
                    ui.label(RichText::new(format!("{}×", list.len())).weak())
                        .on_hover_text(tr!("history.entry_count", count = list.len()));
                    ui.label(RichText::new(format_duration(day_time)).monospace().strong());
                    ui.label(RichText::new(format_hours_minutes(day_rounded)).monospace().strong());
                    ui.end_row();

                    match history.view {
                        HistoryView::Summary => {
                            if !show_project {
                                continue;
                            }
                            for ((id, count, d), r) in projects.iter().zip(&rounded) {
                                ui.label(format!("   {}", project_name(*id)));
                                ui.label(RichText::new(format!("{count}×")).weak());
                                ui.label(RichText::new(format_duration(*d)).monospace());
                                let text = if r.down {
                                    RichText::new(format!("{} ↓", format_hours_minutes(r.time)))
                                        .color(ui.visuals().warn_fg_color)
                                } else {
                                    RichText::new(format_hours_minutes(r.time))
                                };
                                ui.label(text.monospace());
                                ui.end_row();
                            }
                        }
                        HistoryView::All => {
                            for day_entry in list {
                                let e = &day_entry.entry;
                                let mut label = if show_project { project_name(e.project_id) } else { String::new() };
                                if let Some(note) = &e.note {
                                    label = if label.is_empty() { note_label(note) } else { format!("{label} ({})", note_label(note)) };
                                }
                                let times = format!("   {}–{}", e.start.format("%H:%M"), e.end.format("%H:%M"));
                                // Klik på klokkeslæt eller projekt for at rette.
                                let mut clicked = Vec::new();
                                match day_entry.index {
                                    Some(_) => clicked.push(ui.selectable_label(false, RichText::new(times).monospace())),
                                    None => {
                                        ui.label(RichText::new(format!("{times} ▶")).monospace().color(ACCENT))
                                            .on_hover_text(tr!("history.running"));
                                    }
                                }
                                ui.vertical(|ui| {
                                    if !label.is_empty() {
                                        if day_entry.index.is_some() {
                                            clicked.push(ui.selectable_label(false, label));
                                        } else {
                                            ui.label(RichText::new(label).color(ACCENT));
                                        }
                                    }
                                    if !e.description.is_empty() {
                                        ui.label(RichText::new(format!("📝 {}", shorten(&e.description, 50))).weak().small())
                                            .on_hover_text(&e.description);
                                    }
                                });
                                if let Some(index) = day_entry.index {
                                    for response in clicked {
                                        let response = response.on_hover_text(tr!("history.click_to_edit"));
                                        if response.clicked() {
                                            open_editor = Some(EntryEditor::edit(index, e));
                                        }
                                        response.context_menu(|ui| {
                                            if ui.button(tr!("history.edit")).clicked() {
                                                open_editor = Some(EntryEditor::edit(index, e));
                                                ui.close();
                                            }
                                            if ui.button(tr!("history.delete")).clicked() {
                                                open_editor = Some(EntryEditor::delete(index, e));
                                                ui.close();
                                            }
                                        });
                                    }
                                }
                                ui.label(RichText::new(format_duration(e.duration())).monospace());
                                ui.label("");
                                ui.end_row();
                            }
                        }
                    }
                }
                ui.label(RichText::new(tr!("history.total")).strong());
                ui.label("");
                ui.label(RichText::new(format_duration(total)).monospace().strong());
                ui.label(RichText::new(format_hours_minutes(rounded_total)).monospace().strong())
                    .on_hover_text(tr!("history.rounded_total_hint"));
                ui.end_row();
            });
        });
        if open_editor.is_some() {
            history.editor = open_editor;
        }
    }
}

/// Placering (i punkter) der lægger et vindue af størrelsen `size` midt på den
/// skærm, hovedvinduet står på.
fn centered_on_screen(frame: &eframe::Frame, size: Vec2) -> Option<Pos2> {
    let window = frame.winit_window()?;
    let monitor = window.current_monitor().or_else(|| window.primary_monitor())?;
    let scale = monitor.scale_factor() as f32;
    let screen = monitor_rect(&monitor);
    let screen = egui::Rect::from_min_size((screen.min.to_vec2() / scale).to_pos2(), screen.size() / scale);
    Some((screen.center() - size / 2.0).max(screen.min))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn periods_step_by_day_week_and_month() {
        let mut h = ItemHistory::new(Scope::Project(1));
        h.anchor = d(2026, 1, 31);
        h.period = Period::Month;
        h.step(false);
        assert_eq!(h.range(), Some((d(2025, 12, 1), d(2025, 12, 31))));
        h.step(true);
        h.step(true);
        assert_eq!(h.range(), Some((d(2026, 2, 1), d(2026, 2, 28))));
        h.period = Period::Week;
        h.anchor = d(2026, 10, 29);
        h.step(false);
        assert_eq!(h.range(), Some((d(2026, 10, 19), d(2026, 10, 25))));
        h.period = Period::All;
        assert_eq!(h.range(), None);
    }
}
