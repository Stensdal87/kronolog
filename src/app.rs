//! Brugerfladen.

mod break_reminder;
mod changelog;
pub(crate) mod breathing;
mod compact;
mod forgot;
mod help;
mod item_history;
mod shortcuts_dialog;
mod week;

use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;
use std::path::PathBuf;
use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};
use eframe::egui::{self, Color32, Pos2, RichText, Stroke, Vec2};

use crate::entry_editor::EntryEditor;
use crate::export_dialog::ExportDialog;
use crate::idle::{IdleEvent, IdleWatcher};
use crate::tray::{Tray, TrayAction, TrayProject, TrayState};
use break_reminder::BreakReminder;
use breathing::{Breathing, Exercise};
use compact::{NORMAL_MIN_WIDTH, PendingMove, ProjectPicker, WindowLayout};
use item_history::ItemHistory;
use shortcuts_dialog::ShortcutsDialog;
use crate::hotkeys::GlobalHotkeys;
use crate::shortcuts::Action;
use crate::data::{NOTE_PHONE, BilledState, Scope, format_length, note_label, ActiveTimer, SortOrder, TemplateProject, format_duration, DayEntry, format_hours_minutes, round_down_quarter, round_up_quarter, CallTarget, Customer, Data, Id, Project, ThemeChoice, NOTE_PAUSE};

/// Hvor ofte en kørende timers "sidst set" gemmes.
const HEARTBEAT: StdDuration = StdDuration::from_secs(30);

const ACCENT: Color32 = Color32::from_rgb(70, 160, 230);
/// Dagsmålets ring og tekst.
const GOAL_COLOR: Color32 = Color32::from_rgb(80, 170, 100);

/// Om telefonen blev taget automatisk (#27).
#[derive(Clone, Copy, PartialEq)]
enum AutoCall {
    Idle,
    /// Vi startede opkaldet og afslutter det, når mikrofonen er fri.
    Started,
    /// Man var allerede i et opkald; det rører vi ikke.
    Manual,
}

#[derive(PartialEq)]
enum Tab {
    Timer,
    History,
}

/// En kunde eller et projekt, f.eks. til sletning eller omdøbning.
#[derive(Clone, Copy)]
enum Item {
    Customer(Id),
    Project(Id),
}

/// En igangværende omdøbning af en kunde eller et projekt.
struct Rename {
    item: Item,
    name: String,
    error: Option<String>,
    focused: bool,
}

#[derive(PartialEq)]
enum HistoryView {
    /// Én linje pr. projekt.
    Summary,
    /// Hver registrering for sig.
    All,
    /// Ugen som timeregnskab: projekter × ugedage.
    Week,
}

pub struct KronoLogApp {
    data: Data,
    /// Beskeder fra en ny kopi af programmet, der blev startet (den lukker selv).
    instance: Option<crate::single_instance::Instance>,
    path: PathBuf,
    error: Option<String>,
    tab: Tab,
    always_on_top: bool,
    new_customer: String,
    /// Tekstfelter til "nyt projekt" pr. kunde.
    new_project: HashMap<Id, String>,
    history_date: NaiveDate,
    history_view: HistoryView,
    /// Projekter i Historik → Summeret, der er foldet ud.
    history_open: HashSet<Id>,
    last_heartbeat: Instant,
    started: Instant,
    window_level_applied: bool,
    /// Om det er tjekket, at vinduet endte på en skærm efter opstart.
    position_verified: bool,
    call_dialog_open: bool,
    /// Antal ikke-registrerede opkald sidst vi kiggede. Kommer der flere, åbnes dialogen.
    seen_unassigned: usize,
    /// Søgefelter i projektlisten og i opkaldsdialogen.
    search: String,
    dialog_search: String,
    /// "Udfold alle"/"Sammenklap alle" anvendes på kunderne i næste frame.
    set_all_open: Option<bool>,
    /// Kunde eller projekt der venter på bekræftelse af sletning.
    confirm_delete: Option<Item>,
    rename: Option<Rename>,
    /// Åben dialog til at rette/tilføje en registrering.
    editor: Option<EntryEditor>,
    export: Option<ExportDialog>,
    /// Indstillingsmenuen i venstre side.
    settings_open: bool,
    idle: IdleWatcher,
    /// Hvornår brugeren gik fra computeren, mens vi venter på at de kommer tilbage.
    away_since: Option<DateTime<Local>>,
    /// Væk-perioder før dette tidspunkt er allerede besvaret.
    away_handled_until: DateTime<Local>,
    /// Spørgsmålet "skal tiden være pause?", når brugeren er kommet tilbage.
    away_prompt: Option<AwayPrompt>,
    /// Ikonet i systembakken; `None` hvis systemet ikke har en.
    tray: Option<Tray>,
    window_visible: bool,
    /// Sat når programmet afsluttes fra systembakken, så lukning ikke skjuler vinduet.
    quitting: bool,
    /// Om programmet starter, når man logger på. Læses fra systemet ved opstart.
    autostart: bool,
    /// Den visning vinduets størrelse sidst blev sat til.
    layout: WindowLayout,
    layout_applied: bool,
    picker: Option<ProjectPicker>,
    pending_move: Option<PendingMove>,
    /// Hvor meget en flytning skal justeres, fordi vinduesmanageren lægger
    /// titellinjen oveni (KWin/X11: ca. 28 px). Læres ved opstart og ved første
    /// rettede flytning, så senere flytninger lander rigtigt med det samme.
    /// Én for vinduet uden titellinje (indeks 0) og én med (indeks 1): KWin
    /// lægger kun titellinjen oveni, når der er en (#31).
    move_correction: [Vec2; 2],
    /// Om bjælken sidst voksede opad (bunden fast). Samme kant bruges, når den
    /// folder sig sammen igen, så den vender tilbage til samme sted.
    bar_anchor_bottom: Option<bool>,
    /// Som `bar_anchor_bottom`, men højre kant, når bjælken står lodret.
    bar_anchor_right: Option<bool>,
    /// Bjælkens retning, da størrelsen sidst blev sat.
    applied_vertical: bool,
    /// Vinduet til at lave en ny åndedrætsøvelse (#30).
    new_exercise: Option<breathing::NewExercise>,
    /// Hjælpevinduet (F1).
    help: Option<help::Help>,
    /// "Hvad er nyt?"-vinduet.
    changelog: Option<changelog::Changelog>,
    /// Ved computeren uden kørende tid siden da ("Glemt at starte tiden?").
    present_since: Option<DateTime<Local>>,
    /// Påmindelsen vises; værdien er, hvornår man kom.
    forgot_prompt: Option<DateTime<Local>>,
    /// Ingen påmindelse før da ("Om en time" / "Ikke i dag").
    forgot_snoozed_until: Option<DateTime<Local>>,
    /// "Timebudget…": projektet og budgettet i timer, mens det rettes.
    budget_dialog: Option<(Id, f32)>,
    /// Besked efter eksport/import af kunde- og projektlisten.
    list_message: Option<String>,
    /// Dagen, hvor den daglige sikkerhedskopi er lavet.
    backed_up_on: Option<NaiveDate>,
    /// "Gendan sikkerhedskopi?", når en kopi er valgt.
    restore_prompt: Option<crate::backup::Backup>,
    /// "Næste tip" er trykket så mange gange i dag.
    tip_offset: usize,
    /// Rammen (titellinje og kanter), sidst vinduet havde titellinje. Bruges til
    /// at placere vinduet, når titellinjen kommer tilbage (#31).
    decorated_frame: compact::FrameInsets,
    /// Et træk i bjælken er givet videre til vinduesmanageren.
    bar_drag_sent: bool,
    /// Den lodrette bjælke står i venstre side af skærmen, så teksten læses
    /// nedefra og op (#20).
    bar_on_left: bool,
    /// Om vinduesikonet (proceslinjen) har den røde prik (#21).
    window_icon_dot: bool,
    /// Hvornår vinduet sidst skiftede visning; se `apply_layout` (#25).
    layout_changed_at: Instant,
    /// Søgning efter og installation af nye versioner (#26).
    updater: crate::updater::Updater,
    /// Hvilke programmer der bruger mikrofonen (#27).
    mic: crate::mic::MicWatcher,
    /// Et opkaldsprogram har brugt mikrofonen siden da.
    mic_busy_since: Option<Instant>,
    /// Mikrofonen har været fri siden da.
    mic_free_since: Option<Instant>,
    auto_call: AutoCall,
    /// Log over mikrofonen og automatisk telefon (#33).
    mic_log: crate::mic::MicLog,
    /// Hvad loggen sidst skrev om mikrofonen og indstillingen.
    logged_mic_apps: Option<Vec<String>>,
    logged_auto_call: Option<bool>,
    /// Opkaldsvinduerne, som loggen sidst skrev dem.
    logged_call_windows: Vec<crate::mic::CallWindow>,
    /// Navnet fra opkaldsvinduet er forsøgt skrevet i noten for dette opkald.
    call_named: bool,
    /// Sidst `handle_microphone` kørte; et langt hul skrives i loggen.
    last_mic_check: Instant,
    /// Globale genvejstaster (kun Windows indtil videre).
    hotkeys: Option<GlobalHotkeys>,
    shortcuts_dialog: Option<ShortcutsDialog>,
    /// Sæt fokus i søgefeltet i næste frame (Ctrl+F).
    focus_search: bool,
    break_reminder: BreakReminder,
    /// Åndedrætsøvelsen, mens den er i gang (eget vindue).
    breathing: Option<Breathing>,
    /// Vinduet med registreringer for én kunde eller ét projekt.
    item_history: Option<ItemHistory>,
    /// Kørte der en pause sidst? Bruges til at starte/stoppe øvelsen med pausen.
    was_on_pause: bool,
    /// "Computeren har sovet – fortsæt?" efter dvale (issue #6).
    sleep_prompt: Option<SleepPrompt>,
    /// Tekstfeltet til et nyt projekt i kundeskabelonen.
    new_template_project: String,
}

/// Tidstagningen der blev stoppet, da computeren gik i dvale.
struct SleepPrompt {
    snapshot: ActiveTimer,
    from: DateTime<Local>,
    to: DateTime<Local>,
}

/// En periode hvor brugeren var væk, mens der kørte tid på et projekt.
struct AwayPrompt {
    project_id: Id,
    from: DateTime<Local>,
    to: DateTime<Local>,
}

impl KronoLogApp {
    pub fn new(ctx: &egui::Context, path: PathBuf) -> Self {
        let (data, error) = match Data::load(&path) {
            Ok(d) => (d, None),
            // Gem aldrig oven i en fil vi ikke kunne læse.
            Err(e) => (Data::default(), Some(e)),
        };
        let mic_log = crate::mic::MicLog::new(&path);
        let mut app = Self {
            data,
            instance: None,
            path,
            error,
            tab: Tab::Timer,
            always_on_top: true,
            new_customer: String::new(),
            new_project: HashMap::new(),
            history_date: Local::now().date_naive(),
            history_view: HistoryView::Summary,
            history_open: HashSet::new(),
            last_heartbeat: Instant::now(),
            started: Instant::now(),
            window_level_applied: false,
            position_verified: false,
            call_dialog_open: false,
            seen_unassigned: 0,
            search: String::new(),
            dialog_search: String::new(),
            set_all_open: None,
            confirm_delete: None,
            rename: None,
            editor: None,
            export: None,
            settings_open: false,
            idle: IdleWatcher::start(ctx),
            away_since: None,
            away_handled_until: Local::now(),
            away_prompt: None,
            tray: Tray::start(ctx),
            window_visible: true,
            quitting: false,
            autostart: {
                crate::autostart::migrate_old_shortcut();
                crate::autostart::is_enabled()
            },
            layout: WindowLayout::Normal,
            layout_applied: false,
            picker: None,
            pending_move: None,
            move_correction: [Vec2::ZERO; 2],
            bar_anchor_bottom: None,
            bar_anchor_right: None,
            applied_vertical: false,
            decorated_frame: Default::default(),
            new_exercise: None,
            help: None,
            changelog: None,
            backed_up_on: None,
            budget_dialog: None,
            list_message: None,
            present_since: None,
            forgot_prompt: None,
            forgot_snoozed_until: None,
            restore_prompt: None,
            tip_offset: 0,
            bar_drag_sent: false,
            bar_on_left: false,
            window_icon_dot: false,
            layout_changed_at: Instant::now(),
            updater: crate::updater::Updater::new(),
            mic: crate::mic::MicWatcher::start(ctx),
            mic_busy_since: None,
            mic_free_since: None,
            auto_call: AutoCall::Idle,
            mic_log,
            logged_mic_apps: None,
            logged_auto_call: None,
            logged_call_windows: Vec::new(),
            call_named: false,
            last_mic_check: Instant::now(),
            hotkeys: None,
            shortcuts_dialog: None,
            focus_search: false,
            break_reminder: BreakReminder::new(),
            breathing: None,
            item_history: None,
            was_on_pause: false,
            sleep_prompt: None,
            new_template_project: String::new(),
        };
        crate::i18n::apply(app.data.settings.language);
        app.data.localize_internal_names();
        app.hotkeys = GlobalHotkeys::start(ctx, app.data.settings.shortcuts.globals());
        // Var vinduet foldet sammen ved sidste lukning, har det allerede den lille
        // størrelse; den normale størrelse er gemt for sig.
        if app.data.compact {
            app.layout = WindowLayout::Bar;
        }
        ctx.set_theme(app.data.theme.to_egui());
        if app.error.is_none() {
            app.save();
            app.show_changelog_after_update();
            app.show_help_first_time();
        }
        app
    }

    fn save(&mut self) {
        // Første gemning hver dag: kopiér filen, som den var, før dagens ændringer.
        let today = Local::now().date_naive();
        if self.backed_up_on != Some(today) {
            self.backed_up_on = Some(today);
            if let Err(e) = crate::backup::daily(&self.path, today) {
                self.error = Some(tr!("backup.err", error = e));
            }
        }
        if let Err(e) = self.data.save(&self.path) {
            self.error = Some(e);
        }
    }

