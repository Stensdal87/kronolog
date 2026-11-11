//! Hjælpen (F1): et lille vindue med de vigtigste funktioner, foldet ud i
//! emner, med søgning. Plus "Vidste du…?"-tips nederst i hovedvinduet, der
//! peger ind i hjælpen. Teksterne ligger i locales/app.yml (`help.*`, `tips.*`),
//! så hjælpen følger sproget.

use chrono::Datelike;

use super::*;

/// Et emne i hjælpen. Titlen er `help.<key>`, teksten `help.<key>_body`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Topic {
    Start,
    Switch,
    Pause,
    History,
    Billing,
    Lists,
    Export,
    Bar,
    Shortcuts,
    Away,
    Breathing,
    Tray,
}

impl Topic {
    const ALL: [Topic; 12] = [
        Topic::Start,
        Topic::Switch,
        Topic::Pause,
        Topic::History,
        Topic::Billing,
        Topic::Lists,
        Topic::Export,
        Topic::Bar,
        Topic::Shortcuts,
        Topic::Away,
        Topic::Breathing,
        Topic::Tray,
    ];

    fn key(self) -> &'static str {
        match self {
            Topic::Start => "start",
            Topic::Switch => "switch",
            Topic::Pause => "pause",
            Topic::History => "history",
            Topic::Billing => "billing",
            Topic::Lists => "lists",
            Topic::Export => "export",
            Topic::Bar => "bar",
            Topic::Shortcuts => "shortcuts",
            Topic::Away => "away",
            Topic::Breathing => "breathing",
            Topic::Tray => "tray",
        }
    }

    fn title(self) -> String {
        let key = format!("help.{}", self.key());
        tr!(&key)
    }

    fn body(self) -> String {
        let key = format!("help.{}_body", self.key());
        tr!(&key)
    }

    /// Matcher emnet søgningen (titel eller tekst, uden hensyn til store/små bogstaver)?
    fn matches(self, search: &str) -> bool {
        let search = search.trim().to_lowercase();
        search.is_empty() || self.title().to_lowercase().contains(&search) || self.body().to_lowercase().contains(&search)
    }
}

/// "Vidste du…?": tekst (`tips.<key>`) og emnet, der forklarer mere.
const TIPS: [(&str, Topic); 15] = [
    ("week", Topic::History),
    ("favorites", Topic::Lists),
    ("goal", Topic::Start),
    ("share", Topic::Lists),
    ("split", Topic::History),
    ("move", Topic::Switch),
    ("item", Topic::Lists),
    ("round", Topic::Billing),
    ("billed", Topic::Billing),
    ("bar", Topic::Bar),
    ("autocall", Topic::Pause),
    ("keys", Topic::Shortcuts),
    ("note", Topic::Start),
    ("template", Topic::Lists),
    ("f1", Topic::Start),
];

/// Dagens tip; `offset` bladrer videre med "Næste tip".
fn tip_for(day: NaiveDate, offset: usize) -> (&'static str, Topic) {
    TIPS[(day.num_days_from_ce().unsigned_abs() as usize + offset) % TIPS.len()]
}

pub(super) struct Help {
    search: String,
    /// Emnet der skal stå foldet ud (og rulles frem), når vinduet åbner.
    open: Option<Topic>,
    /// Placeringen ved åbning (centreret på skærmen), så vinduet kan flyttes.
    position: Option<Option<Pos2>>,
    focused: bool,
    /// "Udfold alle" / "Sammenklap alle": gælder i ét billede, derefter bestemmer man selv.
    all_open: Option<bool>,
}

impl Help {
    pub(super) fn new(open: Option<Topic>) -> Self {
        Self { search: String::new(), open, position: None, focused: false, all_open: None }
    }
}

impl TimeSnapperApp {
    /// Åbner hjælpen, evt. ved et bestemt emne. Åbner den allerede, rulles der til emnet.
    pub(super) fn open_help(&mut self, topic: Option<Topic>) {
        match &mut self.help {
            Some(help) => {
                help.open = topic;
                help.search.clear();
            }
            None => self.help = Some(Help::new(topic)),
        }
    }

    /// Første gang programmet starter, åbner hjælpen ved "Kom i gang".
    pub(super) fn show_help_first_time(&mut self) {
        if !self.data.help_seen {
            self.data.help_seen = true;
            self.open_help(Some(Topic::Start));
            self.save();
        }
    }

