//! Dialog til at rette, slette eller tilføje en registrering.

use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeDelta, TimeZone, Timelike};
use eframe::egui::{self, Color32, RichText};

use crate::data::{format_duration, Data, Id, TimeEntry, NOTE_PAUSE, NOTE_PHONE};

pub enum EditorAction {
    None,
    Cancel,
    /// Gem registreringen: `Some(index)` retter en eksisterende, `None` tilføjer en ny.
    Save(Option<usize>, TimeEntry),
    Delete(usize),
}

pub struct EntryEditor {
    /// `None` er en ny registrering.
    index: Option<usize>,
    /// Den oprindelige registrering. Uændrede felter beholder deres sekunder.
    original: Option<TimeEntry>,
    project_id: Option<Id>,
    note: Option<String>,
    date: String,
    start: String,
    end: String,
    /// Varigheden i minutter; alternativ til sluttiden (#24).
    minutes: String,
    /// Hvad der sidst blev rettet af sluttid og minutter. Det andet felt følger med.
    end_source: EndSource,
    description: String,
    confirm_delete: bool,
}

/// Hvad sluttiden regnes ud fra, når starttiden ændres.
#[derive(Clone, Copy, PartialEq, Debug)]
enum EndSource {
    /// Ikke rørt: slut er start + 30 min.
    Auto,
    /// Minutterne er tastet: slut er start + minutter.
    Minutes,
    /// Sluttiden er tastet (eller valgt): den står fast, minutterne følger.
    End,
}

/// Standardlængden på en ny registrering, og afstanden i tidsmenuen.
const STEP_MINUTES: i64 = 30;

impl EntryEditor {
    pub fn edit(index: usize, entry: &TimeEntry) -> Self {
        Self {
            index: Some(index),
            original: Some(entry.clone()),
            project_id: Some(entry.project_id),
            note: entry.note.clone(),
            date: entry.start.format("%d-%m-%Y").to_string(),
            start: entry.start.format("%H:%M").to_string(),
            end: entry.end.format("%H:%M").to_string(),
            minutes: (entry.end - entry.start).num_minutes().to_string(),
            end_source: EndSource::End,
            description: entry.description.clone(),
            confirm_delete: false,
        }
    }

    /// Åbner direkte i "er du sikker"-trinnet.
    pub fn delete(index: usize, entry: &TimeEntry) -> Self {
        Self { confirm_delete: true, ..Self::edit(index, entry) }
    }

    /// Ny registrering på en dag. I dag foreslås klokken nu rundet ned til en
    /// halv time (#24); andre dage slutningen af dagens seneste registrering.
    /// Sluttiden sættes til en halv time efter.
    pub fn new(date: NaiveDate, suggested_start: Option<DateTime<Local>>) -> Self {
        let now = Local::now();
        let start = if date == now.date_naive() {
            Some(floor_to_step(now.time()))
        } else {
            suggested_start.map(|t| t.time())
        };
        let mut editor = Self {
            index: None,
            original: None,
            project_id: None,
            note: None,
            date: date.format("%d-%m-%Y").to_string(),
            start: start.map(|t| t.format("%H:%M").to_string()).unwrap_or_default(),
            end: String::new(),
            minutes: STEP_MINUTES.to_string(),
            end_source: EndSource::Auto,
            description: String::new(),
            confirm_delete: false,
        };
        editor.start_changed();
        editor
    }

    /// Starttiden er ændret: flyt sluttiden med (eller ret minutterne, hvis
    /// sluttiden er sat i hånden).
    fn start_changed(&mut self) {
        match self.end_source {
            EndSource::Auto | EndSource::Minutes => self.end_from_minutes(),
            EndSource::End => self.minutes_from_end(),
        }
    }

    fn end_changed(&mut self) {
        self.end_source = EndSource::End;
        self.minutes_from_end();
    }

    fn minutes_changed(&mut self) {
        self.end_source = EndSource::Minutes;
        self.end_from_minutes();
    }

    fn end_from_minutes(&mut self) {
        let (Some(start), Some(minutes)) = (parse_time(&self.start), parse_minutes(&self.minutes)) else { return };
        // Over midnat kan ikke registreres; lad sluttiden stå, så fejlen vises.
        let (end, wrapped) = start.overflowing_add_signed(TimeDelta::minutes(minutes));
        if wrapped == 0 {
            self.end = end.format("%H:%M").to_string();
        }
    }

    fn minutes_from_end(&mut self) {
        if let (Some(start), Some(end)) = (parse_time(&self.start), parse_time(&self.end))
            && end > start
        {
            self.minutes = (end - start).num_minutes().to_string();
        }
    }