    fn apply_window_level(&self, ctx: &egui::Context) {
        let level = if self.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
        // egui sender kun temaet til titellinjen i første frame, før vinduet er
        // vist, og der bliver det ikke altid brugt. Så bjælken kunne være mørk
        // og programmet lyst (#17). Send det igen sammen med niveauet.
        ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(self.data.theme.to_window_theme()));
    }

    /// eframe husker størrelse og placering, men på X11 lægger vinduesmanageren
    /// titellinjen oven i den placering vi beder om, så vinduet kryber nedad for
    /// hver genstart. eframe har bedt om den gemte placering og fået den plus en
    /// forskydning; bed derfor om den gemte placering minus forskydningen.
    fn restore_window_position(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.move_to_primary_if_offscreen(ctx, frame, false) {
            return;
        }
        let Some([x, y]) = self.data.window_pos else { return };
        let Some(inner) = ctx.input(|i| i.viewport().inner_rect) else { return };
        let wanted = Pos2::new(x, y);
        let drift = wanted - inner.min;
        if drift.length() <= 1.0 {
            return;
        }
        // eframe bad om den gemte placering og fik den plus titellinjen: det er
        // netop den justering, senere flytninger skal have.
        if drift.length() < 100.0 {
            // Ved opstart har vinduet titellinje, medmindre det er foldet sammen.
            self.move_correction[usize::from(!self.data.compact)] = drift;
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(wanted + drift));
        } else {
            // Langt fra: eframe har selv flyttet vinduet, f.eks. ind fra Windows'
            // "minimeret"-placering (−32000), hvis programmet blev lukket
            // minimeret. Så er afstanden ikke en titellinje, og lagt til igen
            // ville den sende vinduet ud over kanten (#12).
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(wanted));
        }
    }

    /// Ligger vinduet (eller den gemte placering) uden for alle tilsluttede
    /// skærme, f.eks. fordi en ekstra skærm er taget fra, flyttes det til midten
    /// af den primære skærm. `actual` tjekker hvor vinduet står nu i stedet for
    /// den gemte placering. Returnerer true, hvis vinduet blev flyttet.
    fn move_to_primary_if_offscreen(&mut self, ctx: &egui::Context, frame: &eframe::Frame, actual: bool) -> bool {
        use winit::dpi::{PhysicalPosition, PhysicalSize};

        let Some(window) = frame.winit_window() else { return false };
        let monitors: Vec<egui::Rect> = window.available_monitors().map(|m| monitor_rect(&m)).collect();
        if monitors.is_empty() {
            return false;
        }
        let size = window.outer_size();
        let size = Vec2::new(size.width as f32, size.height as f32);
        // Den placering vinduet skal have: den gemte, ellers den nuværende (i pixels).
        let ppp = ctx.pixels_per_point();
        let wanted = match self.data.window_pos.filter(|_| !actual) {
            Some([x, y]) => Pos2::new(x * ppp, y * ppp),
            None => match window.inner_position() {
                Ok(p) => Pos2::new(p.x as f32, p.y as f32),
                Err(_) => return false,
            },
        };
        if title_bar_visible(egui::Rect::from_min_size(wanted, size), &monitors) {
            return false;
        }

        let Some(target) = window.primary_monitor().or_else(|| window.available_monitors().next()) else {
            return false;
        };
        let screen = monitor_rect(&target);
        // Større end skærmen? Gør vinduet mindre, så det kan ses helt.
        let fitted = size.min(screen.size() - Vec2::splat(80.0)).max(Vec2::splat(200.0));
        if fitted != size {
            let _ = window.request_inner_size(PhysicalSize::new(fitted.x as u32, fitted.y as u32));
        }
        let pos = screen.center() - fitted / 2.0;
        window.set_outer_position(PhysicalPosition::new(pos.x as i32, pos.y as i32));
        // Den gamle placering er ugyldig; den nye gemmes, når vinduet lukkes.
        self.data.window_pos = None;
        self.save();
        true
    }

    fn timer_tab(&mut self, ui: &mut egui::Ui) {
        if !self.call_dialog_open && !self.data.unassigned.is_empty() {
            let text = tr!("timer.calls_waiting", count = self.data.unassigned.len());
            if ui
                .add(egui::Button::new(RichText::new(text).color(Color32::BLACK)).fill(Color32::from_rgb(240, 190, 80)))
                .clicked()
            {
                self.call_dialog_open = true;
            }
        }
        ui.vertical_centered(|ui| {
            self.round_timer(ui);
            if self.data.active.is_none() {
                ui.label(RichText::new(tr!("timer.choose_project")).weak());
            }
        });
        ui.add_space(4.0);

        let active = self.data.active.as_ref();
        let on_call = active.is_some_and(|a| a.project_id.is_none());
        let on_pause = active.is_some_and(|a| a.is_note(NOTE_PAUSE));
        let running = active.is_some();
        let (call_hint, stop_hint) = (self.shortcut_hint(Action::Call), self.shortcut_hint(Action::Stop));
        let pause_hint = self.shortcut_hint(Action::Pause);
        let resume_label = self.data.resume_project().map(|id| self.data.project_label(id));
        let (call_text, call_hover) = match (on_call, &resume_label) {
            (true, Some(label)) => (tr!("timer.end_call"), format!("{}{call_hint}", tr!("timer.end_call_resume_hint", project = label))),
            (true, None) => (tr!("timer.end_call"), format!("{}{call_hint}", tr!("timer.end_call_hint"))),
            (false, _) => (tr!("timer.phone"), format!("{}{call_hint}", tr!("timer.phone_hint"))),
        };
        ui.columns(3, |cols| {
            let size = [cols[0].available_width(), 26.0];
            // Markeret mens opkaldet kører, ligesom pause-knappen.
            let call_button =
                if on_call { egui::Button::selectable(true, call_text) } else { egui::Button::new(call_text) };
            if cols[0].add(call_button.min_size(size.into())).on_hover_text(call_hover).clicked() {
                self.data.toggle_call();
                self.save();
            }
            let (pause_text, pause_hover) = if on_pause {
                match self.data.resume_project() {
                    Some(id) => (
                        tr!("timer.resume"),
                        format!("{}{pause_hint}", tr!("timer.resume_hint", project = self.data.project_label(id))),
                    ),
                    None => (tr!("timer.end_break"), format!("{}{pause_hint}", tr!("timer.end_break_hint"))),
                }
            } else {
                (tr!("timer.break"), format!("{}{pause_hint}", tr!("timer.break_hint")))
            };
            // Almindelig knap som de andre; markeret mens pausen kører.
            let pause_button = if on_pause {
                egui::Button::selectable(true, pause_text)
            } else {
                egui::Button::new(pause_text)
            };
            if cols[1]
                .add(pause_button.min_size(size.into()))
                .on_hover_text(pause_hover)
                .clicked()
            {
                self.data.toggle_pause();
                self.save();
            }
            if cols[2]
                .add_enabled(running, egui::Button::new(tr!("timer.stop")).min_size(size.into()))
                .on_hover_text(format!("{}{stop_hint}", tr!("timer.stop_hint")))
                .clicked()
            {
                self.data.stop();
                self.save();
            }
        });
        if on_pause {
            ui.add_space(4.0);
            let label = if self.breathing.is_some() { tr!("timer.breathing_running") } else { tr!("timer.breathing") };
            let button = egui::Button::new(label).min_size(Vec2::new(ui.available_width(), 26.0));
            if ui.add_enabled(self.breathing.is_none(), button).on_hover_text(tr!("timer.breathing_hint")).clicked() {
                self.start_breathing();
            }
        }
        if let Some(active) = &mut self.data.active {
            ui.add_space(4.0);
            let changed = ui
                .add(
                    egui::TextEdit::singleline(&mut active.description)
                        .hint_text(tr!("timer.note_hint"))
                        .desired_width(f32::INFINITY),
                )
                .changed();
            if changed {
                self.save();
            }
        }
        if on_call {
            ui.add_space(4.0);
            self.call_target_panel(ui);
        }
        ui.separator();

        let today = Local::now().date_naive();
        let totals: HashMap<Id, Duration> = self.data.totals_on(today).into_iter().collect();
        let active_project = self.data.active.as_ref().and_then(|a| a.project_id);
        let mut start_project = None;
        let mut add_project = None;

        let search = ui
            .horizontal(|ui| {
                // Sortering skiftes direkte her (A–Å / senest brugt).
                let (text, hover) = match self.data.settings.sort {
                    SortOrder::Name => (tr!("list.sort_name"), tr!("list.sort_name_hint")),
                    SortOrder::Recent => (tr!("list.sort_recent"), tr!("list.sort_recent_hint")),
                };
                let mut response = None;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!self.search.is_empty(), egui::Button::new("×")).clicked() {
                        self.search.clear();
                    }
                    if ui.button(text).on_hover_text(hover).clicked() {
                        self.data.settings.sort = match self.data.settings.sort {
                            SortOrder::Name => SortOrder::Recent,
                            SortOrder::Recent => SortOrder::Name,
                        };
                        self.save();
                    }
                    response = Some(ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text(tr!("common.search_hint"))
                            .desired_width(ui.available_width()),
                    ));
                });
                response
            })
            .inner
            .expect("søgefeltet tegnes altid");
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.new_customer)
                    .hint_text(tr!("list.new_customer"))
                    .desired_width(ui.available_width() - 30.0),
            );
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("+").clicked() || enter) && !self.new_customer.trim().is_empty() {
                let name = self.new_customer.trim().to_owned();
                let (_, auto_start) = self.data.create_customer(&name);
                if let Some(project_id) = auto_start {
                    self.data.start(project_id);
                }
                self.new_customer.clear();
                self.save();
            }
        });
        ui.add_space(4.0);
        let searching = !self.search.trim().is_empty();
        let force_open = self.set_all_open.take();
        let mut fold_all = None;
        let mut sort = None;
        let mut delete = None;
        let mut rename = None;
        let mut show_entries = None;
        let mut toggle_favorite = None;
        let mut edit_budget = None;

        // Fuld bredde, så rullebjælken bliver i højre side, også når alle
        // kunder er klappet sammen, og listen er smal (#35).
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            let mut any_match = false;
            // ⭐ Favoritter øverst (ikke under søgning, hvor listen viser træffere).
            let favorites = self.data.favorites();
            if !searching && !favorites.is_empty() {
                egui::CollapsingHeader::new(RichText::new(tr!("favorites.title")).strong())
                    .id_salt("favorites")
                    .default_open(true)
                    .open(force_open)
                    .show(ui, |ui| {
                        for project in favorites {
                            let is_active = active_project == Some(project.id);
                            let name = self.data.project_label(project.id);
                            let label = if is_active {
                                RichText::new(format!("▶ {name}")).color(ACCENT).strong()
                            } else {
                                RichText::new(name)
                            };
                            let response = ui.selectable_label(is_active, label);
                            if response.clicked() && !is_active {
                                start_project = Some(project.id);
                            }
                            response.context_menu(|ui| {
                                if ui.button(tr!("favorites.remove")).clicked() {
                                    toggle_favorite = Some(project.id);
                                    ui.close();
                                }
                            });
                        }
                    });
            }
            for customer in self.data.visible_customers() {
                let Some(projects) = matching_projects(&self.data, customer, &self.search) else {
                    continue;
                };
                any_match = true;
                let internal_customer = self.data.is_internal_customer(customer.id);
                let header = egui::CollapsingHeader::new(RichText::new(&customer.name).strong())
                    .id_salt(customer.id)
                    .default_open(true)
                    // Under søgning vises alle træffere, også under sammenklappede kunder.
                    .open(if searching { Some(true) } else { force_open })
                    .show(ui, |ui| {
                        for project in projects {
                            ui.horizontal(|ui| {
                                let is_active = active_project == Some(project.id);
                                let label = if is_active {
                                    RichText::new(format!("▶ {}", project.name)).color(ACCENT).strong()
                                } else {
                                    RichText::new(&project.name)
                                };
                                let response = ui.selectable_label(is_active, label);
                                if response.clicked() && !is_active {
                                    start_project = Some(project.id);
                                }
                                response.context_menu(|ui| {
                                    if ui.button(tr!("item.show")).clicked() {
                                        show_entries = Some(Scope::Project(project.id));
                                        ui.close();
                                    }
                                    let favorite = egui::Button::selectable(project.favorite, tr!("favorites.toggle"));
                                    if ui.add(favorite).clicked() {
                                        toggle_favorite = Some(project.id);
                                        ui.close();
                                    }
                                    if ui.button(tr!("budget.menu")).clicked() {
                                        edit_budget = Some(project.id);
                                        ui.close();
                                    }
                                    ui.separator();
                                    if ui.button(tr!("list.rename")).clicked() {
                                        rename = Some((Item::Project(project.id), project.name.clone()));
                                        ui.close();
                                    }
                                    let item = ui
                                        .add_enabled(!self.data.is_internal(project.id), egui::Button::new(tr!("list.delete_project")))
                                        .on_disabled_hover_text(tr!("list.internal_project_locked"));
                                    if item.clicked() {
                                        delete = Some(Item::Project(project.id));
                                        ui.close();
                                    }
                                });
                                let budget = self.data.budget(project.id);
                                if totals.contains_key(&project.id) || budget.is_some() {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if let Some(d) = totals.get(&project.id) {
                                            ui.label(RichText::new(format_duration(*d)).weak().monospace());
                                        }
                                        // Timebudget: brugt i procent, gul fra 90 % og rød over.
                                        if let Some((used, budget)) = budget {
                                            let percent = budget_percent(used, budget);
                                            let color = budget_color(ui, percent);
                                            ui.label(RichText::new(format!("{percent} %")).small().color(color)).on_hover_text(tr!(
                                                "budget.used",
                                                used = format_hours_minutes(used),
                                                budget = format_hours_minutes(budget),
                                                percent = percent
                                            ));
                                        }
                                    });
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            let text = self.new_project.entry(customer.id).or_default();
                            let response = ui.add(
                                egui::TextEdit::singleline(text)
                                    .hint_text(tr!("list.new_project"))
                                    .desired_width(ui.available_width() - 30.0),
                            );
                            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (ui.button("+").clicked() || enter) && !text.trim().is_empty() {
                                add_project = Some((customer.id, text.trim().to_owned()));
                                text.clear();
                            }
                        });
                    });
                header.header_response.context_menu(|ui| {
                    if ui.button(tr!("item.show")).clicked() {
                        show_entries = Some(Scope::Customer(customer.id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(tr!("list.expand_all")).clicked() {
                        fold_all = Some(true);
                        ui.close();
                    }
                    if ui.button(tr!("list.collapse_all")).clicked() {
                        fold_all = Some(false);
                        ui.close();
                    }
                    ui.separator();
                    let by_name = self.data.settings.sort == SortOrder::Name;
                    if ui.add(egui::Button::selectable(by_name, tr!("list.menu_sort_name"))).clicked() {
                        sort = Some(SortOrder::Name);
                        ui.close();
                    }
                    if ui.add(egui::Button::selectable(!by_name, tr!("list.menu_sort_recent"))).clicked() {
                        sort = Some(SortOrder::Recent);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(tr!("list.rename")).clicked() {
                        rename = Some((Item::Customer(customer.id), customer.name.clone()));
                        ui.close();
                    }
                    let item = ui
                        .add_enabled(!internal_customer, egui::Button::new(tr!("list.delete_customer")))
                        .on_disabled_hover_text(tr!("list.internal_customer_locked"));
                    if item.clicked() {
                        delete = Some(Item::Customer(customer.id));
                        ui.close();
                    }
                });
            }

            if searching && !any_match {
                ui.label(RichText::new(tr!("list.no_match")).weak());
            }
        });

        if let Some(order) = sort {
            self.data.settings.sort = order;
            self.save();
        }
        if fold_all.is_some() {
            self.set_all_open = fold_all;
            ui.ctx().request_repaint();
        }
        if delete.is_some() {
            self.confirm_delete = delete;
        }
        if let Some(scope) = show_entries {
            self.open_item_history(scope);
        }
        if let Some((item, name)) = rename {
            self.rename = Some(Rename { item, name, error: None, focused: false });
        }

        if let Some((customer_id, name)) = add_project {
            self.data.add_project(customer_id, &name);
            self.save();
        }
        if let Some(id) = toggle_favorite {
            self.data.toggle_favorite(id);
            self.save();
        }
        if let Some(id) = edit_budget {
            let hours = self.data.project(id).and_then(|p| p.budget_minutes).map_or(0.0, |m| m as f32 / 60.0);
            self.budget_dialog = Some((id, hours));
        }
        if let Some(id) = start_project {
            self.data.start(id);
            self.save();
        }
    }

    /// Rundt ur: ringen fyldes én gang i timen, prikken går rundt én gang i minuttet.
    /// Rundt ur: ringen fyldes én gang i timen, prikken går rundt én gang i minuttet.
    /// I midten den aktuelle tidstagning og under den dagens tid på projektet.
    /// Til højre en menu, der kan foldes ud (dagens samlede tid og projektets
    /// samlede tid); når den er ude, glider uret til venstre og gør plads.
    fn round_timer(&mut self, ui: &mut egui::Ui) {
        let size = 150.0;
        let width = ui.available_width();
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, size), egui::Sense::hover());
        let open = self.data.day_panel_open;
        let t = ui.ctx().animate_bool_responsive(egui::Id::unique("day_panel"), open);
        let (button_width, gap) = (18.0, 12.0);
        let today = Local::now().date_naive();
        let day_total = self.data.day_total(today);
        let rounded = self.data.rounded_day_total(today);
        // Projektets tid i alt eller denne uge; et klik i menuen skifter (#38).
        let week = self.data.day_panel_week;
        let project_total = self.data.active.as_ref().and_then(|a| a.project_id).map(|id| {
            if week { self.data.project_week_total(id, today) } else { self.data.project_total(id) }
        });
        let project_heading = format!("{} ⇄", if week { tr!("day.project_week") } else { tr!("day.project_total") });

        // Menuens tekster; menuen er præcis så bred som den længste, så uret og
        // menuen kan centreres sammen.
        let small = ui.style().text_styles[&egui::TextStyle::Small].clone();
        let weak = ui.visuals().weak_text_color();
        let mut panel_texts = vec![
            (tr!("day.today"), small.clone()),
            (format_duration(day_total), egui::FontId::monospace(16.0)),
        ];
        if let Some(rounded) = rounded {
            panel_texts.push((tr!("day.rounded", time = format_hours_minutes(rounded)), small.clone()));
        }
        let goal_text = self.data.settings.goal_for(today).map(|goal| {
            let work = self.data.work_day_total(today);
            tr!("goal.progress", goal = format_hours_minutes(goal), percent = budget_percent(work, goal))
        });
        if let Some(text) = &goal_text {
            panel_texts.push((text.clone(), small.clone()));
        }
        if let Some(total) = project_total {
            panel_texts.push((project_heading.clone(), small.clone()));
            panel_texts.push((format_duration(total), egui::FontId::monospace(13.0)));
        }
        let text_width = panel_texts
            .iter()
            .map(|(text, font)| ui.painter().layout_no_wrap(text.clone(), font.clone(), weak).size().x)
            .fold(0.0, f32::max);
        let panel_width = (text_width + 4.0).min((width - size - gap - button_width).max(40.0));
        // Knappen står fast ved højre kant (så man ikke skal flytte musen for at
        // folde menuen sammen igen); ur og menu centreres i pladsen ved siden af.
        let group_width = size + gap + panel_width;
        let open_left = row.left() + (width - button_width - group_width).max(0.0) / 2.0;
        let center = Pos2::new(row.center().x, row.center().y).lerp(Pos2::new(open_left + size / 2.0, row.center().y), t);
        let painter = ui.painter_at(row);
        let ring_width = 12.0;
        // Ringens midterlinje. Ring, bue og prik centreres alle om denne radius.
        let radius = size / 2.0 - ring_width / 2.0 - 1.0;
        let track = ui.visuals().widgets.inactive.bg_fill;

        // egui tegner cirklers kant *udenfor* radius, så træk en halv ringbredde fra.
        painter.circle_stroke(center, radius - ring_width / 2.0, Stroke::new(ring_width, track));

        let (elapsed, label, today_on_project) = match &self.data.active {
            Some(a) => {
                let (label, today_on_project) = match a.project_id {
                    Some(id) => (self.data.project_label(id), Some(self.data.project_day_total(today, id))),
                    None => match self.target_label(&a.target) {
                        Some(target) => (format!("📞 {target}"), None),
                        None => (tr!("common.phone_call"), None),
                    },
                };
                let label = match (&a.note, a.project_id) {
                    (Some(note), Some(_)) => format!("{label} ({})", note_label(note)),
                    _ => label,
                };
                (a.elapsed(), label, today_on_project)
            }
            None => (Duration::zero(), String::new(), None),
        };
        // Hele sekunder, så prikken tikker i takt med sekundtallet i midten.
        let secs = elapsed.num_seconds() as f32;

        if self.data.active.is_some() {
            let hour_fraction = (secs % 3600.0) / 3600.0;
            let point_at = |fraction: f32| {
                let angle = fraction * TAU - TAU / 4.0; // start kl. 12
                center + Vec2::angled(angle) * radius
            };
            let steps = (hour_fraction * 100.0).ceil().max(1.0) as usize;
            let arc: Vec<Pos2> = (0..=steps)
                .map(|i| point_at(hour_fraction * i as f32 / steps as f32))
                .collect();
            painter.add(egui::Shape::line(arc, Stroke::new(ring_width - 4.0, ACCENT)));
            painter.circle_filled(point_at((secs % 60.0) / 60.0), ring_width / 2.0 - 2.0, Color32::WHITE);
        }

        // Dagsmålet: en tynd grøn ring inden for uret, der fyldes i løbet af dagen.
        if let Some(goal) = self.data.settings.goal_for(today) {
            let work_today = self.data.work_day_total(today);
            let goal = goal.num_minutes();
            let goal_radius = radius - ring_width / 2.0 - 5.0;
            let fraction = (work_today.num_minutes() as f32 / goal as f32).clamp(0.0, 1.0);
            painter.circle_stroke(center, goal_radius, Stroke::new(3.0, track));
            if fraction > 0.0 {
                let steps = (fraction * 100.0).ceil().max(1.0) as usize;
                let arc: Vec<Pos2> = (0..=steps)
                    .map(|i| center + Vec2::angled(fraction * TAU * i as f32 / steps as f32 - TAU / 4.0) * goal_radius)
                    .collect();
                painter.add(egui::Shape::line(arc, Stroke::new(3.0, GOAL_COLOR)));
            }
        }

        let text_color = ui.visuals().strong_text_color();
        let current_pos = if today_on_project.is_some() { center - Vec2::new(0.0, 8.0) } else { center };
        painter.text(
            current_pos,
            egui::Align2::CENTER_CENTER,
            format_duration(elapsed),
            egui::FontId::monospace(24.0),
            text_color,
        );
        // Dagens tid på projektet.
        if let Some(today_on_project) = today_on_project {
            painter.text(
                center + Vec2::new(0.0, 16.0),
                egui::Align2::CENTER_CENTER,
                tr!("day.today_time", time = format_duration(today_on_project)),
                egui::FontId::monospace(12.0),
                ui.visuals().weak_text_color(),
            );
        }

        // Menuen til højre, som glider frem.
        if t > 0.01 {
            let panel_rect = egui::Rect::from_min_size(
                Pos2::new(open_left + size + gap, row.top()),
                Vec2::new(panel_width, size),
            );
            let mut panel = ui.new_child(
                egui::UiBuilder::new().max_rect(panel_rect).layout(egui::Layout::top_down(egui::Align::Min)),
            );
            panel.set_opacity(t);
            panel.set_clip_rect(panel_rect.expand(2.0));
            let lines_height = 30.0
                + if rounded.is_some() { 16.0 } else { 0.0 }
                + if goal_text.is_some() { 16.0 } else { 0.0 }
                + if project_total.is_some() { 42.0 } else { 0.0 };
            panel.add_space(((size - lines_height) / 2.0).max(0.0));
            panel.label(RichText::new(tr!("day.today")).weak().small());
            panel.label(RichText::new(format_duration(day_total)).monospace().size(16.0).strong());
            if let Some(rounded) = rounded {
                panel
                    .label(RichText::new(tr!("day.rounded", time = format_hours_minutes(rounded))).small().color(panel.visuals().warn_fg_color))
                    .on_hover_text(tr!("day.rounded_hint"));
            }
            if let Some(text) = &goal_text {
                panel.label(RichText::new(text).small().color(GOAL_COLOR)).on_hover_text(tr!("goal.hint"));
            }
            if let Some(total) = project_total {
                panel.add_space(8.0);
                let heading = egui::Button::new(RichText::new(&project_heading).weak().small()).frame(false);
                if panel.add(heading).on_hover_text(tr!("day.project_toggle_hint")).clicked() {
                    self.data.day_panel_week = !week;
                    self.save();
                }
                panel.label(RichText::new(format_duration(total)).monospace().size(13.0));
            }
        }

        // Knappen der folder menuen ud og ind, ved højre kant.
        let button_rect = egui::Rect::from_center_size(
            Pos2::new(row.right() - button_width / 2.0, row.center().y),
            Vec2::new(button_width, 44.0),
        );
        let (icon, hover) = if open { ("›", tr!("day.hide")) } else { ("‹", tr!("day.show")) };
        // I sit eget lille område (`put` ville flytte placeringen af det, der kommer under uret).
        let mut button_ui = ui.new_child(egui::UiBuilder::new().max_rect(button_rect));
        let button = egui::Button::new(RichText::new(icon).size(18.0)).min_size(button_rect.size());
        if button_ui.add(button).on_hover_text(hover).clicked() {
            self.data.day_panel_open = !open;
            self.save();
        }

        if !label.is_empty() {
            // Projektnavnet med en lille knap til at flytte tiden (ikke under en pause).
            let can_move = self.data.active.as_ref().is_some_and(|a| !a.is_note(NOTE_PAUSE));
            let font = egui::TextStyle::Body.resolve(ui.style());
            let text_width = ui.painter().layout_no_wrap(label.clone(), font, Color32::WHITE).size().x;
            let button_width = if can_move { 28.0 } else { 0.0 };
            ui.horizontal(|ui| {
                ui.add_space(((ui.available_width() - text_width - button_width) / 2.0).max(0.0));
                ui.add(egui::Label::new(RichText::new(&label).strong()).truncate());
                if can_move
                    && move_button(ui).on_hover_text(tr!("timer.move_hint")).clicked()
                {
                    self.picker = Some(ProjectPicker::moving());
                }
            });
            // Nærmer den kørende tid sig projektets timebudget, så sig det her.
            if let Some((used, budget)) = self.data.active.as_ref().and_then(|a| a.project_id).and_then(|id| self.data.budget(id))
            {
                let percent = budget_percent(used, budget);
                if percent >= 90 {
                    let color = budget_color(ui, percent);
                    let text = tr!(
                        "budget.warning",
                        used = format_hours_minutes(used),
                        budget = format_hours_minutes(budget),
                        percent = percent
                    );
                    ui.vertical_centered(|ui| ui.label(RichText::new(text).small().color(color)));
                }
            }
        }
    }

    fn history_tab(&mut self, ui: &mut egui::Ui) {
        // I ugevisningen bladres der en uge ad gangen.
        let week = self.history_view == HistoryView::Week;
        let step = Duration::days(if week { 7 } else { 1 });
        ui.horizontal(|ui| {
            if ui.button("◀").clicked() {
                self.history_date -= step;
            }
            let title = if week {
                let monday = week::monday_of(self.history_date);
                tr!(
                    "week.title",
                    week = self.history_date.iso_week().week(),
                    from = monday.format("%d-%m"),
                    to = (monday + Duration::days(6)).format("%d-%m")
                )
            } else {
                self.history_date.format("%d-%m-%Y").to_string()
            };
            ui.label(RichText::new(title).strong());
            if ui.button("▶").clicked() {
                self.history_date += step;
            }
            if ui.button(tr!("history.today")).clicked() {
                self.history_date = Local::now().date_naive();
            }
        });
        ui.separator();

        let entries = self.data.entries_on(self.history_date);
        let add_clicked = ui.horizontal(|ui| {
            ui.selectable_value(&mut self.history_view, HistoryView::Summary, tr!("history.summary"));
            // Kort navn, så tre faner og knapperne kan være i det smalle vindue.
            ui.selectable_value(&mut self.history_view, HistoryView::All, tr!("history.all_short"))
                .on_hover_text(tr!("history.all"));
            ui.selectable_value(&mut self.history_view, HistoryView::Week, tr!("week.tab"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("📤").on_hover_text(tr!("history.export_hint")).clicked() {
                    self.export = Some(ExportDialog::new());
                }
                ui.button("➕").on_hover_text(tr!("history.add_hint")).clicked()
            })
            .inner
        });
        if add_clicked.inner {
            let last_end = entries.iter().filter(|e| e.index.is_some()).map(|e| e.entry.end).max();
            self.editor = Some(EntryEditor::new(self.history_date, last_end));
        }
        ui.add_space(4.0);

        if self.history_view == HistoryView::Week {
            self.week_view(ui);
            return;
        }
        if entries.is_empty() {
            ui.label(RichText::new(tr!("history.none")).weak());
            return;
        }

        let totals = self.data.totals_on(self.history_date);
        let gaps = Data::gaps(&entries);
        let day_total = totals.iter().fold(Duration::zero(), |sum, (_, d)| sum + *d);
        let mut open_editor = None;
        let mut toggle_rounding = None;
        let mut show_entries = None;
        let mut toggle_fold = None;
        let mut set_billed = None;
        let date = self.history_date;

        // Projektnavnet afkortes, så tid, afrundet tid og flueben kan ses i det
        // smalle vindue (ca. 7 px pr. tegn; resten af rækken fylder ca. 210 px).
        let label_chars = ((ui.available_width() - 210.0) / 7.0).max(12.0) as usize;
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("history").striped(true).show(ui, |ui| {
                match self.history_view {
                    // Ugen er tegnet og returneret ovenfor.
                    HistoryView::Summary | HistoryView::Week => {
                        ui.label(RichText::new("✔").weak().small()).on_hover_text(tr!("history.col_billed_hint"));
                        ui.label("");
                        ui.label(RichText::new(tr!("history.col_time")).weak().small());
                        ui.label(RichText::new(tr!("history.col_rounded")).weak().small())
                            .on_hover_text(tr!("history.col_rounded_hint"));
                        ui.end_row();
                        for (project_id, d) in &totals {
                            let count = entries.iter().filter(|e| e.entry.project_id == *project_id).count();
                            let notes = day_notes(&entries, *project_id);
                            // Klik på den afrundede tid for at runde ned i stedet for op.
                            let r = self.data.rounded(date, *project_id, *d);
                            let open = self.history_open.contains(project_id);
                            ui.horizontal(|ui| {
                                // Afkrydsning af den afrundede tid (#16).
                                let state = self.data.billed_state(date, *project_id, r.time);
                                let mut checked = state != BilledState::No;
                                let response = ui.checkbox(&mut checked, "");
                                if response.changed() {
                                    set_billed = Some((*project_id, checked.then_some(r.time)));
                                }
                                match state {
                                    BilledState::No => {
                                        response.on_hover_text(tr!("history.col_billed_hint"));
                                    }
                                    BilledState::Yes => {
                                        response.on_hover_text(tr!("history.billed", time = format_hours_minutes(r.time)));
                                    }
                                    BilledState::Changed(old) => {
                                        // Hvor meget der mangler (eller er for meget), så man
                                        // ikke selv skal regne forskellen ud.
                                        let diff = r.time - old;
                                        let (sign, amount) = if diff > Duration::zero() { ("+", diff) } else { ("−", -diff) };
                                        let diff_text = if diff > Duration::zero() {
                                            tr!("history.billed_missing", time = format_hours_minutes(amount))
                                        } else {
                                            tr!("history.billed_too_much", time = format_hours_minutes(amount))
                                        };
                                        let hover = tr!(
                                            "history.billed_changed",
                                            old = format_hours_minutes(old),
                                            new = format_hours_minutes(r.time),
                                            diff = diff_text
                                        );
                                        let warn = RichText::new(format!("⚠ {sign}{}", format_hours_minutes(amount)))
                                            .small()
                                            .color(ui.visuals().warn_fg_color);
                                        let warning = ui.add(egui::Button::new(warn).frame(false)).on_hover_text(&hover);
                                        if warning.clicked() {
                                            set_billed = Some((*project_id, Some(r.time)));
                                        }
                                        response.on_hover_text(hover);
                                    }
                                }
                                // Klik på projektet folder dagens registreringer ud (#18).
                                let arrow = if open { "⏷" } else { "⏵" };
                                let full = self.data.project_label(*project_id);
                                // ⚠ med forskellen tager plads fra navnet.
                                let chars = if matches!(state, BilledState::Changed(_)) {
                                    label_chars.saturating_sub(7).max(8)
                                } else {
                                    label_chars
                                };
                                let label = format!("{arrow} {}", shorten(&full, chars));
                                let mut response = ui.add(egui::Button::new(label).frame(false));
                                if response.clicked() {
                                    toggle_fold = Some(*project_id);
                                }
                                response = response.on_hover_text(if notes.is_empty() {
                                    format!("{full}\n{}", tr!("history.fold_hint"))
                                } else {
                                    format!("{full}\n{}\n• {}", tr!("history.notes"), notes.join("\n• "))
                                });
                                response.context_menu(|ui| {
                                    if ui.button(tr!("item.show")).clicked() {
                                        show_entries = Some(Scope::Project(*project_id));
                                        ui.close();
                                    }
                                });
                            });
                            ui.label(RichText::new(format!("{count}×")).weak())
                                .on_hover_text(tr!("history.entry_count", count = count));
                            ui.label(RichText::new(format_duration(*d)).monospace());
                            let mut text = RichText::new(format_hours_minutes(r.time)).monospace();
                            if r.down {
                                text = RichText::new(format!("{} ↓", format_hours_minutes(r.time)))
                                    .monospace()
                                    .color(ui.visuals().warn_fg_color);
                            }
                            let hover = match (r.can_toggle, r.down) {
                                (false, _) => tr!("history.exact_quarter"),
                                (true, false) => tr!(
                                    "history.rounded_up_hint",
                                    from = format_duration(*d),
                                    to = format_hours_minutes(round_down_quarter(*d))
                                ),
                                (true, true) => tr!(
                                    "history.rounded_down_hint",
                                    from = format_duration(*d),
                                    to = format_hours_minutes(round_up_quarter(*d))
                                ),
                            };
                            if r.can_toggle {
                                let response = ui.add(egui::Button::new(text).frame(false)).on_hover_text(hover);
                                if response.clicked() {
                                    toggle_rounding = Some(*project_id);
                                }
                            } else {
                                ui.label(text).on_hover_text(hover);
                            }
                            ui.end_row();

                            if open {
                                for day_entry in entries.iter().filter(|e| e.entry.project_id == *project_id) {
                                    let e = &day_entry.entry;
                                    let mut times = format!("{}–{}", e.start.format("%H:%M"), e.end.format("%H:%M"));
                                    // Typen som ikon (som i bjælken), så rækken ikke brydes.
                                    match e.note.as_deref() {
                                        Some(NOTE_PAUSE) => times.push_str(" ☕"),
                                        Some(NOTE_PHONE) => times.push_str(" 📞"),
                                        _ => {}
                                    }
                                    ui.horizontal(|ui| {
                                        ui.add_space(28.0);
                                        ui.vertical(|ui| {
                                            match day_entry.index {
                                                Some(index) => {
                                                    let response = ui
                                                        .add(egui::Button::new(RichText::new(times).weak()).frame(false))
                                                        .on_hover_text(tr!("history.click_to_edit"));
                                                    if response.clicked() {
                                                        open_editor = Some(EntryEditor::edit(index, e));
                                                    }
                                                }
                                                None => {
                                                    ui.label(RichText::new(format!("▶ {times}")).color(ACCENT))
                                                        .on_hover_text(tr!("history.running"));
                                                }
                                            }
                                            if !e.description.is_empty() {
                                                ui.label(RichText::new(format!("📝 {}", shorten(&e.description, label_chars + 4))).weak().small())
                                                    .on_hover_text(&e.description);
                                            }
                                        });
                                    });
                                    ui.label("");
                                    ui.label(RichText::new(format_duration(e.duration())).monospace().weak());
                                    ui.end_row();
                                }
                            }
                        }
                    }
                    HistoryView::All => {
                        for day_entry in &entries {
                            let e = &day_entry.entry;
                            // Et hul før registreringen: tid, der ikke er registreret.
                            if let Some(&(from, to)) = gaps.iter().find(|(_, to)| *to == e.start) {
                                let text = RichText::new(tr!("gap.label")).italics().color(ui.visuals().warn_fg_color);
                                if ui.add(egui::Button::new(text).frame(false)).on_hover_text(tr!("gap.fill_hint")).clicked() {
                                    open_editor = Some(EntryEditor::new_between(from, to));
                                }
                                ui.label(
                                    RichText::new(format!("{}–{}", from.format("%H:%M"), to.format("%H:%M")))
                                        .monospace()
                                        .color(ui.visuals().warn_fg_color),
                                );
                                ui.label(RichText::new(format_duration(to - from)).monospace().weak());
                                ui.end_row();
                            }
                            let mut label = self.data.project_label(e.project_id);
                            if let Some(note) = &e.note {
                                label = format!("{label} ({})", note_label(note));
                            }
                            ui.vertical(|ui| {
                                match day_entry.index {
                                    Some(index) => {
                                        let response = ui.selectable_label(false, label).on_hover_text(tr!("history.click_to_edit"));
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
                                    // Den kørende timer kan ikke rettes, før den er stoppet.
                                    None => {
                                        ui.label(RichText::new(format!("▶ {label}")).color(ACCENT))
                                            .on_hover_text(tr!("history.running"));
                                    }
                                }
                                if !e.description.is_empty() {
                                    ui.label(RichText::new(format!("📝 {}", shorten(&e.description, 40))).weak().small())
                                        .on_hover_text(&e.description);
                                }
                            });
                            ui.label(
                                RichText::new(format!("{}–{}", e.start.format("%H:%M"), e.end.format("%H:%M")))
                                    .monospace()
                                    .weak(),
                            );
                            ui.label(RichText::new(format_duration(e.duration())).monospace());
                            ui.end_row();
                        }
                    }
                }
                ui.label(RichText::new(tr!("history.total")).strong());
                ui.label("");
                ui.label(RichText::new(format_duration(day_total)).monospace().strong());
                if self.history_view == HistoryView::Summary {
                    // Summen af de afrundede projekttider, som når der faktureres pr. projekt.
                    let rounded = totals
                        .iter()
                        .fold(Duration::zero(), |sum, (id, d)| sum + self.data.rounded(date, *id, *d).time);
                    ui.label(RichText::new(format_hours_minutes(rounded)).monospace().strong())
                        .on_hover_text(tr!("history.rounded_total_hint"));
                }
                ui.end_row();
            });
            // Huller i Summeret: kun en linje, der viser hen til Alle registreringer.
            if self.history_view == HistoryView::Summary && !gaps.is_empty() {
                ui.add_space(6.0);
                let total = gaps.iter().fold(Duration::zero(), |sum, (from, to)| sum + (*to - *from));
                let text = RichText::new(tr!("gap.summary", count = gaps.len(), time = format_hours_minutes(total)))
                    .small()
                    .color(ui.visuals().warn_fg_color);
                if ui.add(egui::Button::new(text).frame(false)).on_hover_text(tr!("gap.summary_hint")).clicked() {
                    self.history_view = HistoryView::All;
                }
            }
        });

        if open_editor.is_some() {
            self.editor = open_editor;
        }
        if let Some(project_id) = toggle_rounding {
            self.data.toggle_round_down(date, project_id);
            self.save();
        }
        if let Some(scope) = show_entries {
            self.open_item_history(scope);
        }
        if let Some(project_id) = toggle_fold
            && !self.history_open.remove(&project_id)
        {
            self.history_open.insert(project_id);
        }
        if let Some((project_id, time)) = set_billed {
            self.data.set_billed(date, project_id, time);
            self.save();
        }
    }

    fn entry_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.editor.as_mut() else { return };
        let action = editor.show(ctx, &mut self.data);
        // En kunde oprettet i vinduet gemmes med det samme (også ved Annuller).
        if editor.take_changed() {
            self.save();
        }
        if !action.apply(&mut self.data) {
            return;
        }
        self.editor = None;
        self.save();
    }
}

