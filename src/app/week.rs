//! Ugeoversigten i Historik: projekter nedad, ugedage henad, med den afrundede
//! tid (samme som i Summeret) og totaler, som et timeregnskab. Et klik på en
//! dag krydser projektets tid den dag af (eller fjerner krydset); et klik på en
//! ugedag åbner dagen.

use chrono::Datelike;

use super::*;

/// Ugens afrundede tid pr. projekt og dag (mandag først). Rækkerne står med
/// mest tid først.
pub(super) fn week_rows(data: &Data, monday: NaiveDate) -> Vec<(Id, [Option<Duration>; 7])> {
    let mut rows: Vec<(Id, [Option<Duration>; 7])> = Vec::new();
    for day in 0..7 {
        let date = monday + Duration::days(day as i64);
        for (project_id, time) in data.totals_on(date) {
            let rounded = data.rounded(date, project_id, time).time;
            let row = match rows.iter_mut().find(|(id, _)| *id == project_id) {
                Some((_, row)) => row,
                None => {
                    rows.push((project_id, [None; 7]));
                    &mut rows.last_mut().expect("lige tilføjet").1
                }
            };
            row[day] = Some(rounded);
        }
    }
    let total = |row: &[Option<Duration>; 7]| row.iter().flatten().fold(Duration::zero(), |s, d| s + *d);
    rows.sort_by_key(|(_, row)| std::cmp::Reverse(total(row)));
    rows
}

