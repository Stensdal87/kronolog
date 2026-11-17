//! "Glemt at starte tiden?": har man været ved computeren et stykke tid uden at
//! tage tid, minder programmet om det og kan starte tiden fra dengang.
//!
//! "Ved computeren" følger `IdleWatcher`: en "væk"-periode afbryder, og
//! tællingen starter forfra, når man kommer tilbage.

use super::*;

/// Hvor længe der skal gå efter "Om en time".
const SNOOZE: Duration = Duration::hours(1);

impl KronoLogApp {
    /// Kaldes i `logic`. Starter påmindelsen, når man har været til stede uden
    /// kørende tid i `settings.forgot_minutes`.
    pub(super) fn handle_forgot_timer(&mut self, ctx: &egui::Context) {
        let now = Local::now();
        // Et langt hul siden sidst kan være dvale. Dvalen meldes af `IdleWatcher`
        // inden for 5 s og nulstiller så tællingen (`woke_up`); indtil da vises
        // påmindelsen ikke, så den ikke når at foreslå tiden fra før dvalen.
        if self.forgot_checked.replace(now).is_some_and(|last| now - last > Duration::seconds(60)) {
            self.forgot_hold_until = Some(now + Duration::seconds(10));
        }
        let settings = &self.data.settings;
        if !settings.forgot_reminder || self.data.active.is_some() || self.away_since.is_some() {
            // Tiden kører, eller man er væk: tæl forfra næste gang.
            self.present_since = None;
            if self.data.active.is_some() {
                self.forgot_prompt = None;
            }
            return;
        }
        let since = *self.present_since.get_or_insert(now);
        let due = since + Duration::minutes(settings.forgot_minutes as i64);
        if self.forgot_prompt.is_some() || self.forgot_snoozed_until.is_some_and(|until| now < until) {
            return;
        }
        if let Some(hold) = self.forgot_hold_until {
            if now < hold {
                ctx.request_repaint_after((hold - now).to_std().unwrap_or_default());
                return;
            }
            self.forgot_hold_until = None;
        }
        if now >= due {
            self.forgot_prompt = Some(since);
        } else {
            // Vågn op, når tiden er gået, også hvis intet andet sker.
            ctx.request_repaint_after((due - now).to_std().unwrap_or_default() + StdDuration::from_millis(100));
        }
    }

    /// Computeren er vågnet ved `at`: man har tidligst været ved den siden da.
    /// En påmindelse, der allerede er vist med tiden fra før dvalen, lukkes.
    pub(super) fn woke_up(&mut self, at: DateTime<Local>) {
        if self.present_since.is_some_and(|since| since < at) {
            self.present_since = Some(at);
        }
        if self.forgot_prompt.is_some_and(|since| since < at) {
            self.forgot_prompt = None;
        }
    }

    pub(super) fn forgot_dialog(&mut self, ctx: &egui::Context) {
        let Some(since) = self.forgot_prompt else { return };
        // Tiden kan ikke starte før den seneste registrering sluttede.
        let from = self.data.start_from_time(since);
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::unique("forgot_dialog")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 40.0).clamp(240.0, 300.0));
            ui.heading(tr!("forgot.title"));
            ui.add_space(4.0);
            ui.label(tr!("forgot.text", time = from.format("%H:%M")));
            ui.add_space(8.0);
            if ui.button(tr!("forgot.choose", time = from.format("%H:%M"))).on_hover_text("Enter").clicked() {
                choice = Some(Choice::Start);
            }
            ui.horizontal(|ui| {
                if ui.button(tr!("forgot.snooze")).on_hover_text("Esc").clicked() {
                    choice = Some(Choice::Snooze);
                }
                if ui.button(tr!("forgot.not_today")).clicked() {
                    choice = Some(Choice::NotToday);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                choice = Some(Choice::Start);
            }
        });
        if choice.is_none() && modal.should_close() {
            choice = Some(Choice::Snooze);
        }
        let Some(choice) = choice else { return };
        self.forgot_prompt = None;
        let now = Local::now();
        match choice {
            Choice::Start => self.picker = Some(ProjectPicker::starting_from(from)),
            Choice::Snooze => self.forgot_snoozed_until = Some(now + SNOOZE),
            Choice::NotToday => {
                let tomorrow = now.date_naive() + Duration::days(1);
                self.forgot_snoozed_until =
                    tomorrow.and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest());
            }
        }
    }
}

enum Choice {
    Start,
    Snooze,
    NotToday,
}