impl eframe::App for KronoLogApp {
    /// Kaldes før hver `ui` og også mens vinduet er skjult i systembakken, så
    /// timeren, systembakken og "væk"-registreringen virker uden vinduet.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray_actions(ctx);
        self.handle_second_start(ctx);
        let global_actions = self.hotkeys.as_ref().map(|h| h.poll()).unwrap_or_default();
        for action in global_actions {
            self.run_action(ctx, action, true);
        }
        self.handle_idle_events();
        self.handle_forgot_timer(ctx);

        if let Some(active) = &mut self.data.active {
            if self.last_heartbeat.elapsed() >= HEARTBEAT {
                active.last_seen = Local::now();
                self.last_heartbeat = Instant::now();
                self.save();
            }
            // Holder systembakkens status opdateret, også når vinduet er skjult.
            ctx.request_repaint_after(StdDuration::from_secs(1));
        }

        let pending = self.data.unassigned.len();
        if pending > self.seen_unassigned {
            self.call_dialog_open = true;
        }
        self.seen_unassigned = pending;

        // Et spørgsmål skal kunne ses: vis vinduet, hvis det er skjult.
        self.update_break_reminder(ctx);

        // Øvelsen følger pausen: starter evt. af sig selv, og lukker når pausen slutter.
        let on_pause = self.data.active.as_ref().is_some_and(|a| a.is_note(NOTE_PAUSE));
        if on_pause && !self.was_on_pause && self.data.settings.breathing_auto && self.breathing.is_none() {
            self.start_breathing();
        } else if !on_pause && self.was_on_pause {
            self.breathing = None;
        }
        self.was_on_pause = on_pause;
        if self.breathing.is_some() && !self.window_visible {
            // Øvelsesvinduet tegnes sammen med hovedvinduet; bed om det, selvom det er skjult.
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        }
        let needs_answer = self.away_prompt.is_some()
            || self.sleep_prompt.is_some()
            || (self.call_dialog_open && pending > 0)
            || self.break_reminder.is_open()
            || self.forgot_prompt.is_some();
        if needs_answer && !self.window_visible {
            self.show_window(ctx);
        }
        self.handle_microphone(ctx);
        self.handle_updates(ctx);
        self.update_tray();
        self.update_window_icon(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.tray.is_some() && self.data.settings.close_to_tray && !self.quitting {
                // Luk til systembakken: tiden kører videre.
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide_window(&ctx);
            } else if !self.quitting {
                self.prepare_quit(&ctx);
            }
        }

