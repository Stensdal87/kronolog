//! Påmindelse om at holde en kort pause, når man har arbejdet længe uden.
//!
//! Det tæller som pause at holde ☕ Pause, at være væk fra computeren i mindst
//! `MIN_BREAK`, eller at have tiden stoppet i mindst `MIN_BREAK`.

use super::*;

/// Så lang tid væk/uden tid før det regnes som en pause.
const MIN_BREAK: Duration = Duration::minutes(5);
/// "Om 10 min".
const SNOOZE: Duration = Duration::minutes(10);

const TIP_COUNT: usize = 8;

/// Tippet i pausepåmindelsen; de skifter fra gang til gang.
fn tip(index: usize) -> String {
    match index % TIP_COUNT {
        0 => tr!("break.tip_legs"),
        1 => tr!("break.tip_window"),
        2 => tr!("break.tip_water"),
        3 => tr!("break.tip_shoulders"),
        4 => tr!("break.tip_walk"),
        5 => tr!("break.tip_eyes"),
        6 => tr!("break.tip_arms"),
        _ => tr!("break.tip_hands"),
    }
}

pub(super) struct BreakReminder {
    /// Hvornår den seneste pause sluttede (eller programmet startede).
    last_break: DateTime<Local>,
    /// Hvornår tiden blev stoppet, mens der ingen tid kører.
    stopped_since: Option<DateTime<Local>>,
    snoozed_until: Option<DateTime<Local>>,
    /// Påmindelsen er fremme: hvor længe der er arbejdet.
    prompt: Option<Duration>,
    tip: usize,
}

impl BreakReminder {
    pub(super) fn new() -> Self {
        let now = Local::now();
        Self { last_break: now, stopped_since: None, snoozed_until: None, prompt: None, tip: 0 }
    }

    pub(super) fn is_open(&self) -> bool {
        self.prompt.is_some()
    }

    /// Har brugeren været væk fra computeren længe nok, tæller det som en pause.
    pub(super) fn note_away(&mut self, since: DateTime<Local>, at: DateTime<Local>) {
        if at - since >= MIN_BREAK {
            self.last_break = self.last_break.max(at);
        }
    }

    fn took_break(&mut self, now: DateTime<Local>) {
        self.last_break = now;
        self.snoozed_until = None;
        self.prompt = None;
    }