    /// Med projektet valgt på forhånd.
    pub fn with_project(mut self, project_id: Option<Id>) -> Self {
        self.project_id = project_id;
        self
    }

    /// Registreringen som felterne beskriver den, eller hvad der er galt.
    fn build(&self) -> Result<TimeEntry, String> {
        let project_id = self.project_id.ok_or_else(|| tr!("editor.err_project"))?;
        let date = parse_date(&self.date).ok_or_else(|| tr!("editor.err_date"))?;
        let original = self.original.as_ref();
        let unchanged_date = original.is_some_and(|o| o.start.date_naive() == date);

        let start = match original {
            Some(o) if unchanged_date && self.start == o.start.format("%H:%M").to_string() => o.start,
            _ => to_local(date, parse_time(&self.start).ok_or_else(|| tr!("editor.err_from"))?)?,
        };
        let end = match original {
            Some(o) if unchanged_date && self.end == o.end.format("%H:%M").to_string() => o.end,
            _ => to_local(date, parse_time(&self.end).ok_or_else(|| tr!("editor.err_to"))?)?,
        };
        if end <= start {
            return Err(tr!("editor.err_order"));
        }
        Ok(TimeEntry { project_id, start, end, note: self.note.clone(), description: self.description.trim().to_owned() })
    }

    pub fn show(&mut self, ctx: &egui::Context, data: &Data) -> EditorAction {
        let built = self.build();
        let mut action = EditorAction::None;

        let modal = egui::Modal::new(egui::Id::unique("entry_editor")).show(ctx, |ui| {
            ui.set_width(280.0);
            ui.heading(if self.index.is_some() { tr!("editor.title_edit") } else { tr!("editor.title_new") });
            ui.add_space(4.0);

            egui::Grid::new("entry_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label(tr!("editor.project"));
                self.project_combo(ui, data);
                ui.end_row();

                ui.label(tr!("editor.type"));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.note, None, tr!("editor.type_normal"));
                    ui.selectable_value(&mut self.note, Some(NOTE_PHONE.to_owned()), tr!("editor.type_phone"));
                    ui.selectable_value(&mut self.note, Some(NOTE_PAUSE.to_owned()), tr!("editor.type_pause"));
                });
                ui.end_row();

                ui.label(tr!("editor.date"));
                ui.add(egui::TextEdit::singleline(&mut self.date).hint_text(tr!("editor.date_hint")).desired_width(100.0));
                ui.end_row();

                ui.label(tr!("editor.time"));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    let mut start_changed = ui.add(time_field(&mut self.start, tr!("editor.from"))).changed();
                    start_changed |= time_menu(ui, "start_menu", &mut self.start);
                    ui.label("–");
                    let mut end_changed = ui.add(time_field(&mut self.end, tr!("editor.to"))).changed();
                    end_changed |= time_menu(ui, "end_menu", &mut self.end);
                    ui.add_space(4.0);
                    let minutes_changed = ui
                        .add(egui::TextEdit::singleline(&mut self.minutes).desired_width(28.0).margin(egui::Margin::symmetric(3, 2)))
                        .on_hover_text(tr!("editor.minutes_hint"))
                        .changed();
                    ui.label(tr!("editor.minutes"));
                    if start_changed {
                        self.start_changed();
                    }
                    if end_changed {
                        self.end_changed();
                    }
                    if minutes_changed {
                        self.minutes_changed();
                    }
                });
                ui.end_row();

                ui.label(tr!("editor.note"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.description)
                        .hint_text(tr!("editor.note_hint"))
                        .desired_width(200.0),
                );
                ui.end_row();
            });

            ui.add_space(4.0);
            match &built {
                Ok(entry) => {
                    ui.label(tr!("editor.duration", duration = format_duration(entry.duration())));
                    let overlaps = data.overlapping(entry, self.index);
                    if !overlaps.is_empty() {
                        ui.colored_label(ui.visuals().warn_fg_color, tr!("editor.overlaps"));
                        for o in overlaps {
                            ui.label(
                                RichText::new(format!(
                                    "   {}–{}  {}",
                                    o.start.format("%H:%M"),
                                    o.end.format("%H:%M"),
                                    data.project_label(o.project_id)
                                ))
                                .weak(),
                            );
                        }
                    }
                }
                Err(e) => {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
            }
            ui.add_space(8.0);

            if self.confirm_delete {
                ui.label(RichText::new(tr!("editor.confirm_delete")).strong());
                ui.horizontal(|ui| {
                    let yes = egui::Button::new(RichText::new(tr!("editor.yes_delete")).color(Color32::WHITE))
                        .fill(Color32::from_rgb(200, 60, 60));
                    if ui.add(yes).clicked()
                        && let Some(index) = self.index
                    {
                        action = EditorAction::Delete(index);
                    }
                    if ui.button(tr!("common.no")).clicked() {
                        self.confirm_delete = false;
                    }
                });
                return;
            }

            ui.horizontal(|ui| {
                if ui.add_enabled(built.is_ok(), egui::Button::new(tr!("common.save"))).on_hover_text("Enter").clicked() {
                    action = save(self.index, &built);
                }
                if ui.button(tr!("common.cancel")).on_hover_text("Esc").clicked() {
                    action = EditorAction::Cancel;
                }
                if self.index.is_some() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr!("common.delete")).clicked() {
                            self.confirm_delete = true;
                        }
                    });
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) && built.is_ok() {
                action = save(self.index, &built);
            }
        });

        if matches!(action, EditorAction::None) && modal.should_close() {
            action = EditorAction::Cancel;
        }
        action
    }

    fn project_combo(&mut self, ui: &mut egui::Ui, data: &Data) {
        let selected = self.project_id.map_or_else(|| tr!("editor.choose_project"), |id| data.project_label(id));
        // Et slettet projekt kan stadig vælges for registreringer, der allerede ligger på det.
        let current_deleted = self.project_id.filter(|id| data.project(*id).is_some_and(|p| p.deleted));
        let mut ids: Vec<Id> = data.visible_projects().map(|p| p.id).collect();
        ids.extend(current_deleted);
        ids.sort_by_key(|id| (!data.is_internal(*id), data.project_label(*id).to_lowercase()));

        egui::ComboBox::from_id_salt("entry_project")
            .width(200.0)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for id in ids {
                    ui.selectable_value(&mut self.project_id, Some(id), data.project_label(id));
                }
            });
    }
}