        // På X11 går "hold øverst" tabt, hvis det sættes før vinduet er vist,
        // så det sendes igen kort efter opstart.
        if !self.window_level_applied {
            let delay = StdDuration::from_millis(500);
            if self.started.elapsed() >= delay {
                self.apply_window_level(&ctx);
                self.restore_window_position(&ctx, frame);
                self.window_level_applied = true;
            } else {
                ctx.request_repaint_after(delay);
            }
        }
        // Sikkerhedsnet: endte vinduet alligevel uden for skærmene, hentes det
        // ind, når flytningerne ovenfor har sat sig.
        if self.window_level_applied && !self.position_verified {
            let delay = StdDuration::from_millis(1500);
            if self.started.elapsed() >= delay {
                self.position_verified = true;
                if ctx.input(|i| i.viewport().minimized) != Some(true) {
                    self.move_to_primary_if_offscreen(&ctx, frame, true);
                }
            } else {
                ctx.request_repaint_after(delay - self.started.elapsed());
            }
        }

        // Tegn igen præcis når uret skifter sekund, så prikken tikker jævnt.
        if let Some(active) = &self.data.active {
            let into_second = active.elapsed().num_milliseconds().rem_euclid(1000) as u64;
            ctx.request_repaint_after(StdDuration::from_millis(1000 - into_second + 5));
        }

        // Tooltips må ikke blive bredere end vinduet; ellers kan de ikke læses
        // i det smalle vindue. Lange tekster brydes i stedet om.
        let tooltip_width = (ctx.content_rect().width() - 16.0).clamp(120.0, 320.0);
        if ctx.global_style().spacing.tooltip_width != tooltip_width {
            ctx.all_styles_mut(|style| style.spacing.tooltip_width = tooltip_width);
        }

        if !self.dialog_open() {
            for action in self.data.settings.shortcuts.take_local_actions(&ctx) {
                self.run_action(&ctx, action, false);
            }
        }

