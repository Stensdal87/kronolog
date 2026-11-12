//! "Hvad er nyt?": ændringerne i hver version, fra CHANGELOG.md (dansk) og
//! CHANGELOG.en.md (engelsk), der bygges ind i programmet. Åbnes fra
//! indstillingerne og af sig selv første gang efter en opdatering.

use super::*;

const DANISH: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/CHANGELOG.md"));
const ENGLISH: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/CHANGELOG.en.md"));

/// Én version: overskriften (fx "1.1.0 (2026-11-12)") og punkterne.
#[derive(Debug, PartialEq)]
struct Release<'a> {
    heading: &'a str,
    items: Vec<&'a str>,
}

/// "1.2.0" → (1, 2, 0). Ukendte dele tæller som 0.
fn version_key(version: &str) -> (u32, u32, u32) {
    let mut parts = version.split('.').map(|p| p.trim().parse().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

/// Versionerne i filen, nyeste først. Versioner nyere end `current` springes
/// over, så næste versions punkter kan skrives, før den er udgivet.
fn releases<'a>(text: &'a str, current: &str) -> Vec<Release<'a>> {
    let current = version_key(current);
    let mut releases: Vec<Release> = Vec::new();
    let mut skipping = false;
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            let heading = heading.trim();
            let version = heading.split_whitespace().next().unwrap_or("");
            skipping = version_key(version) > current;
            if !skipping {
                releases.push(Release { heading, items: Vec::new() });
            }
        } else if let Some(item) = line.trim_start().strip_prefix("- ")
            && !skipping
            && let Some(release) = releases.last_mut()
        {
            release.items.push(item.trim());
        }
    }
    releases
}

pub(super) struct Changelog {
    position: Option<Option<Pos2>>,
}

impl KronoLogApp {
    pub(super) fn open_changelog(&mut self) {
        if self.changelog.is_none() {
            self.changelog = Some(Changelog { position: None });
        }
    }

    /// Første start efter en opdatering: vis hvad der er nyt. Ved allerførste
    /// start (hjælpen åbner) huskes versionen bare.
    pub(super) fn show_changelog_after_update(&mut self) {
        let current = env!("CARGO_PKG_VERSION");
        if self.data.last_version.as_deref() == Some(current) {
            return;
        }
        if self.data.help_seen {
            self.open_changelog();
        }
        self.data.last_version = Some(current.to_owned());
        self.save();
    }

    pub(super) fn changelog_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let Some(changelog) = &mut self.changelog else { return };
        let size = Vec2::new(420.0, 520.0);
        let position = *changelog.position.get_or_insert_with(|| item_history::centered_on_screen(frame, size));
        let level = if self.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal };
        let mut builder = egui::ViewportBuilder::default()
            .with_title(tr!("changelog.window_title"))
            .with_inner_size(size)
            .with_min_inner_size([300.0, 250.0])
            .with_window_level(level);
        if let Some(position) = position {
            builder = builder.with_position(position);
        }
        let text = if rust_i18n::locale().starts_with("da") { DANISH } else { ENGLISH };
        let mut close = false;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("changelog"), builder, |ui, _class| {
            close |= ui.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape));
            egui::Panel::bottom("changelog_buttons").show(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("KronoLog {}", env!("CARGO_PKG_VERSION"))).weak().small());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(tr!("changelog.close")).clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(2.0);
            });
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    for (index, release) in releases(text, env!("CARGO_PKG_VERSION")).into_iter().enumerate() {
                        // Den nyeste version står foldet ud.
                        egui::CollapsingHeader::new(RichText::new(tr!("changelog.version", version = release.heading)).strong())
                            .id_salt(release.heading)
                            .default_open(index == 0)
                            .show(ui, |ui| {
                                for item in release.items {
                                    ui.horizontal_top(|ui| {
                                        ui.label("•");
                                        ui.add(egui::Label::new(item).wrap());
                                    });
                                    ui.add_space(2.0);
                                }
                            });
                    }
                });
            });
        });
        if close {
            self.changelog = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_files_list_the_same_versions_with_the_same_number_of_items() {
        let danish = releases(DANISH, "99.0.0");
        let english = releases(ENGLISH, "99.0.0");
        assert!(danish.len() >= 2);
        let summary = |r: &[Release]| -> Vec<(String, usize)> {
            r.iter().map(|r| (r.heading.split_whitespace().next().unwrap().to_owned(), r.items.len())).collect()
        };
        assert_eq!(summary(&danish), summary(&english));
        assert!(danish.iter().all(|r| !r.items.is_empty()));
    }

    #[test]
    fn the_current_version_is_in_the_changelog() {
        let current = env!("CARGO_PKG_VERSION");
        let listed = releases(DANISH, current);
        assert_eq!(listed[0].heading.split_whitespace().next(), Some(current), "skriv ændringerne i CHANGELOG.md");
    }

    #[test]
    fn newer_versions_are_hidden_until_released() {
        let text = "# x\n\n## 1.2.0\n\n- ny\n\n## 1.1.0 (2026-11-12)\n\n- a\n- b\n";
        let listed = releases(text, "1.1.0");
        assert_eq!(listed, vec![Release { heading: "1.1.0 (2026-11-12)", items: vec!["a", "b"] }]);
        assert_eq!(releases(text, "1.10.0").len(), 2, "1.10 er nyere end 1.2");
    }
}