    /// Opdaterer tilstanden og åbner påmindelsen, når det er tid.
    /// Returnerer true, når påmindelsen lige er blevet åbnet.
    fn check(&mut self, now: DateTime<Local>, activity: Activity, enabled: bool, minutes: u32) -> bool {
        if activity == Activity::Stopped {
            self.stopped_since.get_or_insert(now);
            return false;
        }
        if let Some(stopped) = self.stopped_since.take()
            && now - stopped >= MIN_BREAK
        {
            self.took_break(now);
        }
        match activity {
            // Mens der holdes pause, flytter pausens slutning sig med.
            Activity::Pause => {
                self.took_break(now);
                return false;
            }
            // Ingen påmindelse midt i et telefonopkald.
            Activity::Call => return false,
            Activity::Working | Activity::Stopped => {}
        }
        let worked = now - self.last_break;
        let due = worked >= Duration::minutes(i64::from(minutes));
        let snoozed = self.snoozed_until.is_some_and(|until| now < until);
        if !enabled || !due || snoozed || self.prompt.is_some() {
            return false;
        }
        self.prompt = Some(worked);
        self.tip = (self.tip + 1) % TIP_COUNT;
        true
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Activity {
    Stopped,
    Pause,
    Call,
    Working,
}

impl KronoLogApp {
    /// Holder øje med arbejdstiden og viser påmindelsen, når den er nået.
    pub(super) fn update_break_reminder(&mut self, ctx: &egui::Context) {
        let activity = match &self.data.active {
            None => Activity::Stopped,
            Some(a) if a.is_note(NOTE_PAUSE) => Activity::Pause,
            Some(a) if a.is_call() => Activity::Call,
            Some(_) => Activity::Working,
        };
        let settings = &self.data.settings;
        if self.break_reminder.check(Local::now(), activity, settings.break_reminder, settings.break_minutes) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
        }
    }

    /// "Tid til en lille pause?" Enter holder pause, Esc minder igen om 10 min.
    pub(super) fn break_dialog(&mut self, ctx: &egui::Context) {
        let Some(worked) = self.break_reminder.prompt else { return };
        let worked_text = format_length(worked);
        let tip = tip(self.break_reminder.tip);
        enum Answer {
            Pause,
            PauseBreathing,
            Snooze,
            Skip,
        }
        let mut answer = None;

        let modal = egui::Modal::new(egui::Id::unique("break_dialog")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 40.0).clamp(240.0, 300.0));
            ui.heading(tr!("break.title"));
            ui.add_space(4.0);
            ui.label(tr!("break.worked", length = worked_text));
            ui.add_space(4.0);
            ui.label(RichText::new(tr!("break.tip", tip = tip)).italics().weak());
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr!("break.take_break")).on_hover_text("Enter").clicked() {
                    answer = Some(Answer::Pause);
                }
                if ui.button(tr!("break.with_exercise")).on_hover_text(tr!("break.with_exercise_hint")).clicked() {
                    answer = Some(Answer::PauseBreathing);
                }
                if ui.button(tr!("break.snooze")).on_hover_text("Esc").clicked() {
                    answer = Some(Answer::Snooze);
                }
                if ui.button(tr!("break.skip")).on_hover_text(tr!("break.skip_hint")).clicked() {
                    answer = Some(Answer::Skip);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                answer = Some(Answer::Pause);
            }
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(Answer::Snooze);
        }

        let now = Local::now();
        match answer {
            None => {}
            Some(answer @ (Answer::Pause | Answer::PauseBreathing)) => {
                self.break_reminder.prompt = None;
                if !self.data.active.as_ref().is_some_and(|a| a.is_note(NOTE_PAUSE)) {
                    self.data.toggle_pause();
                    self.save();
                }
                if matches!(answer, Answer::PauseBreathing) {
                    self.start_breathing();
                }
            }
            Some(Answer::Snooze) => {
                self.break_reminder.prompt = None;
                self.break_reminder.snoozed_until = Some(now + SNOOZE);
            }
            Some(Answer::Skip) => self.break_reminder.took_break(now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(minutes: i64) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 26, 8, 0, 0).unwrap() + Duration::minutes(minutes)
    }

    fn reminder() -> BreakReminder {
        BreakReminder { last_break: at(0), stopped_since: None, snoozed_until: None, prompt: None, tip: 0 }
    }

    #[test]
    fn reminds_after_interval_of_work() {
        let mut r = reminder();
        assert!(!r.check(at(59), Activity::Working, true, 60));
        assert!(r.check(at(60), Activity::Working, true, 60));
        assert!(!r.check(at(61), Activity::Working, true, 60), "kun én gang");
        assert_eq!(r.prompt, Some(Duration::minutes(60)));
    }

    #[test]
    fn disabled_call_and_pause_do_not_remind() {
        let mut r = reminder();
        assert!(!r.check(at(90), Activity::Working, false, 60), "slået fra");
        assert!(!r.check(at(90), Activity::Call, true, 60), "ikke under et opkald");
        assert!(!r.check(at(95), Activity::Pause, true, 60));
        assert!(!r.check(at(100), Activity::Working, true, 60), "pausen nulstillede tællingen");
        assert!(r.check(at(155), Activity::Working, true, 60));
    }

    #[test]
    fn stopped_for_five_minutes_counts_as_break() {
        let mut r = reminder();
        r.check(at(50), Activity::Stopped, true, 60);
        r.check(at(52), Activity::Working, true, 60);
        assert!(r.check(at(60), Activity::Working, true, 60), "2 min stop er ikke en pause");

        let mut r = reminder();
        r.check(at(50), Activity::Stopped, true, 60);
        r.check(at(56), Activity::Working, true, 60);
        assert!(!r.check(at(100), Activity::Working, true, 60), "6 min stop er en pause");
        assert!(r.check(at(116), Activity::Working, true, 60));
    }

    #[test]
    fn away_and_snooze() {
        let mut r = reminder();
        r.note_away(at(30), at(33));
        assert!(r.check(at(60), Activity::Working, true, 60), "3 min væk er ikke en pause");
        r.prompt = None;
        r.snoozed_until = Some(at(70));
        assert!(!r.check(at(65), Activity::Working, true, 60), "udsat");
        assert!(r.check(at(70), Activity::Working, true, 60), "igen efter udsættelsen");

        let mut r = reminder();
        r.note_away(at(30), at(40));
        assert!(!r.check(at(60), Activity::Working, true, 60), "10 min væk er en pause");
        assert!(r.check(at(100), Activity::Working, true, 60));
    }
}