        let layout = self.wanted_layout();
        if self.window_level_applied
            && (!self.layout_applied || layout != self.layout || self.applied_vertical != self.data.bar_vertical)
        {
            self.apply_layout(&ctx, frame, layout);
        }
        self.handle_pending_move(&ctx);
        self.update_bar_side(&ctx, frame);
        match layout {
            // Lige efter start kan vinduet stadig have bjælkens størrelse (f.eks.
            // hvis programmet blev slået ned med bjælken fremme), indtil
            // `apply_layout` når at sætte den. Så smalt kan det fulde vindue ikke
            // tegnes (egui går ned på negative bredder), så vent et øjeblik.
            WindowLayout::Normal if ctx.content_rect().width() < NORMAL_MIN_WIDTH => {
                ctx.request_repaint_after(StdDuration::from_millis(50));
            }
            WindowLayout::Normal => self.full_ui(ui, &ctx),
            WindowLayout::Bar => {
                egui::CentralPanel::default()
                    // Uden titellinje: en tynd kant, så bjælken ikke flyder ud i baggrunden.
                    .frame(
                        egui::Frame::central_panel(ui.style())
                            .inner_margin(if self.data.bar_vertical {
                                egui::Margin::symmetric(4, 8)
                            } else {
                                egui::Margin::symmetric(8, 4)
                            })
                            .stroke(ui.visuals().window_stroke),
                    )
                    .show(ui, |ui| self.compact_bar(ui, self.data.bar_vertical));
            }
            WindowLayout::Picker => {
                // Også uden titellinje, så bjælken ikke hopper, når vælgeren åbnes.
                let stroke = ui.visuals().window_stroke;
                egui::Panel::top("compact_bar")
                    .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 4)))
                    // Vælgeren skal bruge bredden, så bjælken står vandret over den.
                    .show(ui, |ui| self.compact_bar(ui, false));
                egui::CentralPanel::default().show(ui, |ui| self.project_picker(ui));
                ui.painter().rect_stroke(ui.max_rect(), 0.0, stroke, egui::StrokeKind::Inside);
            }
        }

        if self.call_dialog_open && !self.data.unassigned.is_empty() {
            self.call_dialog(&ctx);
        }
        if let Some(target) = self.confirm_delete {
            self.delete_dialog(&ctx, target);
        }
        self.entry_editor(&ctx);
        self.away_dialog(&ctx);
        self.sleep_dialog(&ctx);
        self.forgot_dialog(&ctx);
        self.budget_window(&ctx);
        self.break_dialog(&ctx);
        self.breathing_window(&ctx);
        self.new_exercise_dialog(&ctx);
        self.restore_dialog(&ctx);
        self.item_history_window(&ctx, frame);
        self.help_window(&ctx, frame);
        self.changelog_window(&ctx, frame);
        self.rename_dialog(&ctx);
        self.shortcuts_dialog(&ctx);
        if layout == WindowLayout::Normal && self.picker.is_some() {
            let modal = egui::Modal::new(egui::Id::unique("picker_modal")).show(&ctx, |ui| {
                ui.allocate_ui(Vec2::new(320.0, 380.0), |ui| {
                    ui.set_min_size(Vec2::new(320.0, 380.0));
                    self.project_picker(ui);
                });
            });
            if modal.should_close() {
                self.picker = None;
            }
        }
        if let Some(export) = self.export.as_mut()
            && !export.show(&ctx, &self.data, frame, self.always_on_top)
        {
            self.export = None;
        }
    }
}

impl KronoLogApp {
    /// Det fulde program: faner, indstillinger og Timer/Historik.
    fn full_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.toggle_value(&mut self.settings_open, "☰").on_hover_text(tr!("settings.title"));
                ui.selectable_value(&mut self.tab, Tab::Timer, tr!("tabs.timer"));
                // Historikken åbner altid i den summerede visning.
                if ui.selectable_value(&mut self.tab, Tab::History, tr!("tabs.history")).changed() {
                    self.history_view = HistoryView::Summary;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fold_hint = self.shortcut_hint(Action::Fold);
                    if fold_button(ui, self.data.bar_vertical)
                        .on_hover_text(format!("{}{fold_hint}", tr!("tabs.fold_hint")))
                        .clicked()
                    {
                        self.data.compact = true;
                        self.save();
                    }
                    if ui
                        .toggle_value(&mut self.always_on_top, "📌")
                        .on_hover_text(tr!("tabs.on_top_hint"))
                        .changed()
                    {
                        self.apply_window_level(ctx);
                    }
                    let help_hint = self.shortcut_hint(Action::Help);
                    if ui.button("?").on_hover_text(format!("{}{help_hint}", tr!("action.help"))).clicked() {
                        self.open_help(None);
                    }
                });
            });
        });

        self.update_banner(ui, ctx);

        if let Some(error) = self.error.clone() {
            egui::Panel::bottom("error").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if ui.small_button("×").clicked() {
                        self.error = None;
                    }
                });
            });
        }

        let mut settings_open = self.settings_open;
        egui::Panel::left("settings")
            .resizable(false)
            .drag_to_open(false)
            .exact_size(210.0)
            .show_collapsible(ui, &mut settings_open, |ui| {
                // Indstillingerne er højere end et lavt vindue; scroll i stedet for at skære af.
                // En fast scrollbar: egui's standard ("flydende") er usynlig, indtil musen
                // er over den, så man ikke kan se, at der er mere.
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        // Hold indholdet inden for panelet (ellers skubber de brede
                        // skydere scrollbaren ud over kanten, hvor den ikke ses).
                        ui.set_max_width(ui.available_width());
                        ui.spacing_mut().slider_width = 80.0;
                        self.settings_panel(ui, ctx);
                    });
            });
        self.settings_open = settings_open;

        if self.tab == Tab::Timer {
            self.tip_line(ui);
        }
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Timer => self.timer_tab(ui),
            Tab::History => self.history_tab(ui),
        });
    }
}

impl KronoLogApp {
    /// Spørger hvor det ældste ikke-registrerede telefonopkald skal registreres.
    /// Enter registrerer (når et mål er udfyldt), Esc lukker dialogen uden at
    /// miste opkaldet.
    fn call_dialog(&mut self, ctx: &egui::Context) {
        let call = self.data.unassigned[0].clone();
        let mut target = call.target.clone();
        // Noten fra under opkaldet; kan skrives eller rettes her (#36).
        let mut description = call.description.clone();
        let internal = self.data.internal_project_id();
        let internal_name = self.data.project(internal).map_or_else(String::new, |p| p.name.clone());
        let mut confirm = false;
        let mut discard = false;

        let modal = egui::Modal::new(egui::Id::unique("call_dialog")).show(ctx, |ui| {
            ui.set_width(270.0);
            ui.heading(tr!("common.phone_call"));
            ui.label(format!(
                "{}  {}–{}  ({})",
                call.start.format("%d-%m"),
                call.start.format("%H:%M"),
                call.end.format("%H:%M"),
                format_duration(call.duration()),
            ));
            if self.data.unassigned.len() > 1 {
                ui.label(RichText::new(tr!("call.more_waiting", count = self.data.unassigned.len() - 1)).weak());
            }
            ui.label(tr!("call.where"));
            ui.separator();

            let full = Vec2::new(ui.available_width(), 24.0);
            let response = ui.add(
                egui::Button::selectable(target.project_id == Some(internal), RichText::new(internal_name).strong())
                    .min_size(full),
            );
            if response.clicked() {
                target = CallTarget { project_id: Some(internal), ..Default::default() };
            }
            confirm |= response.double_clicked();
            ui.add_space(4.0);

            search_field(ui, &mut self.dialog_search);
            let mut matches = Vec::new();
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for customer in self.data.visible_customers() {
                    let Some(projects) = matching_projects(&self.data, customer, &self.dialog_search) else {
                        continue;
                    };
                    let projects: Vec<_> = projects.into_iter().filter(|p| p.id != internal).collect();
                    if projects.is_empty() {
                        continue;
                    }
                    matches.extend(projects.iter().map(|p| p.id));
                    ui.label(RichText::new(&customer.name).strong());
                    for project in projects {
                        let response = ui.selectable_label(target.project_id == Some(project.id), format!("   {}", project.name));
                        if response.clicked() {
                            target = CallTarget { project_id: Some(project.id), ..Default::default() };
                        }
                        confirm |= response.double_clicked();
                    }
                }
            });

            ui.separator();
            ui.label(RichText::new(tr!("call.or_type")).weak());
            typed_target_fields(ui, &mut target);
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(&mut description)
                    .hint_text(tr!("call.note_hint"))
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(4.0);

            let complete = self.data.target_is_complete(&target);
            ui.horizontal(|ui| {
                confirm |= ui
                    .add_enabled(complete, egui::Button::new(tr!("call.register")))
                    .on_hover_text("Enter")
                    .clicked();
                if ui.button(tr!("call.later")).on_hover_text("Esc").clicked() {
                    ui.close();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr!("call.discard")).on_hover_text(tr!("call.discard_hint")).clicked() {
                        discard = true;
                    }
                });
            });
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                // Har søgningen kun ét træf, vælger Enter det.
                if !complete && !self.dialog_search.trim().is_empty() && matches.len() == 1 {
                    target = CallTarget { project_id: Some(matches[0]), ..Default::default() };
                }
                confirm = true;
            }
            confirm &= self.data.target_is_complete(&target);
        });

        if modal.should_close() {
            self.call_dialog_open = false;
        }
        if target != call.target || description != call.description {
            self.data.unassigned[0].target = target;
            self.data.unassigned[0].description = description;
            self.save();
        }
        if confirm && self.data.register_first_unassigned() {
            self.dialog_search.clear();
            self.save();
        } else if discard {
            self.data.discard_first_unassigned();
            self.dialog_search.clear();
            self.save();
        }
    }

    /// Omdøb en kunde eller et projekt. Enter gemmer, Esc annullerer.
    fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some(Rename { item, name, error, focused }) = self.rename.as_mut() else { return };
        let item = *item;
        let title = match item {
            Item::Customer(_) => tr!("rename.customer_title"),
            Item::Project(id) => {
                let customer = self.data.project(id).and_then(|p| self.data.customer(p.customer_id));
                tr!("rename.project_title", customer = customer.map_or("?", |c| c.name.as_str()))
            }
        };
        let mut save = false;
        let mut cancel = false;

        let modal = egui::Modal::new(egui::Id::unique("rename_dialog")).show(ctx, |ui| {
            ui.set_width(260.0);
            ui.heading(title);
            ui.add_space(4.0);
            let response = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
            // Markér feltet, når dialogen åbner, så man kan skrive med det samme.
            if !*focused {
                response.request_focus();
                *focused = true;
            }
            if response.changed() {
                *error = None;
            }
            if let Some(e) = error.as_ref() {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                save |= ui.button(tr!("common.save")).on_hover_text("Enter").clicked();
                cancel |= ui.button(tr!("common.cancel")).on_hover_text("Esc").clicked();
            });
            save |= ui.input(|i| i.key_pressed(egui::Key::Enter));
        });
        cancel |= modal.should_close();

        if save {
            let result = match item {
                Item::Customer(id) => self.data.rename_customer(id, name),
                Item::Project(id) => self.data.rename_project(id, name),
            };
            match result {
                Ok(()) => {
                    self.rename = None;
                    self.save();
                }
                Err(e) => *error = Some(e),
            }
        } else if cancel {
            self.rename = None;
        }
    }

    /// Spørger om man er sikker, før en kunde eller et projekt slettes.
    fn delete_dialog(&mut self, ctx: &egui::Context, target: Item) {
        let (title, name, details) = match target {
            Item::Customer(id) => {
                let name = self.data.customer(id).map_or_else(String::new, |c| c.name.clone());
                let projects = self.data.visible_projects().filter(|p| p.customer_id == id).count();
                let (count, total) = self.data.customer_stats(id);
                let details = tr!("delete.customer_details", projects = projects, count = count, total = format_duration(total));
                (tr!("delete.customer_title"), name, details)
            }
            Item::Project(id) => {
                let (count, total) = self.data.project_stats(id);
                let details = tr!("delete.project_details", count = count, total = format_duration(total));
                (tr!("delete.project_title"), self.data.project_label(id), details)
            }
        };
        let affects_timer = self.data.active.as_ref().and_then(|a| a.project_id).is_some_and(|id| match target {
            Item::Project(p) => p == id,
            Item::Customer(c) => self.data.project(id).is_some_and(|p| p.customer_id == c),
        });

        let mut confirmed = false;
        let modal = egui::Modal::new(egui::Id::unique("delete_dialog")).show(ctx, |ui| {
            ui.set_width(260.0);
            ui.heading(format!("🗑 {title}"));
            ui.label(tr!("delete.confirm", name = name));
            ui.add_space(4.0);
            ui.label(RichText::new(details).weak());
            if affects_timer {
                ui.label(RichText::new(tr!("delete.timer_running")).color(ui.visuals().warn_fg_color));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let delete = egui::Button::new(RichText::new(tr!("common.delete")).color(Color32::WHITE))
                    .fill(Color32::from_rgb(200, 60, 60));
                if ui.add(delete).clicked() {
                    confirmed = true;
                }
                if ui.button(tr!("common.cancel")).on_hover_text("Esc").clicked() {
                    ui.close();
                }
            });
        });

        if confirmed {
            match target {
                Item::Customer(id) => self.data.delete_customer(id),
                Item::Project(id) => self.data.delete_project(id),
            }
            self.confirm_delete = None;
            self.save();
        } else if modal.should_close() {
            self.confirm_delete = None;
        }
    }

    /// Valg af projekt mens et opkald kører, så det registreres direkte når det slutter.
    fn call_target_panel(&mut self, ui: &mut egui::Ui) {
        let Some(active) = &self.data.active else { return };
        let mut target = active.target.clone();

        ui.label(RichText::new(tr!("call.register_on")).weak());
        let selected = match target.project_id {
            Some(id) => self.data.project_label(id),
            None => tr!("call.choose_existing"),
        };
        egui::ComboBox::from_id_salt("call_target")
            .width(ui.available_width() - 8.0)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                // Samme rækkefølge som listerne (Intern først).
                let ids: Vec<Id> = self
                    .data
                    .visible_customers()
                    .flat_map(|c| self.data.customer_projects(c.id))
                    .map(|p| p.id)
                    .collect();
                for id in ids {
                    if ui.selectable_label(target.project_id == Some(id), self.data.project_label(id)).clicked() {
                        target = CallTarget { project_id: Some(id), ..Default::default() };
                    }
                }
            });
        typed_target_fields(ui, &mut target);

        if let Some(active) = &mut self.data.active
            && active.target != target
        {
            active.target = target;
            self.save();
        }
    }

    /// "Kunde / Projekt" for et opkalds mål, hvis det er udfyldt.
    fn target_label(&self, target: &CallTarget) -> Option<String> {
        if let Some(id) = target.project_id {
            return Some(self.data.project_label(id));
        }
        let (customer, project) = (target.customer.trim(), target.project.trim());
        (!customer.is_empty() && !project.is_empty()).then(|| format!("{customer} / {project}"))
    }
}

impl KronoLogApp {
    /// Om en dialog er åben (så genveje og bjælken venter).
    fn dialog_open(&self) -> bool {
        (self.call_dialog_open && !self.data.unassigned.is_empty())
            || self.away_prompt.is_some()
            || self.confirm_delete.is_some()
            || self.editor.is_some()
            || self.rename.is_some()
            || self.export.is_some()
            || self.shortcuts_dialog.is_some()
            || self.break_reminder.is_open()
            || self.sleep_prompt.is_some()
            || self.new_exercise.is_some()
            || self.restore_prompt.is_some()
            || self.forgot_prompt.is_some()
            || self.budget_dialog.is_some()
    }

