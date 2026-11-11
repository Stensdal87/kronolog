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
        let mut toggle = None;
        let mut open_day = None;
        // Projektnavnet afkortes, så ugen kan ses i det smalle vindue.
        // Hver tidskolonne er ca. 36 px ("0:00" i 12 px plus mellemrum); navnet er ca. 6 px pr. tegn.
        let label_chars = ((ui.available_width() - 16.0 - (shown.len() + 1) as f32 * 36.0) / 6.0).max(8.0) as usize;
        let cell = |text: String| RichText::new(text).monospace().size(12.0);

        egui::ScrollArea::both().show(ui, |ui| {
            // Tiderne er knapper uden ramme; uden knappernes luft er der plads
            // til hele ugen i det smalle vindue.
            ui.spacing_mut().button_padding = Vec2::ZERO;
            egui::Grid::new("week").striped(true).spacing([8.0, 6.0]).min_col_width(0.0).show(ui, |ui| {
                ui.label("");
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
                ui.end_row();

                let mut day_totals = [Duration::zero(); 7];
                for (project_id, row) in &rows {
                    let full = self.data.project_label(*project_id);
                    ui.label(RichText::new(shorten(&full, label_chars)).size(12.0)).on_hover_text(&full);
                    for &d in &shown {
                        let date = monday + Duration::days(d as i64);
                        let Some(time) = row[d] else {
                            ui.label(cell("·".into()).weak());
                            continue;
                        };
                        day_totals[d] += time;
                        let state = self.data.billed_state(date, *project_id, time);
                        let mut text = cell(format_hours_minutes(time));
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
                        let hover = format!("{full}\n{}\n{hover}", date.format("%d-%m-%Y"));
                        if ui.add(egui::Button::new(text).frame(false)).on_hover_text(hover).clicked() {
                            // Ændret tid krydses af med den nye; ellers skiftes krydset.
                            let bill = state != BilledState::Yes;
                            toggle = Some((date, *project_id, bill.then_some(time)));
                        }
                    }
                    let total = row.iter().flatten().fold(Duration::zero(), |s, d| s + *d);
                    ui.label(cell(format_hours_minutes(total)).strong());
                    ui.end_row();
                }

                ui.label(RichText::new(tr!("history.total")).weak().small());
                for &d in &shown {
                    ui.label(cell(format_hours_minutes(day_totals[d])).strong());
                }
                let week_total = day_totals.iter().fold(Duration::zero(), |s, d| s + *d);
                ui.label(cell(format_hours_minutes(week_total)).strong());
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.label(RichText::new(tr!("week.hint")).weak().small());
        });

        if let Some((date, project_id, time)) = toggle {
            self.data.set_billed(date, project_id, time);
            self.save();
        }
        if let Some(date) = open_day {
            self.history_date = date;
            self.history_view = HistoryView::Summary;
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
