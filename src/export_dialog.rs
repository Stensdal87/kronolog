//! Dialog til eksport af en periode til Excel eller CSV.

use std::path::PathBuf;
use std::time::{Duration as StdDuration, Instant};

use chrono::{Datelike, Duration, Local, Months, NaiveDate};
use eframe::egui::{self, RichText};

use crate::data::{format_duration, Data, Scope};
use crate::entry_editor::parse_date;
use crate::export::{self, CsvContent};

#[derive(Clone, Copy, PartialEq)]
enum FileFormat {
    Excel,
    Csv,
}

pub struct ExportDialog {
    from: String,
    to: String,
    format: FileFormat,
    csv_content: CsvContent,
    /// Kun én kunde eller ét projekt (fra vinduet med registreringer).
    scope: Option<Scope>,
    /// Resultatet af sidste eksport: stien eller en fejl.
    result: Option<Result<PathBuf, String>>,
    /// Sat når "Gem som" venter på, at vinduet ikke længere ligger øverst.
    save_requested: Option<Instant>,
}

/// Hvor længe vi venter på, at vinduesmanageren har sænket vinduet.
const LOWER_DELAY: StdDuration = StdDuration::from_millis(150);

impl ExportDialog {
    /// Åbner med indeværende måned som periode.
    pub fn new() -> Self {
        let (from, to) = this_month(Local::now().date_naive());
        Self {
            from: fmt(from),
            to: fmt(to),
            format: FileFormat::Excel,
            csv_content: CsvContent::Entries,
            scope: None,
            result: None,
            save_requested: None,
        }
    }

    /// Eksport af én kunde eller ét projekt, med en periode valgt på forhånd.
    pub fn for_scope(scope: Scope, from: NaiveDate, to: NaiveDate) -> Self {
        Self { from: fmt(from), to: fmt(to), scope: Some(scope), ..Self::new() }
    }

    fn period(&self) -> Result<(NaiveDate, NaiveDate), String> {
        let from = parse_date(&self.from).ok_or_else(|| tr!("export.err_from"))?;
        let to = parse_date(&self.to).ok_or_else(|| tr!("export.err_to"))?;
        if to < from {
            return Err(tr!("export.err_order"));
        }
        Ok((from, to))
    }

    fn set_period(&mut self, (from, to): (NaiveDate, NaiveDate)) {
        self.from = fmt(from);
        self.to = fmt(to);
        self.result = None;
    }