    /// Udfører en handling fra en genvejstast. `global` er når genvejen blev
    /// trykket i et andet program; så hentes vinduet frem, hvis det skal bruges.
    fn run_action(&mut self, ctx: &egui::Context, action: Action, global: bool) {
        match action {
            Action::SwitchProject => {
                if global {
                    self.show_window(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                self.picker = match (&self.picker, global) {
                    (Some(_), false) => None,
                    _ => Some(ProjectPicker::default()),
                };
            }
            Action::Pause => self.data.toggle_pause(),
            Action::Call => self.data.toggle_call(),
            Action::Stop => self.data.stop(),
            Action::ToggleWindow => {
                if self.window_visible {
                    self.hide_window(ctx);
                } else {
                    self.show_window(ctx);
                }
            }
            Action::Fold => self.data.compact = !self.data.compact,
            Action::Search => {
                if self.data.compact {
                    self.picker = Some(ProjectPicker::default());
                } else {
                    self.tab = Tab::Timer;
                    self.focus_search = true;
                }
            }
            Action::TimerTab | Action::HistoryTab => {
                self.data.compact = false;
                self.tab = if action == Action::TimerTab { Tab::Timer } else { Tab::History };
                self.history_view = HistoryView::Summary;
            }
            Action::Export => {
                self.data.compact = false;
                self.export = Some(ExportDialog::new());
            }
            Action::Help => self.open_help(None),
        }
        self.save();
        ctx.request_repaint();
    }

    /// " (Ctrl+P)" til tooltips, hvis handlingen har en genvej i programmet.
    fn shortcut_hint(&self, action: Action) -> String {
        self.data.settings.shortcuts.local(action).map_or_else(String::new, |c| format!(" ({c})"))
    }

    pub fn with_instance(mut self, instance: crate::single_instance::Instance) -> Self {
        self.instance = Some(instance);
        self
    }

    /// Programmet blev startet igen: vis det kørende vindue i stedet.
    fn handle_second_start(&mut self, ctx: &egui::Context) {
        let Some(instance) = &self.instance else { return };
        instance.set_context(ctx);
        let show = instance.poll().into_iter().any(|c| matches!(c, crate::single_instance::Command::Show));
        if show {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            self.show_window(ctx);
        }
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.window_visible = true;
        // Når vinduet vises igen, går "hold øverst" og placeringen tabt på X11;
        // sæt dem igen efter et øjeblik, som ved opstart.
        self.started = Instant::now();
        self.window_level_applied = false;
        self.position_verified = false;
        ctx.request_repaint();
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.data.window_pos = Some([rect.min.x, rect.min.y]);
            self.save();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.window_visible = false;
        // Vinduerne tegnes sammen med hovedvinduet og ville stå frosset.
        self.item_history = None;
        self.help = None;
        self.changelog = None;
    }

    /// Stopper timeren og gemmer, inden programmet lukker.
    fn prepare_quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        self.data.stop();
        if self.window_visible
            && let Some(rect) = ctx.input(|i| i.viewport().inner_rect)
        {
            self.data.window_pos = Some([rect.min.x, rect.min.y]);
        }
        self.save();
    }

    fn handle_tray_actions(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else { return };
        for action in tray.poll() {
            match action {
                TrayAction::Toggle if self.window_visible => self.hide_window(ctx),
                TrayAction::Toggle => self.show_window(ctx),
                TrayAction::Start(id) => self.data.start(id),
                TrayAction::Call => self.data.toggle_call(),
                TrayAction::Pause => self.data.toggle_pause(),
                TrayAction::Stop => self.data.stop(),
                TrayAction::Quit => {
                    self.prepare_quit(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            self.save();
        }
    }

    /// Beskriver systembakkens menu ud fra den aktuelle tilstand.
    /// Tager telefonen, når et opkaldsprogram begynder at bruge mikrofonen, og
    /// lægger på igen, når det stopper (#27). Korte blink ignoreres: mikrofonen
    /// skal være i brug i 2 s og fri i 4 s.
    fn handle_microphone(&mut self, ctx: &egui::Context) {
        let settings = &self.data.settings;
        self.mic.set_wanted(settings.auto_call || self.settings_open, &settings.call_apps);
        let now = Instant::now();
        let gap = now - std::mem::replace(&mut self.last_mic_check, now);
        if self.logged_auto_call != Some(settings.auto_call) {
            self.logged_auto_call = Some(settings.auto_call);
            self.mic_log.write(&format!(
                "KronoLog {}: automatisk telefon {}, programmer: \"{}\"",
                env!("CARGO_PKG_VERSION"),
                if settings.auto_call { "TIL" } else { "FRA" },
                settings.call_apps
            ));
        }
        if !settings.auto_call {
            self.auto_call = AutoCall::Idle;
            self.logged_mic_apps = None;
            return;
        }
        // Har programmet stået stille (minimeret/skjult), kan et opkald være gået forbi.
        if gap > StdDuration::from_secs(3) {
            let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
            self.mic_log.write(&format!(
                "Programmet har ikke tjekket mikrofonen i {:.1} s (vindue {})",
                gap.as_secs_f32(),
                if !self.window_visible { "skjult" } else if minimized { "minimeret" } else { "synligt" }
            ));
        }
        let apps = self.mic.apps().to_vec();
        let busy = crate::mic::matches(&apps, &settings.call_apps);
        if self.logged_mic_apps.as_ref() != Some(&apps) {
            let line = match (apps.is_empty(), busy) {
                (true, _) => "Mikrofonen er fri".to_owned(),
                (false, true) => format!("Mikrofonen bruges af: {} (et opkaldsprogram)", apps.join(", ")),
                (false, false) => format!(
                    "Mikrofonen bruges af: {} (matcher ikke \"{}\", ignoreres)",
                    apps.join(", "),
                    settings.call_apps
                ),
            };
            self.mic_log.write(&line);
            self.logged_mic_apps = Some(apps);
        }
        // Opkaldsprogrammets vinduer, så man kan se, hvad titlerne siger om,
        // hvem man taler med.
        let windows = self.mic.windows().to_vec();
        if windows != self.logged_call_windows {
            if !windows.is_empty() {
                let titles: Vec<String> = windows
                    .iter()
                    .map(|w| format!("\"{}\"{}", w.title, if w.new { " (nyt)" } else { "" }))
                    .collect();
                self.mic_log.write(&format!("Vinduer: {}", titles.join(", ")));
            }
            self.logged_call_windows = windows.clone();
        }
        let in_call = self.data.active.as_ref().is_some_and(|a| a.is_call());
        let current = self.active_description();
        if busy {
            self.mic_free_since = None;
            let since = *self.mic_busy_since.get_or_insert(now);
            if self.auto_call == AutoCall::Idle {
                if since.elapsed() >= StdDuration::from_secs(2) {
                    // Var man allerede i et opkald, styrer man det selv.
                    self.auto_call = if in_call { AutoCall::Manual } else { AutoCall::Started };
                    if in_call {
                        self.mic_log.write("Opkald i gang i forvejen: telefonen røres ikke");
                    } else {
                        self.data.toggle_call();
                        self.save();
                        self.mic_log.write(&format!(
                            "Telefon startet efter {:.1} s (før: {current})",
                            since.elapsed().as_secs_f32()
                        ));
                    }
                } else {
                    ctx.request_repaint_after(StdDuration::from_millis(500));
                }
            }
        } else {
            self.mic_busy_since = None;
            let since = *self.mic_free_since.get_or_insert(now);
            if self.auto_call != AutoCall::Idle {
                if since.elapsed() >= StdDuration::from_secs(4) {
                    // Lagde man selv på undervejs, er der intet at afslutte.
                    if self.auto_call == AutoCall::Started && in_call {
                        self.data.toggle_call();
                        self.save();
                        let after = self.active_description();
                        self.mic_log.write(&format!("Telefon afsluttet; fortsætter: {after}"));
                    } else if self.auto_call == AutoCall::Started {
                        self.mic_log.write("Mikrofonen fri; opkaldet var allerede afsluttet i hånden");
                    }
                    self.auto_call = AutoCall::Idle;
                    self.call_named = false;
                } else {
                    ctx.request_repaint_after(StdDuration::from_millis(500));
                }
            }
        }
        // Navnet fra opkaldsvinduet i noten, hvis den er tom. Vinduet kommer
        // måske lidt efter mikrofonen, så der prøves, indtil der er et navn.
        if self.auto_call == AutoCall::Started && !self.call_named && self.data.settings.call_name
            && let Some(name) = crate::mic::caller_name(&windows)
        {
            self.call_named = true;
            if let Some(active) = self.data.active.as_mut().filter(|a| a.is_call()) {
                if active.description.trim().is_empty() {
                    active.description = name.clone();
                    self.save();
                    self.mic_log.write(&format!("Navn fra opkaldsvinduet skrevet i noten: \"{name}\""));
                } else {
                    self.mic_log.write(&format!("Navn fra opkaldsvinduet: \"{name}\" (noten var udfyldt, røres ikke)"));
                }
            }
        }
    }

    /// Hvad tiden kører på, til loggen.
    fn active_description(&self) -> String {
        match &self.data.active {
            None => "ingen tid kører".to_owned(),
            Some(a) if a.is_call() => "telefon".to_owned(),
            Some(a) if a.is_note(NOTE_PAUSE) => "pause".to_owned(),
            Some(a) => a.project_id.map_or_else(|| "?".to_owned(), |id| self.data.project_label(id)),
        }
    }

    /// Søger efter nye versioner og installerer en hentet opdatering (#26).
    fn handle_updates(&mut self, ctx: &egui::Context) {
        let Some(installer) = self.updater.poll(ctx, self.data.settings.check_updates) else { return };
        // Tiden kører videre efter opdateringen (se `Data::load`).
        self.data.updating = true;
        if self.window_visible
            && let Some(rect) = ctx.input(|i| i.viewport().inner_rect)
        {
            self.data.window_pos = Some([rect.min.x, rect.min.y]);
        }
        self.save();
        match crate::updater::run_installer(&installer) {
            Ok(()) => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => {
                self.data.updating = false;
                self.save();
                self.updater.status = crate::updater::Status::Failed(e);
            }
        }
    }

    /// Opdatér nu (installeret på Windows) eller åbn udgivelsessiden.
    fn start_update(&mut self, ctx: &egui::Context) {
        let Some(release) = self.updater.available().cloned() else { return };
        if crate::updater::can_install(&release) {
            self.updater.download(ctx);
        } else {
            crate::updater::open_page(&release.page);
        }
    }

    /// Banneret øverst, når der er en ny version.
    fn update_banner(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(release) = self.updater.available().cloned() else { return };
        if self.updater.dismissed {
            return;
        }
        let downloading = matches!(self.updater.status, crate::updater::Status::Downloading(_));
        egui::Panel::top("update").show(ui, |ui| {
            ui.horizontal(|ui| {
                if downloading {
                    ui.spinner();
                    ui.label(tr!("update.downloading", version = release.version));
                    return;
                }
                ui.label(RichText::new(tr!("update.available", version = release.version)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text(tr!("update.later")).clicked() {
                        self.updater.dismissed = true;
                    }
                    let label = if crate::updater::can_install(&release) {
                        tr!("update.install")
                    } else {
                        tr!("update.download")
                    };
                    if ui.button(label).clicked() {
                        self.start_update(ctx);
                    }
                });
            });
        });
    }

    /// Om den blinkende prik (systembakken og bjælken) er tændt lige nu: i lige
    /// sekunder af tidstagningen, så de blinker i samme takt (#21). Uden blink
    /// er den altid tændt. Sekunderne følger tidstagningen, ligesom opdateringen
    /// af vinduet.
    pub(super) fn blink_on(&self) -> bool {
        !self.data.settings.blink || self.data.active.as_ref().is_none_or(|a| a.elapsed().num_seconds() % 2 == 0)
    }

    /// Gemmer kunde- og projektlisten som CSV, så en kollega kan indlæse den.
    fn export_project_list(&mut self) {
        let mut dialog = rfd::FileDialog::new()
            .set_title(tr!("share.export_title"))
            .set_file_name(tr!("share.file_name"))
            .add_filter("CSV", &["csv"]);
        if let Some(dir) = directories::UserDirs::new().and_then(|d| d.download_dir().map(PathBuf::from)) {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else { return };
        let list = self.data.project_list();
        self.list_message = Some(match std::fs::write(&path, crate::project_list::to_csv(&list)) {
            Ok(()) => tr!("share.exported", count = list.len(), path = path.display()),
            Err(e) => tr!("share.err", error = e),
        });
    }

    /// Indlæser en delt liste og tilføjer de kunder og projekter, der mangler.
    fn import_project_list(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(tr!("share.import_title"))
            .add_filter("CSV", &["csv", "txt"])
            .pick_file()
        else {
            return;
        };
        self.list_message = Some(match std::fs::read(&path) {
            Ok(bytes) => {
                let list = crate::project_list::parse_csv(&String::from_utf8_lossy(&bytes));
                let (customers, projects) = self.data.import_project_list(&list);
                self.save();
                tr!("share.imported", customers = customers, projects = projects)
            }
            Err(e) => tr!("share.err", error = e),
        });
    }

    /// "Timebudget…" for et projekt: antal timer (0 fjerner budgettet).
    fn budget_window(&mut self, ctx: &egui::Context) {
        let Some((project_id, mut hours)) = self.budget_dialog else { return };
        let (mut save, mut close) = (false, false);
        let used = self.data.project_total(project_id);
        let modal = egui::Modal::new(egui::Id::unique("budget_dialog")).show(ctx, |ui| {
            ui.set_width(260.0);
            ui.heading(tr!("budget.title"));
            ui.label(RichText::new(self.data.project_label(project_id)).strong());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(tr!("budget.hours"));
                ui.add(egui::DragValue::new(&mut hours).range(0.0..=10000.0).speed(0.5).fixed_decimals(1).suffix(" t"));
            });
            ui.label(RichText::new(tr!("budget.used_so_far", used = format_hours_minutes(used))).weak().small());
            ui.label(RichText::new(tr!("budget.hint")).weak().small());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                save |= ui.button(tr!("common.save")).clicked();
                close |= ui.button(tr!("common.cancel")).clicked();
            });
            save |= ui.input(|i| i.key_pressed(egui::Key::Enter));
        });
        close |= modal.should_close();
        if save {
            let minutes = (hours * 60.0).round() as u32;
            self.data.set_budget(project_id, Some(minutes));
            self.save();
            self.budget_dialog = None;
        } else if close {
            self.budget_dialog = None;
        } else {
            self.budget_dialog = Some((project_id, hours));
        }
    }

    /// "Gendan sikkerhedskopien fra …?" Den nuværende fil gemmes først, så
    /// gendannelsen kan fortrydes.
    fn restore_dialog(&mut self, ctx: &egui::Context) {
        let Some(backup) = self.restore_prompt.clone() else { return };
        let mut restore = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("restore_backup")).show(ctx, |ui| {
            ui.set_width(270.0);
            ui.heading(tr!("backup.confirm_title"));
            ui.label(tr!("backup.confirm", date = backup.date.format("%d-%m-%Y")));
            ui.label(RichText::new(tr!("backup.confirm_safe")).weak().small());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                restore |= ui.button(tr!("backup.restore_button")).clicked();
                cancel |= ui.button(tr!("common.cancel")).clicked();
            });
        });
        if modal.should_close() {
            cancel = true;
        }
        if restore {
            self.restore_prompt = None;
            let result = crate::backup::save_before_restore(&self.path).and_then(|_| Data::load(&backup.path));
            match result {
                Ok(data) => {
                    self.data = data;
                    crate::i18n::apply(self.data.settings.language);
                    ctx.set_theme(self.data.theme.to_egui());
                    self.save();
                }
                Err(e) => self.error = Some(tr!("backup.err_restore", error = e)),
            }
        } else if cancel {
            self.restore_prompt = None;
        }
    }

    /// Tiden kører på et projekt eller et opkald (ikke pause).
    fn recording(&self) -> bool {
        self.data.active.as_ref().is_some_and(|a| !a.is_note(NOTE_PAUSE))
    }

    /// Vinduesikonet (proceslinjen) får en fast rød prik, mens tiden kører (#21).
    /// Det blinker ikke; det ville være for uroligt på proceslinjen.
    fn update_window_icon(&mut self, ctx: &egui::Context) {
        let dot = self.recording();
        if dot == self.window_icon_dot {
            return;
        }
        self.window_icon_dot = dot;
        let icon = crate::tray::app_icon();
        let icon = if dot { crate::tray::with_recording_dot(&icon) } else { icon };
        ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(icon))));
    }

    fn update_tray(&mut self) {
        if self.tray.is_none() {
            return;
        }
        let active = self.data.active.as_ref();
        let active_project = active.and_then(|a| a.project_id);
        let status = match active {
            None => tr!("common.no_timer"),
            Some(a) => {
                let what = match a.project_id {
                    Some(id) => self.data.project_label(id),
                    None => tr!("common.phone_call"),
                };
                let what = match &a.note {
                    Some(note) if a.project_id.is_some() => format!("{what} ({})", note_label(note)),
                    _ => what,
                };
                format!("▶ {what} · {}", format_duration(a.elapsed()))
            }
        };
        let on_pause = active.is_some_and(|a| a.is_note(NOTE_PAUSE));
        let on_call = active.is_some_and(|a| a.is_call());
        let dot = self.recording() && self.blink_on();
        let resume = self.data.resume_project().map(|id| self.data.project_label(id));
        let pause_label = match (on_pause, &resume) {
            (false, _) => tr!("timer.break"),
            (true, Some(label)) => tr!("tray.resume_on", project = label),
            (true, None) => tr!("timer.end_break"),
        };
        let call_label = match (on_call, &resume) {
            (false, _) => tr!("timer.phone"),
            (true, Some(label)) => tr!("tray.end_call_resume", project = label),
            (true, None) => tr!("tray.end_call"),
        };
        let customers = self
            .data
            .visible_customers()
            .map(|c| {
                let projects = self
                    .data
                    .customer_projects(c.id)
                    .into_iter()
                    .map(|p| TrayProject { id: p.id, name: p.name.clone(), active: active_project == Some(p.id) })
                    .collect::<Vec<_>>();
                (c.name.clone(), projects)
            })
            .filter(|(_, projects)| !projects.is_empty())
            .collect();
        let state = TrayState {
            status,
            running: active.is_some(),
            call_label,
            pause_label,
            window_visible: self.window_visible,
            dot,
            language: rust_i18n::locale().to_string(),
            customers,
        };
        if let Some(tray) = &mut self.tray {
            tray.update(state);
        }
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(6.0);
        ui.heading(tr!("settings.title"));
        ui.add_space(10.0);

        ui.label(RichText::new(tr!("settings.language")).strong());
        let mut language = self.data.settings.language;
        egui::ComboBox::from_id_salt("language")
            .width(ui.available_width() - 8.0)
            .selected_text(language.label())
            .show_ui(ui, |ui| {
                for option in crate::i18n::Language::ALL {
                    ui.selectable_value(&mut language, option, option.label());
                }
            });
        if language != self.data.settings.language {
            self.data.settings.language = language;
            crate::i18n::apply(language);
            self.data.localize_internal_names();
            self.save();
        }
        settings_section(ui, tr!("settings.theme"));
        let mut theme = self.data.theme;
        ui.radio_value(&mut theme, ThemeChoice::System, tr!("settings.theme_system"));
        ui.radio_value(&mut theme, ThemeChoice::Light, tr!("settings.theme_light"));
        ui.radio_value(&mut theme, ThemeChoice::Dark, tr!("settings.theme_dark"));
        if theme != self.data.theme {
            self.data.theme = theme;
            ctx.set_theme(theme.to_egui());
            self.save();
        }
        // Lodret bjælke til siden af skærmen (#19).
        settings_section(ui, tr!("settings.bar"));
        let mut vertical = self.data.bar_vertical;
        ui.radio_value(&mut vertical, false, tr!("settings.bar_horizontal"));
        ui.radio_value(&mut vertical, true, tr!("settings.bar_vertical"));
        if vertical != self.data.bar_vertical {
            self.data.bar_vertical = vertical;
            self.save();
        }
        if ui.checkbox(&mut self.data.settings.blink, tr!("settings.blink")).changed() {
            self.save();
        }
        settings_section(ui, tr!("settings.start"));
        let mut autostart = self.autostart;
        if ui.checkbox(&mut autostart, tr!("settings.autostart")).changed() {
            if let Err(e) = crate::autostart::set_enabled(autostart) {
                self.error = Some(e);
            }
            self.autostart = crate::autostart::is_enabled();
        }
        settings_section(ui, tr!("settings.new_customers"));
        ui.label(
            RichText::new(tr!("settings.new_customers_hint"))
                .weak()
                .small(),
        );
        let mut template = self.data.settings.customer_template.clone();
        let mut remove = None;
        for (i, project) in template.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                // Navnet til venstre, Auto start og × til højre.
                let name_width = (ui.available_width() - 118.0).max(40.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(name_width, 18.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| ui.add(egui::Label::new(&project.name).truncate()),
                );
                ui.checkbox(&mut project.auto_start, tr!("settings.auto_start"));
                if ui.small_button("×").on_hover_text(tr!("settings.remove_template")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            template.remove(i);
        }
        // Kun ét projekt kan starte automatisk: det senest markerede vinder.
        let old = &self.data.settings.customer_template;
        if let Some(newly) = template
            .iter()
            .position(|p| p.auto_start && !old.iter().any(|o| o.name == p.name && o.auto_start))
        {
            template.iter_mut().enumerate().for_each(|(i, p)| p.auto_start = i == newly);
        }
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.new_template_project)
                    .hint_text(tr!("list.new_project"))
                    .desired_width(ui.available_width() - 30.0),
            );
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let name = self.new_template_project.trim().to_owned();
            if (ui.button("+").clicked() || enter) && !name.is_empty() && !template.iter().any(|p| p.name == name) {
                template.push(TemplateProject { name, auto_start: false });
                self.new_template_project.clear();
            }
        });
        if template != self.data.settings.customer_template {
            self.data.settings.customer_template = template;
            self.save();
        }
        settings_section(ui, tr!("settings.break_reminder"));
        let mut settings = self.data.settings.clone();
        ui.checkbox(&mut settings.break_reminder, tr!("settings.break_reminder_check"));
        ui.add_enabled_ui(settings.break_reminder, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr!("settings.after"));
                ui.add(egui::Slider::new(&mut settings.break_minutes, 15..=180).step_by(5.0).suffix(" min"));
            });
        });
        ui.label(
            RichText::new(tr!("settings.break_reminder_hint"))
                .weak()
                .small(),
        );
        if settings != self.data.settings {
            self.data.settings = settings;
            self.save();
        }
        settings_section(ui, tr!("breathing.window_title"));
        let mut settings = self.data.settings.clone();
        let mut remove = None;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("breathing_exercise")
                // Panelet kan være meget smalt i første billede; aldrig negativ bredde.
                .width((ui.available_width() - 36.0).max(40.0))
                .selected_text(settings.breathing_exercise.label())
                .show_ui(ui, |ui| {
                    for exercise in Exercise::all(&settings.custom_exercises) {
                        ui.horizontal(|ui| {
                            // Egne øvelser kan slettes igen (#30).
                            if let Exercise::Custom(custom) = &exercise
                                && ui.small_button("×").on_hover_text(tr!("breathing.delete")).clicked()
                            {
                                remove = Some(custom.clone());
                            }
                            let label = exercise.label();
                            ui.selectable_value(&mut settings.breathing_exercise, exercise, label);
                        });
                    }
                });
            if ui.button("+").on_hover_text(tr!("breathing.new_hint")).clicked() {
                self.new_exercise = Some(Default::default());
            }
        });
        if let Some(custom) = remove {
            settings.custom_exercises.retain(|c| *c != custom);
            if settings.breathing_exercise == Exercise::Custom(custom) {
                settings.breathing_exercise = Exercise::default();
            }
        }
        // Pointene (#29).
        ui.label(RichText::new(self.breathing_stats().summary()).weak().small());
        ui.horizontal(|ui| {
            ui.label(tr!("settings.length"));
            ui.radio_value(&mut settings.breathing_minutes, 1, "1 min");
            ui.radio_value(&mut settings.breathing_minutes, 2, "2 min");
        });
        ui.checkbox(&mut settings.breathing_auto, tr!("settings.breathing_auto"));
        if ui.add_enabled(self.breathing.is_none(), egui::Button::new(tr!("settings.try_now"))).clicked() {
            self.start_breathing();
        }
        if settings != self.data.settings {
            self.data.settings = settings;
            self.save();
        }
        settings_section(ui, tr!("settings.away"));
        let mut settings = self.data.settings.clone();
        ui.checkbox(&mut settings.away_enabled, tr!("settings.away_check"));
        ui.add_enabled_ui(settings.away_enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr!("settings.after"));
                ui.add(egui::Slider::new(&mut settings.away_minutes, 5..=60).step_by(5.0).suffix(" min"));
            });
        });
        ui.label(
            RichText::new(tr!("settings.away_hint"))
                .weak()
                .small(),
        );
        // "Glemt at starte tiden?"
        ui.checkbox(&mut settings.forgot_reminder, tr!("settings.forgot_check"));
        ui.add_enabled_ui(settings.forgot_reminder, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr!("settings.after"));
                ui.add(egui::Slider::new(&mut settings.forgot_minutes, 5..=60).step_by(5.0).suffix(" min"));
            });
        });
        settings_section(ui, tr!("settings.shortcuts"));
        if ui.button(tr!("settings.shortcuts_button")).clicked() {
            self.shortcuts_dialog = Some(ShortcutsDialog::default());
        }

        // Telefonen automatisk (#27).
        settings_section(ui, tr!("settings.calls"));
        ui.checkbox(&mut settings.auto_call, tr!("settings.auto_call"));
        ui.add_enabled_ui(settings.auto_call, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr!("settings.call_apps"));
                // Tekstfeltet lægger sin egen margen oveni; ellers blev rækken (og hele panelet) for bred.
                ui.add(egui::TextEdit::singleline(&mut settings.call_apps).desired_width((ui.available_width() - 50.0).max(40.0)));
                // Vælg blandt programmer, der har brugt mikrofonen, så navnet er rigtigt (#28).
                ui.menu_button("+", |ui| {
                    let known = self.mic.known_apps();
                    if known.is_empty() {
                        ui.label(RichText::new(tr!("settings.no_mic_apps")).weak());
                    }
                    for app in known {
                        if ui.button(app).clicked() {
                            settings.call_apps = crate::mic::add_to_list(&settings.call_apps, app);
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(tr!("settings.pick_mic_app"));
            });
        });
        ui.add_enabled_ui(settings.auto_call, |ui| {
            ui.checkbox(&mut settings.call_name, tr!("settings.call_name"))
                .on_hover_text(tr!("settings.call_name_hint"));
        });
        let apps = self.mic.apps().join(", ");
        let using = if apps.is_empty() { tr!("settings.mic_free") } else { tr!("settings.mic_used_by", apps = apps) };
        ui.label(RichText::new(format!("{}\n{using}", tr!("settings.call_apps_hint"))).weak().small());
        // Loggen til fejlsøgning (#33).
        if ui.small_button(tr!("settings.open_mic_log")).on_hover_text(tr!("settings.open_mic_log_hint")).clicked()
            && let Some(folder) = self.mic_log.folder()
        {
            crate::updater::open_page(&folder.to_string_lossy());
        }

        if self.tray.is_some() {
            settings_section(ui, tr!("settings.tray"));
            ui.checkbox(&mut settings.close_to_tray, tr!("settings.close_to_tray"));
            ui.label(
                RichText::new(tr!("settings.tray_hint"))
                    .weak()
                    .small(),
            );
        }

        if settings != self.data.settings {
            self.data.settings = settings;
            self.save();
        }

        // Dagsmål pr. ugedag: den grønne ring i uret.
        settings_section(ui, tr!("goal.title"));
        if ui.checkbox(&mut self.data.settings.daily_goal, tr!("goal.enable")).changed() {
            self.save();
        }
        if self.data.settings.daily_goal {
            let day_names = tr!("week.days");
            let day_names: Vec<&str> = day_names.split(',').collect();
            let mut goals = self.data.settings.goal_minutes;
            let mut changed = false;
            // Ma–To på én række og Fr–Sø på den næste, så det passer i det smalle panel
            // (bredere indhold ville gøre hele panelet bredere og klippe andre tekster).
            ui.spacing_mut().interact_size.x = 38.0;
            for days in [0..4, 4..7] {
                egui::Grid::new(("goal_days", days.start)).spacing([6.0, 2.0]).min_col_width(38.0).show(ui, |ui| {
                    for day in days.clone() {
                        ui.label(RichText::new(day_names.get(day).copied().unwrap_or("")).small().weak());
                    }
                    ui.end_row();
                    for day in days.clone() {
                        let drag = egui::DragValue::new(&mut goals[day])
                            .range(0..=16 * 60)
                            .speed(5.0)
                            .custom_formatter(|m, _| format!("{}:{:02}", m as u32 / 60, m as u32 % 60))
                            .custom_parser(|text| {
                                let (h, m) = text.trim().split_once([':', '.']).unwrap_or((text.trim(), "0"));
                                Some(h.parse::<f64>().ok()? * 60.0 + m.parse::<f64>().ok()?)
                            });
                        changed |= ui.add(drag).changed();
                    }
                    ui.end_row();
                });
            }
            ui.label(RichText::new(tr!("goal.days_hint")).weak().small());
            if changed {
                self.data.settings.goal_minutes = goals;
                self.save();
            }
        }

        // Del kunde- og projektlisten med kolleger.
        settings_section(ui, tr!("share.title"));
        ui.label(RichText::new(tr!("share.hint")).weak().small());
        ui.horizontal(|ui| {
            if ui.button(tr!("share.export")).clicked() {
                self.export_project_list();
            }
            if ui.button(tr!("share.import")).clicked() {
                self.import_project_list();
            }
        });
        if let Some(message) = &self.list_message {
            ui.label(RichText::new(message).small());
        }

        // Daglige sikkerhedskopier.
        settings_section(ui, tr!("backup.title"));
        ui.label(RichText::new(tr!("backup.hint", days = crate::backup::KEEP)).weak().small());
        let backups = crate::backup::list(&self.path);
        // Hver for sig: på én række blev de bredere end panelet, så alt indhold
        // blev bredere og tekster i panelet blev klippet.
        egui::ComboBox::from_id_salt("restore_backup")
            .selected_text(tr!("backup.restore"))
            .width((ui.available_width() - 8.0).max(60.0))
            .show_ui(ui, |ui| {
                if backups.is_empty() {
                    ui.label(RichText::new(tr!("backup.none")).weak());
                }
                for backup in &backups {
                    if ui.button(backup.date.format("%d-%m-%Y").to_string()).clicked() {
                        self.restore_prompt = Some(backup.clone());
                    }
                }
            });
        if ui.button(tr!("backup.open_folder")).clicked() {
            let folder = crate::backup::folder(&self.path);
            let _ = std::fs::create_dir_all(&folder);
            crate::updater::open_page(&folder.to_string_lossy());
        }

        settings_section(ui, tr!("settings.help"));
        if ui.button(format!("? {}", tr!("help.open"))).clicked() {
            self.open_help(None);
        }
        if ui.checkbox(&mut self.data.settings.show_tips, tr!("settings.show_tips")).changed() {
            self.save();
        }

        settings_section(ui, tr!("settings.updates"));
        if ui.checkbox(&mut self.data.settings.check_updates, tr!("settings.check_updates")).changed() {
            self.save();
        }
        // Ombrydes, så status kan stå under knappen i det smalle panel.
        ui.horizontal_wrapped(|ui| {
            use crate::updater::Status;
            let checking = matches!(self.updater.status, Status::Checking | Status::Downloading(_));
            if ui.add_enabled(!checking, egui::Button::new(tr!("settings.check_now"))).clicked() {
                self.updater.check(ctx, true);
            }
            match self.updater.status.clone() {
                Status::Idle => {}
                Status::Checking => {
                    ui.spinner();
                }
                Status::UpToDate => {
                    ui.label(RichText::new(tr!("update.up_to_date")).weak());
                }
                Status::Available(release) => {
                    let label = if crate::updater::can_install(&release) {
                        tr!("update.install_version", version = release.version)
                    } else {
                        tr!("update.download_version", version = release.version)
                    };
                    if ui.button(label).clicked() {
                        self.start_update(ctx);
                    }
                }
                Status::Downloading(release) => {
                    ui.spinner();
                    ui.label(tr!("update.downloading", version = release.version));
                }
                Status::Failed(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, tr!("update.failed"))
                        .on_hover_text(error);
                }
            }
        });

        if ui.button(tr!("changelog.open")).clicked() {
            self.open_changelog();
        }

        ui.add_space(20.0);
        ui.label(RichText::new(format!("KronoLog {}", env!("CARGO_PKG_VERSION"))).weak().small());
        ui.add_space(4.0);
    }

    fn handle_idle_events(&mut self) {
        for event in self.idle.poll() {
            match event {
                IdleEvent::Away { since } => {
                    if self.away_since.is_none_or(|s| since < s) {
                        self.away_since = Some(since);
                    }
                }
                IdleEvent::Back { at } => {
                    if let Some(since) = self.away_since.take() {
                        self.break_reminder.note_away(since, at);
                        self.consider_away(since, at);
                    }
                }
                IdleEvent::Slept { from, to } => {
                    // Dvale: stop tiden dér, hvor computeren gik i dvale, og spørg om
                    // der skal fortsættes. Et samtidigt "væk"-spørgsmål er overflødigt.
                    self.break_reminder.note_away(from, to);
                    self.away_since = None;
                    self.away_prompt = None;
                    self.away_handled_until = self.away_handled_until.max(to);
                    if let Some(snapshot) = self.data.suspend(from) {
                        self.save();
                        // Et opkald uden projekt ender som ventende opkald i stedet.
                        if snapshot.project_id.is_some() {
                            self.sleep_prompt = Some(SleepPrompt { snapshot, from, to });
                        }
                    }
                }
            }
        }
    }

    /// Spørg om pause, hvis brugeren har været væk længe nok, mens der kørte
    /// tid på et projekt (ikke under en pause eller et opkald uden projekt).
    fn consider_away(&mut self, since: DateTime<Local>, at: DateTime<Local>) {
        let settings = &self.data.settings;
        let Some(active) = &self.data.active else { return };
        let Some(project_id) = active.project_id else { return };
        if !settings.away_enabled || active.is_note(NOTE_PAUSE) {
            return;
        }
        let from = since.max(active.start).max(self.away_handled_until);
        if at - from < Duration::minutes(i64::from(settings.away_minutes)) {
            return;
        }
        match &mut self.away_prompt {
            // Flere meldinger om samme fravær (f.eks. både inaktivitet og dvale) slås sammen.
            Some(prompt) if prompt.project_id == project_id => {
                prompt.from = prompt.from.min(from);
                prompt.to = prompt.to.max(at);
            }
            _ => self.away_prompt = Some(AwayPrompt { project_id, from, to: at }),
        }
    }

    /// Efter dvale: tiden blev stoppet, da computeren gik i dvale. Enter
    /// fortsætter på samme projekt (uret tæller videre), Esc lader den være stoppet.
    fn sleep_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &self.sleep_prompt else { return };
        let label = match prompt.snapshot.project_id {
            Some(id) => self.data.project_label(id),
            None => return,
        };
        let label = match &prompt.snapshot.note {
            Some(note) => format!("{label} ({})", note_label(note)),
            None => label,
        };
        let (from, to) = (prompt.from, prompt.to);
        let mut answer = None;

        let modal = egui::Modal::new(egui::Id::unique("sleep_dialog")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 40.0).clamp(240.0, 300.0));
            ui.heading(tr!("sleep.title"));
            ui.add_space(4.0);
            ui.label(tr!("sleep.period", from = from.format("%H:%M"), to = to.format("%H:%M")));
            ui.label(tr!("sleep.stopped", time = from.format("%H:%M")));
            ui.add_space(4.0);
            ui.label(RichText::new(tr!("sleep.resume_question", project = label)).strong());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("timer.resume")).on_hover_text("Enter").clicked() {
                    answer = Some(true);
                }
                if ui.button(tr!("sleep.no_thanks")).on_hover_text("Esc").clicked() {
                    answer = Some(false);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                answer = Some(true);
            }
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(false);
        }
        if let Some(resume) = answer
            && let Some(prompt) = self.sleep_prompt.take()
            && resume
        {
            self.data.resume(prompt.snapshot);
            self.save();
        }
    }

    /// "Velkommen tilbage"-dialogen. Enter registrerer pause, Esc beholder tiden på projektet.
    fn away_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &self.away_prompt else { return };
        let (project_id, from, to) = (prompt.project_id, prompt.from, prompt.to);
        let length = format_length(to - from);
        let mut answer = None;

        let modal = egui::Modal::new(egui::Id::unique("away_dialog")).show(ctx, |ui| {
            ui.set_width(270.0);
            ui.heading(tr!("away.title"));
            ui.add_space(4.0);
            ui.label(tr!("away.period", from = from.format("%H:%M"), to = to.format("%H:%M"), length = length));
            ui.label(tr!("away.meanwhile", project = self.data.project_label(project_id)));
            ui.add_space(4.0);
            ui.label(RichText::new(tr!("away.question")).strong());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("away.yes")).on_hover_text("Enter").clicked() {
                    answer = Some(true);
                }
                if ui.button(tr!("away.no")).on_hover_text("Esc").clicked() {
                    answer = Some(false);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                answer = Some(true);
            }
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(false);
        }

        if let Some(as_pause) = answer {
            if as_pause && self.data.convert_to_pause(project_id, from, to) {
                self.save();
            }
            self.away_handled_until = self.away_handled_until.max(to);
            self.away_prompt = None;
        }
    }
}