    pub(super) fn help_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let Some(help) = &mut self.help else { return };
        let size = Vec2::new(390.0, 560.0);
        let position = *help.position.get_or_insert_with(|| item_history::centered_on_screen(frame, size));
        let level = if self.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
        let mut builder = egui::ViewportBuilder::default()
            .with_title(tr!("help.window_title"))
            .with_inner_size(size)
            .with_min_inner_size([300.0, 300.0])
            .with_window_level(level);
        if let Some(position) = position {
            builder = builder.with_position(position);
        }
        let mut close = false;
        let mut open_shortcuts = false;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("help"), builder, |ui, _class| {
            close |= ui.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape));
            egui::CentralPanel::default().show(ui, |ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut help.search)
                        .hint_text(tr!("help.search_hint"))
                        .desired_width(f32::INFINITY),
                );
                if !help.focused {
                    search.request_focus();
                    help.focused = true;
                }
                // Som ved kundelisten.
                ui.horizontal(|ui| {
                    if ui.small_button(tr!("list.expand_all")).clicked() {
                        help.all_open = Some(true);
                    }
                    if ui.small_button(tr!("list.collapse_all")).clicked() {
                        help.all_open = Some(false);
                    }
                });
                ui.add_space(4.0);
                let searching = !help.search.trim().is_empty();
                let all_open = help.all_open.take();
                let topics: Vec<Topic> = Topic::ALL.into_iter().filter(|t| t.matches(&help.search)).collect();
                // Det ønskede emne foldes ud én gang; bagefter bestemmer man selv.
                let open = help.open.take();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    if topics.is_empty() {
                        ui.label(RichText::new(tr!("help.no_match")).weak());
                    }
                    for topic in topics {
                        let force = if all_open.is_some() {
                            all_open
                        } else if searching {
                            Some(true)
                        } else if open.is_some() {
                            Some(open == Some(topic))
                        } else {
                            None
                        };
                        let response = egui::CollapsingHeader::new(RichText::new(topic.title()).strong())
                            .id_salt(topic.key())
                            .open(force)
                            .show(ui, |ui| {
                                for line in topic.body().lines() {
                                    ui.label(line);
                                    ui.add_space(2.0);
                                }
                                if topic == Topic::Shortcuts && ui.button(tr!("help.open_shortcuts")).clicked() {
                                    open_shortcuts = true;
                                }
                            });
                        if open == Some(topic) {
                            response.header_response.scroll_to_me(Some(egui::Align::Min));
                        }
                    }
                });
            });
        });
        if open_shortcuts {
            self.shortcuts_dialog = Some(ShortcutsDialog::default());
            self.show_window(ctx);
        }
        if close {
            self.help = None;
        }
    }

    /// "Vidste du…?" nederst i timer-fanen. Klik åbner hjælpen ved emnet.
    pub(super) fn tip_line(&mut self, ui: &mut egui::Ui) {
        let today = Local::now().date_naive();
        if !self.data.settings.show_tips || self.data.tips_hidden_on == Some(today) {
            return;
        }
        let (key, topic) = tip_for(today, self.tip_offset);
        let key = format!("tips.{key}");
        egui::Panel::bottom("tip").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text(tr!("tips.hide")).clicked() {
                        self.data.tips_hidden_on = Some(today);
                        self.save();
                    }
                    if ui.small_button("›").on_hover_text(tr!("tips.next")).clicked() {
                        self.tip_offset += 1;
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let text = format!("💡 {} {}", tr!("tips.did_you_know"), tr!(&key));
                        let label = egui::Label::new(RichText::new(text).small().weak()).wrap().sense(egui::Sense::click());
                        if ui.add(label).on_hover_text(tr!("tips.read_more")).clicked() {
                            self.open_help(Some(topic));
                        }
                    });
                });
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_and_tip_has_text_in_both_languages() {
        for locale in ["da", "en"] {
            for topic in Topic::ALL {
                for key in [format!("help.{}", topic.key()), format!("help.{}_body", topic.key())] {
                    let text = rust_i18n::t!(&key, locale = locale).to_string();
                    assert_ne!(text, key, "{key} mangler på {locale}");
                }
            }
            for (tip, _) in TIPS {
                let key = format!("tips.{tip}");
                assert_ne!(rust_i18n::t!(&key, locale = locale).to_string(), key, "{key} mangler på {locale}");
            }
        }
    }

    #[test]
    fn search_finds_topics_by_title_or_text() {
        rust_i18n::set_locale("da");
        assert!(Topic::History.matches("timeregnskab"), "ugeoversigten står under historik");
        assert!(Topic::Tray.matches("sikkerhedskopi"));
        assert!(Topic::History.matches("del op"), "✂ Del op står under historik");
        assert!(Topic::Switch.matches("SKYDEREN"), "store bogstaver er lige meget");
        assert!(!Topic::Breathing.matches("faktura"));
        assert!(Topic::ALL.iter().all(|t| t.matches("  ")), "tom søgning viser alt");
    }

    #[test]
    fn tip_changes_each_day_and_with_next() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        assert_ne!(tip_for(day, 0), tip_for(day + Duration::days(1), 0));
        assert_eq!(tip_for(day, 1), tip_for(day + Duration::days(1), 0));
        assert_eq!(tip_for(day, TIPS.len()), tip_for(day, 0), "går rundt");
    }
}