/// Mandagen i ugen med `date`.
pub(super) fn monday_of(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

impl KronoLogApp {
    pub(super) fn week_view(&mut self, ui: &mut egui::Ui) {
        let monday = monday_of(self.history_date);
        let rows = week_rows(&self.data, monday);
        if rows.is_empty() {
            ui.label(RichText::new(tr!("week.none")).weak());
            return;
        }
        // Weekenden vises kun, hvis der er tid på den.
        let shown: Vec<usize> = (0..7).filter(|&d| d < 5 || rows.iter().any(|(_, row)| row[d].is_some())).collect();
        let day_names = tr!("week.days");
        let day_names: Vec<&str> = day_names.split(',').collect();
        let today = Local::now().date_naive();
        let sunday = monday + Duration::days(6);
        let mut toggle = None;
        let mut open_day = None;
        let mut toggle_fold = None;
        let mut open_editor = None;
        let mut show_entries = None;
        // Projektnavnet afkortes, så ugen kan ses i det smalle vindue.
        // Hver tidskolonne er ca. 36 px ("0:00" i 12 px plus mellemrum); navnet er ca. 6 px pr. tegn,
        // og linkkolonnen tager ca. 26 px mere.
        let any_link = rows.iter().any(|(id, _)| self.data.link_for(*id).is_some());
        let rest = 16.0 + (shown.len() + 1) as f32 * 36.0 + if any_link { 26.0 } else { 0.0 };
        let label_chars = ((ui.available_width() - rest) / 6.0).max(8.0) as usize;
        let cell = |text: String| RichText::new(text).monospace().size(12.0);
        // Timer (t:mm) eller kommatal (1,75) til fx Salesforce; højreklik kopierer
        // altid kommatallet.
        let separator = tr!("history.decimal_separator");
        let decimal = self.data.week_decimal;
        let shown_time = |d: Duration| if decimal { format_decimal_hours(d, &separator) } else { format_hours_minutes(d) };
        let mut switch_format = false;
        let mut copy = None;
        // En tid eller total, der kan højreklikkes for at kopiere kommatallet.
        let copyable = |ui: &mut egui::Ui, text: RichText, time: Duration, copy: &mut Option<String>| {
            let hours = format_decimal_hours(time, &separator);
            let response = ui
                .add(egui::Label::new(text).sense(egui::Sense::click()))
                .on_hover_text(tr!("week.copy_hint", hours = hours.as_str()));
            if response.secondary_clicked() {
                *copy = Some(hours);
            }
        };

        egui::ScrollArea::both().show(ui, |ui| {
            // Tiderne er knapper uden ramme; uden knappernes luft er der plads
            // til hele ugen i det smalle vindue.
            ui.spacing_mut().button_padding = Vec2::ZERO;
            egui::Grid::new("week").striped(true).spacing([8.0, 6.0]).min_col_width(0.0).show(ui, |ui| {
                // Hjørnet skifter mellem timer og kommatal og viser, hvad der vises nu.
                let (format, hint) = if decimal {
                    (format!("0{separator}00"), tr!("week.show_hours"))
                } else {
                    (tr!("week.format_hours"), tr!("week.show_decimal"))
                };
                let format = RichText::new(format).monospace().small().color(ACCENT);
                if super::hover_button(ui, format).on_hover_text(hint).clicked() {
                    switch_format = true;
                }
                for &d in &shown {
                    let date = monday + Duration::days(d as i64);
                    let mut name = RichText::new(day_names.get(d).copied().unwrap_or("")).small();
                    name = if date == today { name.strong() } else { name.weak() };
                    let hover = tr!("week.open_day", date = date.format("%d-%m-%Y"));
                    if ui.add(egui::Button::new(name).frame(false)).on_hover_text(hover).clicked() {
                        open_day = Some(date);
                    }
                }
                ui.label(RichText::new(tr!("history.total")).weak().small());
                if any_link {
                    ui.label(RichText::new(tr!("link.col")).weak().small()).on_hover_text(tr!("link.col_hint"));
                }
                ui.end_row();

                let mut day_totals = [Duration::zero(); 7];
                for (project_id, row) in &rows {
                    let full = self.data.project_label(*project_id);
                    // Klik på projektet folder ugens registreringer ud, så noterne kan læses.
                    let open = self.history_open.contains(project_id);
                    let arrow = if open { "⏷" } else { "⏵" };
                    let label = RichText::new(format!("{arrow} {}", shorten(&full, label_chars))).size(12.0);
                    let mut response = ui.add(egui::Button::new(label).frame(false));
                    if response.clicked() {
                        toggle_fold = Some(*project_id);
                    }
                    response = response.on_hover_text(format!("{full}\n{}", tr!("week.fold_hint")));
                    response.context_menu(|ui| {
                        super::link_menu_item(ui, self.data.link_for(*project_id));
                        if ui.button(tr!("item.show")).clicked() {
                            show_entries = Some(Scope::Project(*project_id));
                            ui.close();
                        }
                    });
                    for &d in &shown {
                        let date = monday + Duration::days(d as i64);
                        let Some(time) = row[d] else {
                            ui.label(cell("·".into()).weak());
                            continue;
                        };
                        day_totals[d] += time;
                        let state = self.data.billed_state(date, *project_id, time);
                        let mut text = cell(shown_time(time));
                        let hover = match state {
                            BilledState::No => tr!("week.click_to_bill"),
                            BilledState::Yes => {
                                text = text.color(BILLED_COLOR);
                                tr!("history.billed", time = format_hours_minutes(time))
                            }
                            BilledState::Changed(old) => {
                                text = text.color(ui.visuals().warn_fg_color);
                                tr!("week.billed_changed", old = format_hours_minutes(old))
                            }
                        };
                        let hours = format_decimal_hours(time, &separator);
                        let copy_hint = tr!("week.copy_hint", hours = hours.as_str());
                        let hover = format!("{full}\n{}\n{hover}\n{copy_hint}", date.format("%d-%m-%Y"));
                        let response = super::hover_button(ui, text).on_hover_text(hover);
                        if response.clicked() {
                            // Ændret tid krydses af med den nye; ellers skiftes krydset.
                            let bill = state != BilledState::Yes;
                            toggle = Some((date, *project_id, bill.then_some(time)));
                        }
                        if response.secondary_clicked() {
                            copy = Some(hours);
                        }
                    }
                    let total = row.iter().flatten().fold(Duration::zero(), |s, d| s + *d);
                    copyable(ui, cell(shown_time(total)).strong(), total, &mut copy);
                    // Link til fx Salesforce, hvor tiden skal registreres (#41).
                    if any_link {
                        match self.data.link_for(*project_id) {
                            Some(link) => super::link_button(ui, link),
                            None => {
                                ui.label("");
                            }
                        }
                    }
                    ui.end_row();

                    if open {
                        // Ugens registreringer med klokkeslæt og note; tiden står i
                        // sin egen ugedags kolonne, så det er til at se hvornår.
                        for day_entry in self.data.entries_in(Scope::Project(*project_id), Some(monday), Some(sunday)) {
                            let e = &day_entry.entry;
                            let day = e.start.date_naive().weekday().num_days_from_monday() as usize;
                            let mut times = format!("{}–{}", e.start.format("%H:%M"), e.end.format("%H:%M"));
                            match e.note.as_deref() {
                                Some(NOTE_PAUSE) => times.push_str(" ☕"),
                                Some(NOTE_PHONE) => times.push_str(" 📞"),
                                _ => {}
                            }
                            ui.horizontal(|ui| {
                                ui.add_space(12.0);
                                ui.vertical(|ui| {
                                    let text = RichText::new(times).monospace().size(11.0).weak();
                                    // Dagskolonnen viser t:mm; den nøjagtige længde står i hjælpeteksten.
                                    let length = format_duration(e.duration());
                                    match day_entry.index {
                                        Some(index) => {
                                            let response = ui
                                                .add(egui::Button::new(text).frame(false))
                                                .on_hover_text(format!("{length}\n{}", tr!("history.click_to_edit")));
                                            if response.clicked() {
                                                open_editor = Some(EntryEditor::edit(index, e));
                                            }
                                        }
                                        // Den kørende timer kan ikke rettes, før den er stoppet.
                                        None => {
                                            ui.label(text.color(ACCENT))
                                                .on_hover_text(format!("{length}\n{}", tr!("history.running")));
                                        }
                                    }
                                    if !e.description.is_empty() {
                                        ui.label(RichText::new(format!("📝 {}", shorten(&e.description, label_chars))).weak().small())
                                            .on_hover_text(&e.description);
                                    }
                                });
                            });
                            for &d in &shown {
                                if d == day {
                                    ui.label(cell(shown_time(e.duration())).weak());
                                } else {
                                    ui.label("");
                                }
                            }
                            ui.label("");
                            if any_link {
                                ui.label("");
                            }
                            ui.end_row();
                        }
                    }
                }

                ui.label(RichText::new(tr!("history.total")).weak().small());
                for &d in &shown {
                    copyable(ui, cell(shown_time(day_totals[d])).strong(), day_totals[d], &mut copy);
                }
                let week_total = day_totals.iter().fold(Duration::zero(), |s, d| s + *d);
                copyable(ui, cell(shown_time(week_total)).strong(), week_total, &mut copy);
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.label(RichText::new(tr!("week.hint")).weak().small());
        });

        if let Some((date, project_id, time)) = toggle {
            self.data.set_billed(date, project_id, time);
            self.save();
        }
        if switch_format {
            self.data.week_decimal = !self.data.week_decimal;
            self.save();
        }
        if let Some(hours) = copy {
            super::copy_with_notice(ui.ctx(), hours);
        }
        if let Some(date) = open_day {
            self.history_date = date;
            self.history_view = HistoryView::Summary;
        }
        if let Some(project_id) = toggle_fold
            && !self.history_open.remove(&project_id)
        {
            self.history_open.insert(project_id);
        }
        if open_editor.is_some() {
            self.editor = open_editor;
        }
        if let Some(scope) = show_entries {
            self.open_item_history(scope);
        }
    }
}

/// Krydset af: grøn.
const BILLED_COLOR: Color32 = Color32::from_rgb(80, 170, 100);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{TimeEntry, round_up_quarter};
    use chrono::TimeZone;

    #[test]
    fn week_rows_round_each_day_and_sort_by_total() {
        let mut data = Data::default();
        let c = data.add_customer("K");
        let (a, b) = (data.add_project(c, "A"), data.add_project(c, "B"));
        let at = |day, h, m| Local.with_ymd_and_hms(2026, 11, day, h, m, 0).unwrap();
        // Mandag 9. og onsdag 11. november 2026.
        data.entries = vec![
            TimeEntry { project_id: a, start: at(9, 8, 0), end: at(9, 8, 20), ..Default::default() },
            TimeEntry { project_id: b, start: at(9, 9, 0), end: at(9, 11, 0), ..Default::default() },
            TimeEntry { project_id: a, start: at(11, 8, 0), end: at(11, 8, 50), ..Default::default() },
        ];
        let monday = monday_of(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap());
        assert_eq!(monday, NaiveDate::from_ymd_opt(2026, 11, 9).unwrap());
        let rows = week_rows(&data, monday);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, b, "mest tid først (2:00 mod 0:30 + 1:00)");
        assert_eq!(rows[1].1[0], Some(round_up_quarter(Duration::minutes(20))), "rundet op pr. dag");
        assert_eq!(rows[1].1[2], Some(Duration::hours(1)));
        assert_eq!(rows[1].1[1], None);
    }
}