fn time_field(text: &mut String, hint: String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).hint_text(hint).desired_width(40.0).margin(egui::Margin::symmetric(3, 2))
}

/// En lille rulleliste ved siden af et tidsfelt med hver halve time. Den
/// åbner ved tiden i feltet. Giver `true`, når en tid er valgt.
fn time_menu(ui: &mut egui::Ui, id: &str, text: &mut String) -> bool {
    let current = parse_time(text).map(floor_to_step);
    let scrolled = ui.make_persistent_id((id, "scrolled"));
    let mut chosen = None;
    let button = ui.add(egui::Button::new("⏷").small()).on_hover_text(tr!("editor.pick_time"));
    let shown = egui::Popup::menu(&button).id(ui.make_persistent_id(id)).show(|ui| {
        let first_frame = !ui.ctx().data(|d| d.get_temp::<bool>(scrolled).unwrap_or(false));
        egui::ScrollArea::vertical().max_height(200.0).min_scrolled_height(200.0).show(ui, |ui| {
            for slot in (0..24 * 60).step_by(STEP_MINUTES as usize) {
                let time = NaiveTime::from_hms_opt(slot / 60, slot % 60, 0).unwrap_or_default();
                let selected = current == Some(time);
                let item = ui.selectable_label(selected, time.format("%H:%M").to_string());
                if selected && first_frame {
                    item.scroll_to_me(Some(egui::Align::Center));
                }
                if item.clicked() {
                    chosen = Some(time);
                }
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(scrolled, true));
    });
    if shown.is_none() {
        ui.ctx().data_mut(|d| d.remove::<bool>(scrolled));
    }
    match chosen {
        Some(time) => {
            *text = time.format("%H:%M").to_string();
            true
        }
        None => false,
    }
}

/// Rundet ned til nærmeste halve time.
fn floor_to_step(time: NaiveTime) -> NaiveTime {
    let minutes = time.num_seconds_from_midnight() / 60;
    let floored = minutes - minutes % STEP_MINUTES as u32;
    NaiveTime::from_hms_opt(floored / 60, floored % 60, 0).unwrap_or(time)
}

/// Antal minutter: "45", eller timer og minutter som "1:30".
fn parse_minutes(text: &str) -> Option<i64> {
    let text = text.trim();
    let minutes = match text.split_once([':', '.']) {
        Some((h, m)) => h.parse::<i64>().ok()? * 60 + m.parse::<i64>().ok()?,
        None => text.parse().ok()?,
    };
    (minutes > 0).then_some(minutes)
}

fn save(index: Option<usize>, built: &Result<TimeEntry, String>) -> EditorAction {
    match built {
        Ok(entry) => EditorAction::Save(index, entry.clone()),
        Err(_) => EditorAction::None,
    }
}

fn to_local(date: NaiveDate, time: NaiveTime) -> Result<DateTime<Local>, String> {
    // Ved skift til/fra sommertid kan et klokkeslæt være tvetydigt; tag det tidligste.
    Local
        .from_local_datetime(&date.and_time(time))
        .earliest()
        .ok_or_else(|| tr!("editor.err_dst"))
}

/// Accepterer "8:30", "08:30", "8.30", "0830", "830" og "8".
fn parse_time(text: &str) -> Option<NaiveTime> {
    let text = text.trim().replace(['.', ','], ":");
    if let Some((h, m)) = text.split_once(':') {
        return NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0);
    }
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (h, m) = match text.len() {
        1 | 2 => (text.as_str(), "0"),
        3 | 4 => text.split_at(text.len() - 2),
        _ => return None,
    };
    NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0)
}