/// To tekstfelter til ny/eksisterende kunde og projekt. At skrive i dem fravælger
/// et valgt eksisterende projekt.
/// 🗕 i topbjælken. Folder man sammen til den lodrette bjælke, står stregen
/// lodret ude i højre side i stedet for vandret i bunden.
fn fold_button(ui: &mut egui::Ui, vertical: bool) -> egui::Response {
    if !vertical {
        return ui.button("🗕");
    }
    // Samme størrelse som 🗕-knappen; tegnet er usynligt, stregen tegnes selv.
    let response = ui.button(RichText::new("🗕").color(Color32::TRANSPARENT));
    let color = ui.style().interact(&response).fg_stroke.color;
    let r = response.rect.shrink2(Vec2::new(6.0, 5.0));
    ui.painter().line_segment([r.right_top(), r.right_bottom()], Stroke::new(2.0, color));
    response
}

/// Overskrift for en sektion i indstillingerne, med en streg over, så
/// sektionerne er nemme at skelne fra hinanden (#15).
fn settings_section(ui: &mut egui::Ui, title: String) {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(RichText::new(title).strong());
}

fn typed_target_fields(ui: &mut egui::Ui, target: &mut CallTarget) {
    let mut changed = false;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0 - 4.0;
        changed |= ui
            .add(egui::TextEdit::singleline(&mut target.customer).hint_text(tr!("common.customer")).desired_width(width))
            .changed();
        changed |= ui
            .add(egui::TextEdit::singleline(&mut target.project).hint_text(tr!("common.project")).desired_width(width))
            .changed();
    });
    if changed {
        target.project_id = None;
    }
}