    /// Returnerer false når dialogen skal lukkes. `frame` er vores vindue, som
    /// "Gem som"-dialogen lægges oven på. `always_on_top` er om vinduet er sat
    /// til at ligge øverst; så slås det fra, mens "Gem som" er åben.
    pub fn show(&mut self, ctx: &egui::Context, data: &Data, frame: &eframe::Frame, always_on_top: bool) -> bool {
        let today = Local::now().date_naive();
        let period = self.period();
        let rows = period.clone().map(|(from, to)| export::collect(data, from, to, self.scope));
        let mut keep_open = true;
        let mut export = false;

        let modal = egui::Modal::new(egui::Id::unique("export_dialog")).show(ctx, |ui| {
            ui.set_width(290.0);
            ui.heading(tr!("export.title"));
            if let Some(scope) = self.scope {
                ui.label(RichText::new(data.scope_label(scope)).strong());
            }
            ui.add_space(4.0);

            ui.label(RichText::new(tr!("export.period")).strong());
            ui.horizontal_wrapped(|ui| {
                let quick: [(String, (NaiveDate, NaiveDate)); 5] = [
                    (tr!("export.today"), (today, today)),
                    (tr!("export.this_week"), this_week(today)),
                    (tr!("export.last_week"), this_week(today - Duration::days(7))),
                    (tr!("export.this_month"), this_month(today)),
                    (tr!("export.last_month"), this_month(first_of_month(today) - Duration::days(1))),
                ];
                for (label, range) in quick {
                    if ui.small_button(label).clicked() {
                        self.set_period(range);
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr!("export.from"));
                if ui.add(egui::TextEdit::singleline(&mut self.from).desired_width(90.0)).changed() {
                    self.result = None;
                }
                ui.label(tr!("export.to"));
                if ui.add(egui::TextEdit::singleline(&mut self.to).desired_width(90.0)).changed() {
                    self.result = None;
                }
            });
            ui.label(RichText::new(tr!("export.whole_days")).weak().small());
            ui.add_space(6.0);

            ui.label(RichText::new(tr!("export.format")).strong());
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.format, FileFormat::Excel, "Excel (.xlsx)");
                ui.selectable_value(&mut self.format, FileFormat::Csv, "CSV");
            });
            match self.format {
                FileFormat::Excel => {
                    ui.label(RichText::new(tr!("export.excel_sheets")).weak().small());
                }
                FileFormat::Csv => {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.csv_content, CsvContent::Entries, tr!("export.all_entries"));
                        ui.selectable_value(&mut self.csv_content, CsvContent::Summary, tr!("export.summary"));
                    });
                }
            }
            ui.add_space(6.0);

            let has_rows = match &rows {
                Ok(rows) if rows.is_empty() => {
                    ui.label(RichText::new(tr!("export.none")).weak());
                    false
                }
                Ok(rows) => {
                    let total = rows.iter().fold(Duration::zero(), |sum, r| sum + r.duration());
                    ui.label(tr!("export.summary_line", count = rows.len(), total = format_duration(total)));
                    true
                }
                Err(e) => {
                    ui.colored_label(ui.visuals().error_fg_color, e.as_str());
                    false
                }
            };

            match &self.result {
                Some(Ok(path)) => {
                    ui.colored_label(egui::Color32::from_rgb(80, 170, 90), tr!("export.saved", path = path.display()));
                    if ui.small_button(tr!("export.open_file")).clicked() {
                        open_file(path);
                    }
                }
                Some(Err(e)) => {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
                None => {}
            }
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                export |= ui.add_enabled(has_rows, egui::Button::new(tr!("export.export_button"))).on_hover_text("Enter").clicked();
                if ui.button(tr!("common.close")).on_hover_text("Esc").clicked() {
                    keep_open = false;
                }
            });
            export |= has_rows && ui.input(|i| i.key_pressed(egui::Key::Enter));
        });

        if modal.should_close() {
            keep_open = false;
        }
        if export && self.save_requested.is_none() {
            // "Gem som" blokerer, til den lukkes, så vinduet skal sænkes en frame
            // før, ellers kan den havne bag et vindue der ligger øverst.
            if always_on_top {
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
            }
            self.save_requested = Some(Instant::now());
        }
        if let Some(requested) = self.save_requested {
            let waited = requested.elapsed();
            if !always_on_top || waited >= LOWER_DELAY {
                self.save_requested = None;
                if let (Ok(rows), Ok((from, to))) = (&rows, period)
                    && let Some(path) = self.ask_path(data, from, to, frame)
                {
                    let result = match self.format {
                        FileFormat::Excel => export::write_xlsx(&path, rows, &rust_i18n::locale()),
                        FileFormat::Csv => export::write_csv(&path, rows, self.csv_content, &rust_i18n::locale()),
                    };
                    self.result = Some(result.map(|()| path));
                }
                if always_on_top {
                    ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
                }
            } else if keep_open {
                ctx.request_repaint_after(LOWER_DELAY - waited);
            } else {
                // Lukket inden "Gem som" nåede at åbne: læg vinduet øverst igen.
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
            }
        }
        keep_open
    }

    /// Viser systemets "Gem som"-dialog. Starter i Overførsler/Downloads.
    fn ask_path(&self, data: &Data, from: NaiveDate, to: NaiveDate, frame: &eframe::Frame) -> Option<PathBuf> {
        let (ext, filter) = match self.format {
            FileFormat::Excel => ("xlsx", "Excel"),
            FileFormat::Csv => ("csv", "CSV"),
        };
        let scope = self.scope.map(|s| format!("_{}", file_safe(&data.scope_label(s)))).unwrap_or_default();
        let name = format!("kronolog{scope}_{}_{}.{ext}", from.format("%Y-%m-%d"), to.format("%Y-%m-%d"));
        // Med vores vindue som forælder lægger vinduesmanageren dialogen ovenpå,
        // også når KronoLog er sat til at ligge øverst.
        let mut dialog = rfd::FileDialog::new()
            .set_parent(frame)
            .set_title(tr!("export.save_title"))
            .set_file_name(name)
            .add_filter(filter, &[ext]);
        if let Some(dir) = directories::UserDirs::new().and_then(|d| d.download_dir().map(PathBuf::from)) {
            dialog = dialog.set_directory(dir);
        }
        let path = dialog.save_file()?;
        // Sørg for den rigtige endelse, hvis den er blevet slettet i dialogen.
        Some(if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            path
        } else {
            path.with_extension(ext)
        })
    }
}

/// Navnet som en del af et filnavn: kun bogstaver, tal, - og _.
fn file_safe(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() || c == '-' {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

fn fmt(date: NaiveDate) -> String {
    date.format("%d-%m-%Y").to_string()
}

/// Mandag til søndag i ugen, der indeholder `day`.
pub(crate) fn this_week(day: NaiveDate) -> (NaiveDate, NaiveDate) {
    let monday = day - Duration::days(day.weekday().num_days_from_monday() as i64);
    (monday, monday + Duration::days(6))
}

fn first_of_month(day: NaiveDate) -> NaiveDate {
    day.with_day(1).unwrap_or(day)
}

/// Første til sidste dag i måneden, der indeholder `day`.
pub(crate) fn this_month(day: NaiveDate) -> (NaiveDate, NaiveDate) {
    let first = first_of_month(day);
    let last = first.checked_add_months(Months::new(1)).map_or(day, |next| next - Duration::days(1));
    (first, last)
}

/// Åbner filen i det program, systemet bruger til den slags filer.
fn open_file(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    let _ = result;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn scope_name_is_safe_in_file_names() {
        assert_eq!(file_safe("Ny Kunde / Hotline"), "Ny_Kunde_Hotline");
        assert_eq!(file_safe("Ærø: \"Test\""), "Ærø_Test");
    }

    #[test]
    fn week_runs_monday_to_sunday() {
        // 20-10-2026 er en tirsdag.
        assert_eq!(this_week(d(2026, 10, 20)), (d(2026, 10, 19), d(2026, 10, 25)));
        assert_eq!(this_week(d(2026, 10, 25)), (d(2026, 10, 19), d(2026, 10, 25)));
    }

    #[test]
    fn month_handles_lengths_and_year_end() {
        assert_eq!(this_month(d(2026, 10, 20)), (d(2026, 10, 1), d(2026, 10, 31)));
        assert_eq!(this_month(d(2028, 2, 10)), (d(2028, 2, 1), d(2028, 2, 29)));
        assert_eq!(this_month(d(2026, 12, 31)), (d(2026, 12, 1), d(2026, 12, 31)));
    }
}