/// Accepterer dd-mm-åååå, dd.mm.åååå, dd/mm/åååå og åååå-mm-dd.
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    let text = text.trim();
    ["%d-%m-%Y", "%d.%m.%Y", "%d/%m/%Y", "%Y-%m-%d"]
        .iter()
        .find_map(|f| NaiveDate::parse_from_str(text, f).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_time_formats() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0);
        assert_eq!(parse_time("8:30"), t(8, 30));
        assert_eq!(parse_time("08:30"), t(8, 30));
        assert_eq!(parse_time("8.30"), t(8, 30));
        assert_eq!(parse_time("0830"), t(8, 30));
        assert_eq!(parse_time("830"), t(8, 30));
        assert_eq!(parse_time("14"), t(14, 0));
        assert_eq!(parse_time(" 23:59 "), t(23, 59));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("8:60"), None);
        assert_eq!(parse_time("abc"), None);
        assert_eq!(parse_time(""), None);
    }

    #[test]
    fn parses_common_date_formats() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 20);
        assert_eq!(parse_date("20-10-2026"), d);
        assert_eq!(parse_date("20.10.2026"), d);
        assert_eq!(parse_date("2026-10-20"), d);
        assert_eq!(parse_date("32-10-2026"), None);
    }

    #[test]
    fn rounds_down_to_the_half_hour() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(floor_to_step(t(8, 29)), t(8, 0));
        assert_eq!(floor_to_step(t(8, 30)), t(8, 30));
        assert_eq!(floor_to_step(NaiveTime::from_hms_opt(23, 59, 59).unwrap()), t(23, 30));
        assert_eq!(parse_minutes("45"), Some(45));
        assert_eq!(parse_minutes("1:30"), Some(90));
        assert_eq!(parse_minutes("0"), None);
        assert_eq!(parse_minutes("x"), None);
    }

    #[test]
    fn end_follows_start_until_set_and_minutes_follow_end() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let mut editor = EntryEditor::new(day, None);
        editor.start = "9:00".to_owned();
        editor.start_changed();
        assert_eq!((editor.end.as_str(), editor.minutes.as_str()), ("09:30", "30"), "start + 30 min");

        editor.minutes = "45".to_owned();
        editor.minutes_changed();
        assert_eq!(editor.end, "09:45");
        editor.start = "10:00".to_owned();
        editor.start_changed();
        assert_eq!(editor.end, "10:45", "minutterne bevares, når starten flyttes");

        editor.end = "12:00".to_owned();
        editor.end_changed();
        assert_eq!(editor.minutes, "120");
        editor.start = "11:00".to_owned();
        editor.start_changed();
        assert_eq!((editor.end.as_str(), editor.minutes.as_str()), ("12:00", "60"), "en tastet sluttid står fast");

        editor.start = "23:50".to_owned();
        editor.minutes = "30".to_owned();
        editor.minutes_changed();
        assert_eq!(editor.end, "12:00", "over midnat røres sluttiden ikke");
    }

    #[test]
    fn unchanged_times_keep_their_seconds() {
        let start = Local.with_ymd_and_hms(2026, 10, 20, 8, 0, 17).unwrap();
        let end = Local.with_ymd_and_hms(2026, 10, 20, 9, 30, 45).unwrap();
        let entry = TimeEntry { project_id: 1, start, end, ..Default::default() };

        let mut editor = EntryEditor::edit(0, &entry);
        assert_eq!(editor.build().unwrap().duration(), end - start);

        editor.end = "10:00".to_owned();
        let changed = editor.build().unwrap();
        assert_eq!(changed.start, start, "fra-tiden er ikke rørt");
        assert_eq!(changed.end, Local.with_ymd_and_hms(2026, 10, 20, 10, 0, 0).unwrap());

        editor.end = "7:00".to_owned();
        assert!(editor.build().is_err(), "til før fra");
    }
}