fn search_field(ui: &mut egui::Ui, query: &mut String) -> egui::Response {
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(query)
                .hint_text(tr!("common.search_hint"))
                .desired_width(ui.available_width() - 30.0),
        );
        if ui.add_enabled(!query.is_empty(), egui::Button::new("×")).clicked() {
            query.clear();
        }
        response
    })
    .inner
}

/// Kundens projekter der matcher søgningen (uden hensyn til store/små bogstaver).
/// Matcher kundens navn, kommer alle projekterne med. `None` betyder at kunden
/// skal skjules.
fn matching_projects<'a>(data: &'a Data, customer: &Customer, query: &str) -> Option<Vec<&'a Project>> {
    let query = query.trim().to_lowercase();
    let projects = data.customer_projects(customer.id);
    if query.is_empty() || customer.name.to_lowercase().contains(&query) {
        return Some(projects);
    }
    let hits: Vec<_> = projects.into_iter().filter(|p| p.name.to_lowercase().contains(&query)).collect();
    (!hits.is_empty()).then_some(hits)
}

/// Forkorter en tekst til højst `max` tegn med "…" til sidst.
/// Brugt tid i procent af budgettet.
fn budget_percent(used: Duration, budget: Duration) -> i64 {
    (used.num_minutes() * 100).checked_div(budget.num_minutes()).unwrap_or(0)
}

/// Svag under 90 %, gul fra 90 % og rød over budgettet.
fn budget_color(ui: &egui::Ui, percent: i64) -> Color32 {
    match percent {
        ..90 => ui.visuals().weak_text_color(),
        90..=100 => ui.visuals().warn_fg_color,
        _ => ui.visuals().error_fg_color,
    }
}

fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Dagens forskellige noter på et projekt, i den rækkefølge de blev skrevet.
fn day_notes(entries: &[DayEntry], project_id: Id) -> Vec<String> {
    let mut notes: Vec<String> = Vec::new();
    for e in entries.iter().filter(|e| e.entry.project_id == project_id) {
        let note = e.entry.description.trim();
        if !note.is_empty() && !notes.iter().any(|n| n == note) {
            notes.push(note.to_owned());
        }
    }
    notes
}

fn monitor_rect(monitor: &winit::monitor::MonitorHandle) -> egui::Rect {
    let (pos, size) = (monitor.position(), monitor.size());
    egui::Rect::from_min_size(
        Pos2::new(pos.x as f32, pos.y as f32),
        Vec2::new(size.width as f32, size.height as f32),
    )
}

/// Om vinduets øverste del (hvor titellinjen er) kan ses og gribes på en skærm:
/// mindst 80 × 20 pixels af den skal ligge på samme skærm.
fn title_bar_visible(window: egui::Rect, monitors: &[egui::Rect]) -> bool {
    let top = egui::Rect::from_min_size(window.min, Vec2::new(window.width(), window.height().min(40.0)));
    monitors.iter().any(|m| {
        let overlap = m.intersect(top);
        overlap.width() >= 80.0_f32.min(window.width()) && overlap.height() >= 20.0
    })
}

/// Lille knap med to modsatrettede pile (⇄), tegnet selv så de står præcis i
/// midten; tegnet fra emoji-skrifttypen sidder forskudt i højden.
fn move_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(24.0, 18.0), egui::Sense::click());
    let visuals = ui.style().interact(&response);
    let painter = ui.painter();
    painter.rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
    let stroke = Stroke::new(1.5, visuals.fg_stroke.color);
    let (c, half, gap, head) = (rect.center(), 5.5, 2.5, 3.0);
    // Øverst en pil mod højre, nederst en mod venstre.
    for (y, dir) in [(c.y - gap, 1.0), (c.y + gap, -1.0)] {
        let (from, to) = (Pos2::new(c.x - half * dir, y), Pos2::new(c.x + half * dir, y));
        painter.line_segment([from, to], stroke);
        painter.line_segment([to, to + Vec2::new(-head * dir, -head)], stroke);
        painter.line_segment([to, to + Vec2::new(-head * dir, head)], stroke);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    #[test]
    fn title_bar_must_be_on_a_monitor() {
        // To skærme over hinanden, 2560 × 1440.
        let monitors = [rect(0.0, 0.0, 2560.0, 1440.0), rect(0.0, 1440.0, 2560.0, 1440.0)];
        let window = |x, y| rect(x, y, 340.0, 560.0);

        assert!(title_bar_visible(window(100.0, 100.0), &monitors));
        assert!(title_bar_visible(window(100.0, 2000.0), &monitors), "på den nederste skærm");
        assert!(title_bar_visible(window(2400.0, 100.0), &monitors), "delvist udenfor, men kan gribes");
        assert!(!title_bar_visible(window(3000.0, 100.0), &monitors), "til højre for alle skærme");
        assert!(!title_bar_visible(window(100.0, 2870.0), &monitors), "kun 10 px af toppen synlig");
        assert!(!title_bar_visible(window(100.0, -500.0), &monitors), "titellinjen over skærmen");

        // Kun den nederste skærm tilbage: et vindue fra den øverste er nu væk.
        assert!(!title_bar_visible(window(100.0, 100.0), &monitors[1..]));
    }
}
