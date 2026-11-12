//! Datamodel og lagring. Alt gemmes i én JSON-fil i brugerens datamappe.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};

pub type Id = u64;

pub const NOTE_PHONE: &str = "Telefon";
pub const NOTE_PAUSE: &str = "Pause";

/// Et udsnit af registreringerne: én kunde (alle dens projekter) eller ét projekt.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Scope {
    Customer(Id),
    Project(Id),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Customer {
    pub id: Id,
    pub name: String,
    /// Slettede kunder skjules, men beholdes så historikken stadig kan vise navnet.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: Id,
    pub customer_id: Id,
    pub name: String,
    /// Slettede projekter skjules, men beholdes så historikken stadig kan vise navnet.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    /// Står øverst under ⭐ Favoritter.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    /// Timebudget i minutter; der advares, når tiden nærmer sig.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_minutes: Option<u32>,
}

/// En afsluttet tidsregistrering.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct TimeEntry {
    pub project_id: Id,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    /// Typen, f.eks. "Telefon" eller "Pause".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Brugerens egen note om hvad tiden er brugt på.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl TimeEntry {
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// En registrering på en bestemt dag. `index` peger ind i `Data.entries`;
/// `None` er den kørende timer, som ikke kan rettes.
pub struct DayEntry {
    pub index: Option<usize>,
    pub entry: TimeEntry,
}

/// Hvor et telefonopkald skal registreres: enten et eksisterende projekt eller
/// en kunde og et projekt skrevet som tekst (oprettes hvis de ikke findes).
#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct CallTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Id>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub customer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project: String,
}

impl CallTarget {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn typed(&self) -> Option<(&str, &str)> {
        let (customer, project) = (self.customer.trim(), self.project.trim());
        (!customer.is_empty() && !project.is_empty()).then_some((customer, project))
    }
}

/// Et telefonopkald der er afsluttet, men endnu ikke registreret på et projekt.
#[derive(Serialize, Deserialize, Clone)]
pub struct UnassignedEntry {
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    #[serde(default, skip_serializing_if = "CallTarget::is_empty")]
    pub target: CallTarget,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl UnassignedEntry {
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// Den timer der kører lige nu. `last_seen` opdateres løbende, så en timer der
/// efterlades af et nedbrud kan afsluttes på et fornuftigt tidspunkt.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ActiveTimer {
    /// `None` er et telefonopkald, der endnu ikke hører til et projekt.
    pub project_id: Option<Id>,
    pub start: DateTime<Local>,
    pub last_seen: DateTime<Local>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Kan udfyldes mens et opkald kører, så det registreres direkte når det slutter.
    #[serde(default, skip_serializing_if = "CallTarget::is_empty")]
    pub target: CallTarget,
    /// Under en pause eller et opkald: tidstagningen der kørte før, som der
    /// fortsættes på, når pausen/opkaldet afsluttes med samme knap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Box<ActiveTimer>>,
    /// Tid fra før en pause, som uret tæller videre fra. Kun til visning;
    /// registreringerne gemmes stadig hver for sig.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub carried_secs: i64,
    /// Note der skrives mens tiden kører; følger med registreringen.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Kopi af en kørende tidstagning med uret frosset (`carried_secs` = vist tid),
/// så den kan genoptages efter en pause eller et opkald.
fn frozen(active: &ActiveTimer) -> Box<ActiveTimer> {
    Box::new(ActiveTimer { carried_secs: active.elapsed().num_seconds(), ..active.clone() })
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

impl ActiveTimer {
    pub fn is_note(&self, note: &str) -> bool {
        self.note.as_deref() == Some(note)
    }

    /// Et telefonopkald, der endnu ikke hører til et projekt.
    pub fn is_call(&self) -> bool {
        self.project_id.is_none()
    }

    /// Tiden uret viser: denne tidstagning plus evt. tid fra før en pause.
    pub fn elapsed(&self) -> Duration {
        Local::now() - self.start + Duration::seconds(self.carried_secs)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub fn to_egui(self) -> eframe::egui::ThemePreference {
        use eframe::egui::ThemePreference;
        match self {
            Self::System => ThemePreference::System,
            Self::Light => ThemePreference::Light,
            Self::Dark => ThemePreference::Dark,
        }
    }

    /// Temaet til vinduets titellinje.
    pub fn to_window_theme(self) -> eframe::egui::SystemTheme {
        use eframe::egui::SystemTheme;
        match self {
            Self::System => SystemTheme::SystemDefault,
            Self::Light => SystemTheme::Light,
            Self::Dark => SystemTheme::Dark,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Data {
    next_id: Id,
    pub customers: Vec<Customer>,
    pub projects: Vec<Project>,
    pub entries: Vec<TimeEntry>,
    pub active: Option<ActiveTimer>,
    #[serde(default)]
    pub unassigned: Vec<UnassignedEntry>,
    /// Projektet "Intern tid", som pauser og intern tid registreres på.
    #[serde(default)]
    internal_project: Option<Id>,
    #[serde(default)]
    pub theme: ThemeChoice,
    /// Vinduets indre placering (skærmpunkter) da det sidst blev lukket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_pos: Option<[f32; 2]>,
    /// Projekter hvis afrundede tid en given dag er rundet ned manuelt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub round_down: Vec<RoundDown>,
    /// Projekter hvis tid en given dag er krydset af som overført/faktureret (#16).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub billed: Vec<Billed>,
    #[serde(default)]
    pub settings: Settings,
    /// Vinduet er foldet sammen til en lille bjælke.
    #[serde(default)]
    pub compact: bool,
    /// Bjælken står lodret, f.eks. i siden af skærmen (#19).
    #[serde(default)]
    pub bar_vertical: bool,
    /// Hjælpen er vist (den åbner af sig selv første gang).
    #[serde(default)]
    pub help_seen: bool,
    /// Versionen ved sidste start; er den en anden nu, vises "Hvad er nyt?".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_version: Option<String>,
    /// "Vidste du…?" er skjult resten af denne dag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tips_hidden_on: Option<NaiveDate>,
    /// Gennemførte åndedrætsøvelser pr. dag, til pointene (#29).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub breathing_done: std::collections::BTreeMap<NaiveDate, u32>,
    /// Programmet lukkede for at blive opdateret (#26): en kørende timer skal
    /// køre videre ved næste start i stedet for at blive afsluttet.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub updating: bool,
    /// Vinduets størrelse (skærmpunkter), før det blev foldet sammen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal_size: Option<[f32; 2]>,
    /// Menuen ved siden af uret (dagens tid, projektets samlede tid) er åben.
    #[serde(default)]
    pub day_panel_open: bool,
    /// Menuen viser projektets tid denne uge i stedet for i alt (#38).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub day_panel_week: bool,
}

/// Brugerens indstillinger (menuen ☰).
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Spørg om tiden skal være pause, når man har været væk fra computeren.
    pub away_enabled: bool,
    /// Hvor mange minutter man skal have været væk, før der spørges.
    pub away_minutes: u32,
    /// Lukkeknappen skjuler vinduet i systembakken i stedet for at afslutte.
    pub close_to_tray: bool,
    /// Foreslå en kort pause, når man har arbejdet `break_minutes` uden.
    pub break_reminder: bool,
    pub break_minutes: u32,
    /// Guidet åndedrætsøvelse i pauserne.
    pub breathing_exercise: crate::app::breathing::Exercise,
    /// Øvelsens længde i minutter (1 eller 2).
    pub breathing_minutes: u32,
    /// Start øvelsen af sig selv, når en pause begynder.
    pub breathing_auto: bool,
    /// Egne åndedrætsøvelser (#30).
    pub custom_exercises: Vec<crate::app::breathing::CustomExercise>,
    pub shortcuts: crate::shortcuts::Shortcuts,
    /// Projekter alle nye kunder får (issue #9).
    pub customer_template: Vec<TemplateProject>,
    /// Rækkefølge af kunder og projekter i listerne (issue #2).
    pub sort: SortOrder,
    pub language: crate::i18n::Language,
    /// Søg selv efter nye versioner (#26).
    pub check_updates: bool,
    /// Prikken i bjælken og i systembakken blinker, mens tiden kører.
    pub blink: bool,
    /// Vis dagsmålet (den grønne ring i uret).
    pub daily_goal: bool,
    /// Dagsmål i minutter pr. ugedag (mandag først); 0 = intet mål den dag.
    /// Huskes, når dagsmålet slås fra.
    pub goal_minutes: [u32; 7],
    /// "Glemt at starte tiden?" efter `forgot_minutes` ved computeren uden tid.
    pub forgot_reminder: bool,
    pub forgot_minutes: u32,
    /// "Vidste du…?"-tips nederst i timer-fanen.
    pub show_tips: bool,
    /// Tag telefonen, når et af `call_apps` bruger mikrofonen (#27).
    pub auto_call: bool,
    /// Programmerne, der tæller som opkald, adskilt med komma.
    pub call_apps: String,
}

/// Et projekt i kundeskabelonen.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct TemplateProject {
    pub name: String,
    /// Tiden starter på dette projekt, når en ny kunde oprettes.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_start: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default, Debug)]
pub enum SortOrder {
    /// Alfabetisk (dansk alfabet).
    #[default]
    Name,
    /// Det senest brugte først.
    Recent,
}

impl Settings {
    /// Dagens mål, hvis dagsmålet er slået til og dagen har et.
    pub fn goal_for(&self, date: NaiveDate) -> Option<Duration> {
        let minutes = self.goal_minutes[date.weekday().num_days_from_monday() as usize];
        (self.daily_goal && minutes > 0).then(|| Duration::minutes(minutes as i64))
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            away_enabled: true,
            away_minutes: 15,
            close_to_tray: true,
            break_reminder: false,
            break_minutes: 60,
            breathing_exercise: Default::default(),
            breathing_minutes: 2,
            breathing_auto: false,
            custom_exercises: Vec::new(),
            shortcuts: Default::default(),
            customer_template: Vec::new(),
            sort: SortOrder::Name,
            language: Default::default(),
            check_updates: true,
            blink: true,
            daily_goal: false,
            // 7:30 mandag–torsdag, 7:00 fredag, intet i weekenden.
            goal_minutes: [450, 450, 450, 450, 420, 0, 0],
            forgot_reminder: true,
            forgot_minutes: 15,
            show_tips: true,
            auto_call: false,
            call_apps: "Teams, Zoom".to_owned(),
        }
    }
}

/// Et manuelt valg om at runde et projekts tid en bestemt dag ned i stedet for op.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct RoundDown {
    pub date: NaiveDate,
    pub project_id: Id,
}

/// Et projekts tid en dag, krydset af som overført (f.eks. til fakturering).
/// `minutes` er den afrundede tid, da der blev krydset af.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Billed {
    pub date: NaiveDate,
    pub project_id: Id,
    pub minutes: i64,
}

/// Afkrydsningen af et projekts tid en dag.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BilledState {
    No,
    Yes,
    /// Krydset af, men den afrundede tid er ændret siden (her den gamle tid).
    Changed(Duration),
}

/// Et projekts tid en dag, afrundet til kvarter.
pub struct Rounded {
    pub time: Duration,
    /// Rundet ned manuelt i stedet for op.
    pub down: bool,
    /// Om op og ned giver forskellige tider (ellers er der intet at vælge).
    pub can_toggle: bool,
}

impl Data {
    /// Standardplacering: ~/.local/share/kronolog/data.json på Linux,
    /// %APPDATA%\KronoLog\data\data.json på Windows.
    pub fn default_path() -> PathBuf {
        Self::path_for("KronoLog")
    }

    /// Datafilens placering for et givet programnavn.
    pub fn path_for(app_name: &str) -> PathBuf {
        directories::ProjectDirs::from("", "", app_name)
            .map(|d| d.data_dir().join("data.json"))
            .unwrap_or_else(|| PathBuf::from(format!("{}-data.json", app_name.to_lowercase())))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let mut data: Data = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| tr!("data.err_read", path = path.display(), error = e))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(tr!("data.err_open", path = path.display(), error = e)),
        };
        // En timer der stadig "kører" i filen betyder at programmet ikke blev
        // lukket pænt. Afslut den ved sidste livstegn i stedet for at tælle videre.
        // Undtagen lige efter en opdatering: da lukkede programmet med vilje, og
        // tiden fortsætter (er det over 15 minutter siden, er noget gået galt).
        let updated = std::mem::take(&mut data.updating);
        if let Some(last_seen) = data.active.as_ref().map(|a| a.last_seen)
            && !(updated && Local::now() - last_seen < Duration::minutes(15))
        {
            data.stop_at(last_seen);
        }
        data.internal_project_id();
        Ok(data)
    }

    /// Skriver først til en midlertidig fil og omdøber bagefter, så datafilen
    /// ikke bliver ødelagt hvis programmet stoppes midt i en skrivning.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| tr!("data.err_folder", error = e))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| tr!("data.err_save", error = e))?;
        std::fs::rename(&tmp, path).map_err(|e| tr!("data.err_save", error = e))
    }

    fn new_id(&mut self) -> Id {
        self.next_id += 1;
        self.next_id
    }

    pub fn add_customer(&mut self, name: &str) -> Id {
        let id = self.new_id();
        self.customers.push(Customer { id, name: name.to_owned(), deleted: false });
        id
    }

    pub fn add_project(&mut self, customer_id: Id, name: &str) -> Id {
        let id = self.new_id();
        self.projects.push(Project {
            id,
            customer_id,
            name: name.to_owned(),
            deleted: false,
            favorite: false,
            budget_minutes: None,
        });
        id
    }

    /// Favoritprojekterne (ikke slettede), sorteret efter navn.
    pub fn favorites(&self) -> Vec<&Project> {
        let mut favorites: Vec<&Project> = self.visible_projects().filter(|p| p.favorite).collect();
        favorites.sort_by_key(|p| self.project_label(p.id).to_lowercase());
        favorites
    }

    pub fn toggle_favorite(&mut self, project_id: Id) {
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
            p.favorite = !p.favorite;
        }
    }

    /// Sætter projektets timebudget (`None` eller 0 fjerner det).
    pub fn set_budget(&mut self, project_id: Id, minutes: Option<u32>) {
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
            p.budget_minutes = minutes.filter(|m| *m > 0);
        }
    }

    /// Brugt tid og budget, hvis projektet har et.
    pub fn budget(&self, project_id: Id) -> Option<(Duration, Duration)> {
        let budget = self.project(project_id)?.budget_minutes?;
        Some((self.project_total(project_id), Duration::minutes(budget as i64)))
    }

    /// Kunder og projekter som par (kunde, projekt) til deling med kolleger;
    /// en kunde uden projekter giver (kunde, ""). Ikke den interne kunde.
    pub fn project_list(&self) -> Vec<(String, String)> {
        let mut list = Vec::new();
        for customer in self.visible_customers().filter(|c| !self.is_internal_customer(c.id)) {
            let projects = self.customer_projects(customer.id);
            if projects.is_empty() {
                list.push((customer.name.clone(), String::new()));
            }
            list.extend(projects.into_iter().map(|p| (customer.name.clone(), p.name.clone())));
        }
        list
    }

    /// Tilføjer de kunder og projekter fra en delt liste, der mangler (navne
    /// sammenlignes uden hensyn til store/små bogstaver). Nye kunder får ikke
    /// skabelonens projekter. Giver antal nye kunder og projekter.
    pub fn import_project_list(&mut self, list: &[(String, String)]) -> (usize, usize) {
        let (mut customers, mut projects) = (0, 0);
        for (customer, project) in list {
            let (customer, project) = (customer.trim(), project.trim());
            if customer.is_empty() {
                continue;
            }
            let existing = self.visible_customers().find(|c| same_name(&c.name, customer)).map(|c| c.id);
            let customer_id = match existing {
                Some(id) => id,
                None => {
                    customers += 1;
                    self.add_customer(customer)
                }
            };
            if !project.is_empty()
                && !self.visible_projects().any(|p| p.customer_id == customer_id && same_name(&p.name, project))
            {
                self.add_project(customer_id, project);
                projects += 1;
            }
        }
        (customers, projects)
    }

    /// Dagens arbejdstid (uden pauser), til dagsmålet.
    pub fn work_day_total(&self, date: NaiveDate) -> Duration {
        self.entries_on(date)
            .iter()
            .filter(|e| !e.entry.note.as_deref().is_some_and(|n| n == NOTE_PAUSE))
            .fold(Duration::zero(), |sum, e| sum + e.entry.duration())
    }

    /// Id på projektet "Intern tid". Opretter kunden "Intern" og projektet, hvis de mangler.
    pub fn internal_project_id(&mut self) -> Id {
        if let Some(id) = self.internal_project
            && self.project(id).is_some()
        {
            return id;
        }
        let customer = self.add_customer("Intern");
        let id = self.add_project(customer, "Intern tid");
        self.internal_project = Some(id);
        id
    }

    /// Giver den interne kunde og "Intern tid" navne på det aktuelle sprog –
    /// men kun hvis de stadig har et af standardnavnene (ikke hvis brugeren
    /// selv har omdøbt dem).
    pub fn localize_internal_names(&mut self) {
        let lang = rust_i18n::locale().to_string();
        self.localize_internal_names_in(&lang);
    }

    fn localize_internal_names_in(&mut self, lang: &str) {
        const CUSTOMER_DEFAULTS: [&str; 2] = ["Intern", "Internal"];
        const PROJECT_DEFAULTS: [&str; 2] = ["Intern tid", "Internal time"];
        let Some(id) = self.internal_project else { return };
        let customer_name = rust_i18n::t!("internal.customer", locale = lang).to_string();
        let project_name = rust_i18n::t!("internal.project", locale = lang).to_string();
        let Some(project) = self.projects.iter_mut().find(|p| p.id == id) else { return };
        if PROJECT_DEFAULTS.contains(&project.name.as_str()) {
            project.name = project_name;
        }
        let customer_id = project.customer_id;
        if let Some(customer) = self.customers.iter_mut().find(|c| c.id == customer_id)
            && CUSTOMER_DEFAULTS.contains(&customer.name.as_str())
        {
            customer.name = customer_name;
        }
    }

    /// Kunder der ikke er slettet: den interne kunde først, derefter efter
    /// `settings.sort` (navn, eller senest brugt med de ubrugte bagerst).
    pub fn visible_customers(&self) -> impl Iterator<Item = &Customer> {
        let mut customers: Vec<&Customer> = self.customers.iter().filter(|c| !c.deleted).collect();
        let last_used = |c: &Customer| {
            self.projects.iter().filter(|p| p.customer_id == c.id).filter_map(|p| self.last_used(p.id)).max()
        };
        customers.sort_by_cached_key(|c| {
            let recent = match self.settings.sort {
                SortOrder::Recent => last_used(c).map(std::cmp::Reverse),
                SortOrder::Name => None,
            };
            (!self.is_internal_customer(c.id), recent.is_none(), recent, sort_key(&c.name))
        });
        customers.into_iter()
    }

    /// En kundes projekter (ikke slettede), efter `settings.sort`.
    pub fn customer_projects(&self, customer_id: Id) -> Vec<&Project> {
        let mut projects: Vec<&Project> =
            self.visible_projects().filter(|p| p.customer_id == customer_id).collect();
        projects.sort_by_cached_key(|p| {
            let recent = match self.settings.sort {
                SortOrder::Recent => self.last_used(p.id).map(std::cmp::Reverse),
                SortOrder::Name => None,
            };
            (recent.is_none(), recent, sort_key(&p.name))
        });
        projects
    }

    /// Hvornår der sidst blev registreret tid på et projekt (eller nu, hvis det kører).
    fn last_used(&self, project_id: Id) -> Option<DateTime<Local>> {
        if self.active.as_ref().is_some_and(|a| a.project_id == Some(project_id)) {
            return Some(Local::now());
        }
        self.entries.iter().filter(|e| e.project_id == project_id).map(|e| e.end).max()
    }

    /// Opretter en kunde med projekterne fra kundeskabelonen (issue #9).
    /// Returnerer kunden og evt. det projekt, der er markeret Auto start.
    pub fn create_customer(&mut self, name: &str) -> (Id, Option<Id>) {
        let customer = self.add_customer(name);
        let mut auto_start = None;
        for template in self.settings.customer_template.clone() {
            let name = template.name.trim();
            if name.is_empty() {
                continue;
            }
            let id = self.add_project(customer, name);
            if template.auto_start && auto_start.is_none() {
                auto_start = Some(id);
            }
        }
        (customer, auto_start)
    }

    /// Den (ikke slettede) kunde med navnet, uden hensyn til store/små bogstaver.
    pub fn find_customer(&self, name: &str) -> Option<Id> {
        let name = name.trim();
        self.visible_customers().find(|c| same_name(&c.name, name)).map(|c| c.id)
    }

    /// "+ Ny kunde…" i ret-vinduet (#37): opretter kunden med skabelonens
    /// projekter (uden at starte noget) og giver projektet, der skal vælges:
    /// det med auto start, ellers det første. `first_project` bruges, når
    /// skabelonen er tom (eller kunden findes uden projekter). Findes kunden,
    /// oprettes den ikke igen. Giver (projekt, om kunden fandtes), eller `None`
    /// hvis der ikke er noget projekt at vælge.
    pub fn add_customer_from_editor(&mut self, name: &str, first_project: &str) -> Option<(Id, bool)> {
        let (name, first) = (name.trim(), first_project.trim());
        if name.is_empty() {
            return None;
        }
        // Ingen skabelon og intet første projekt: opret ikke en kunde uden projekter.
        let template_empty = self.settings.customer_template.iter().all(|t| t.name.trim().is_empty());
        if first.is_empty() && template_empty && self.find_customer(name).is_none() {
            return None;
        }
        let (customer, existed, auto_start) = match self.find_customer(name) {
            Some(id) => (id, true, None),
            None => {
                let (id, auto_start) = self.create_customer(name);
                (id, false, auto_start)
            }
        };
        if !first.is_empty() {
            let existing =
                self.visible_projects().find(|p| p.customer_id == customer && same_name(&p.name, first)).map(|p| p.id);
            return Some((existing.unwrap_or_else(|| self.add_project(customer, first)), existed));
        }
        let pick = auto_start.or_else(|| self.customer_projects(customer).first().map(|p| p.id))?;
        Some((pick, existed))
    }

    /// Flytter den kørende tidstagning til et andet projekt uden at nulstille
    /// uret (issue #10). Et opkald bliver dermed registreret på projektet.
    pub fn move_active(&mut self, project_id: Id) {
        if let Some(active) = &mut self.active {
            active.project_id = Some(project_id);
            active.target = CallTarget::default();
        }
    }

    /// Det tidligste tidspunkt i dag fra `wanted`, tiden kan startes bagud fra:
    /// ikke før den seneste registrering sluttede, og ikke efter nu.
    pub fn start_from_time(&self, wanted: DateTime<Local>) -> DateTime<Local> {
        let now = Local::now();
        let latest_end = self.entries.iter().map(|e| e.end).filter(|end| *end <= now).max();
        latest_end.map_or(wanted, |end| wanted.max(end)).min(now)
    }

    /// Starter tid på projektet fra `from` (bagud), f.eks. når man har glemt at
    /// starte den. Se `start_from_time`.
    pub fn start_from(&mut self, project_id: Id, from: DateTime<Local>) {
        let from = self.start_from_time(from);
        self.start(project_id);
        if let Some(active) = &mut self.active {
            active.start = from;
        }
    }

    /// Flytter den kørende tid fra `at` til et andet projekt, f.eks. hvis man
    /// glemte at skifte. Tiden før `at` gemmes på det hidtidige projekt (med dets
    /// type og note), og tiden kører videre på `project_id` fra `at`. Er `at`
    /// ved starten (eller før), flyttes det hele som `move_active`.
    pub fn split_active(&mut self, at: DateTime<Local>, project_id: Id) {
        let Some(active) = &self.active else { return };
        let now = Local::now();
        if at <= active.start + Duration::seconds(1) || active.project_id.is_none() {
            self.move_active(project_id);
            return;
        }
        let at = at.min(now);
        self.stop_at(at);
        self.active = Some(ActiveTimer { project_id: Some(project_id), start: at, last_seen: now, ..Default::default() });
    }

    /// Stopper tiden pr. `at`, f.eks. da computeren gik i dvale (issue #6).
    /// Returnerer tidstagningen med uret frosset, så den kan genoptages.
    pub fn suspend(&mut self, at: DateTime<Local>) -> Option<ActiveTimer> {
        let active = self.active.as_ref()?;
        let at = at.max(active.start);
        let snapshot = ActiveTimer {
            carried_secs: active.carried_secs + (at - active.start).num_seconds(),
            ..active.clone()
        };
        self.stop_at(at);
        Some(snapshot)
    }

    /// Genoptager en tidstagning fra `suspend`: samme projekt, note og type, og
    /// uret tæller videre. Gør intet, hvis projektet er slettet i mellemtiden.
    pub fn resume(&mut self, snapshot: ActiveTimer) {
        if let Some(id) = snapshot.project_id
            && !self.project(id).is_some_and(|p| !p.deleted)
        {
            return;
        }
        let now = Local::now();
        self.stop();
        self.active = Some(ActiveTimer { start: now, last_seen: now, ..snapshot });
    }

    /// Projekter der ikke er slettet.
    pub fn visible_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.iter().filter(|p| !p.deleted)
    }

    pub fn is_internal(&self, project_id: Id) -> bool {
        self.internal_project == Some(project_id)
    }

    pub fn is_internal_customer(&self, customer_id: Id) -> bool {
        self.internal_project
            .and_then(|id| self.project(id))
            .is_some_and(|p| p.customer_id == customer_id)
    }

    /// Antal registreringer og samlet tid på et projekt.
    pub fn project_stats(&self, project_id: Id) -> (usize, Duration) {
        let entries: Vec<_> = self.entries.iter().filter(|e| e.project_id == project_id).collect();
        (entries.len(), self.project_total(project_id))
    }

    /// Antal registreringer og samlet tid på alle en kundes projekter.
    pub fn customer_stats(&self, customer_id: Id) -> (usize, Duration) {
        self.projects
            .iter()
            .filter(|p| p.customer_id == customer_id)
            .map(|p| self.project_stats(p.id))
            .fold((0, Duration::zero()), |(n, d), (pn, pd)| (n + pn, d + pd))
    }

    /// Skjuler et projekt. Kører timeren på det, stoppes den først, så tiden gemmes.
    /// Registreringerne bevares i historikken. "Intern tid" kan ikke slettes.
    pub fn delete_project(&mut self, project_id: Id) {
        if self.is_internal(project_id) {
            return;
        }
        if self.active.as_ref().is_some_and(|a| a.project_id == Some(project_id)) {
            self.stop();
        }
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
            p.deleted = true;
        }
    }

    /// Omdøber en kunde. Navnet må ikke være tomt eller allerede bruges af en
    /// anden kunde (uanset store/små bogstaver).
    pub fn rename_customer(&mut self, customer_id: Id, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(tr!("data.err_empty_name"));
        }
        if self.visible_customers().any(|c| c.id != customer_id && same_name(&c.name, name)) {
            return Err(tr!("data.err_customer_exists"));
        }
        let customer = self.customers.iter_mut().find(|c| c.id == customer_id).ok_or_else(|| tr!("data.err_no_customer"))?;
        customer.name = name.to_owned();
        Ok(())
    }

    /// Omdøber et projekt. Navnet må ikke være tomt eller allerede bruges af et
    /// andet projekt hos samme kunde.
    pub fn rename_project(&mut self, project_id: Id, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(tr!("data.err_empty_name"));
        }
        let customer_id = self.project(project_id).ok_or_else(|| tr!("data.err_no_project"))?.customer_id;
        if self
            .visible_projects()
            .any(|p| p.id != project_id && p.customer_id == customer_id && same_name(&p.name, name))
        {
            return Err(tr!("data.err_project_exists"));
        }
        if let Some(project) = self.projects.iter_mut().find(|p| p.id == project_id) {
            project.name = name.to_owned();
        }
        Ok(())
    }

    /// Skjuler en kunde og alle dens projekter.
    pub fn delete_customer(&mut self, customer_id: Id) {
        if self.is_internal_customer(customer_id) {
            return;
        }
        let ids: Vec<Id> = self.projects.iter().filter(|p| p.customer_id == customer_id).map(|p| p.id).collect();
        for id in ids {
            self.delete_project(id);
        }
        if let Some(c) = self.customers.iter_mut().find(|c| c.id == customer_id) {
            c.deleted = true;
        }
    }

    pub fn project(&self, id: Id) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn customer(&self, id: Id) -> Option<&Customer> {
        self.customers.iter().find(|c| c.id == id)
    }

    /// "Kunde / Projekt" til visning.
    pub fn project_label(&self, project_id: Id) -> String {
        let Some(project) = self.project(project_id) else {
            return tr!("data.deleted_project");
        };
        let customer = self.customer(project.customer_id).map_or("?", |c| c.name.as_str());
        format!("{customer} / {}", project.name)
    }

    /// Starter tid på et projekt. Kører der allerede en timer, gemmes den først.
    pub fn start(&mut self, project_id: Id) {
        self.start_timer(Some(project_id), None);
    }

    /// Starter et telefonopkald, som først skal registreres på et projekt bagefter.
    pub fn start_call(&mut self) {
        self.start_timer(None, Some(NOTE_PHONE));
    }

    /// Starter en pause, som registreres på "Intern tid". Kører der en pause,
    /// afsluttes den i stedet, og tiden fortsætter, hvor den slap før pausen.
    pub fn toggle_pause(&mut self) {
        if self.active.as_ref().is_some_and(|a| a.is_note(NOTE_PAUSE)) {
            self.end_interruption();
            return;
        }
        // Under et opkald: pausen afslutter opkaldet og vender bagefter tilbage
        // til det, der kørte før opkaldet.
        let previous = match self.active.as_ref() {
            Some(a) if a.is_call() => a.previous.clone(),
            other => other.map(frozen),
        };
        let id = self.internal_project_id();
        self.start_timer(Some(id), Some(NOTE_PAUSE));
        if let Some(active) = &mut self.active {
            active.previous = previous;
        }
    }

    /// Starter et telefonopkald. Kører der et opkald, afsluttes det i stedet, og
    /// tiden fortsætter på det, der kørte, da telefonen ringede. Opkaldet
    /// registreres på sit projekt, eller venter på at få et.
    pub fn toggle_call(&mut self) {
        if self.active.as_ref().is_some_and(ActiveTimer::is_call) {
            self.end_interruption();
            return;
        }
        let previous = self.active.as_ref().map(frozen);
        self.start_call();
        if let Some(active) = &mut self.active {
            active.previous = previous;
        }
    }

    /// Afslutter den kørende pause/det kørende opkald og fortsætter på det, der
    /// kørte før: samme projekt, note og type, og uret tæller videre derfra.
    fn end_interruption(&mut self) {
        let Some(active) = self.active.as_ref() else { return };
        let previous = active.previous.clone();
        self.stop();
        let Some(previous) = previous else { return };
        if let Some(id) = previous.project_id
            && !self.project(id).is_some_and(|p| !p.deleted)
        {
            return; // projektet er slettet i mellemtiden
        }
        let now = Local::now();
        self.active = Some(ActiveTimer { start: now, last_seen: now, ..*previous });
    }

    /// Projektet en kørende pause eller et kørende opkald vender tilbage til,
    /// hvis det stadig findes.
    pub fn resume_project(&self) -> Option<Id> {
        let id = self.active.as_ref()?.previous.as_ref()?.project_id?;
        self.project(id).filter(|p| !p.deleted).map(|p| p.id)
    }

    /// Flytter tiden fra `from` til `to` fra den kørende timer over på pause.
    /// Tiden før `from` gemmes på projektet, perioden bliver en pause, og timeren
    /// kører videre på projektet fra `to`, mens uret fortsætter uden pausen.
    /// Gør intet og returnerer false, hvis timeren ikke længere passer.
    pub fn convert_to_pause(&mut self, project_id: Id, from: DateTime<Local>, to: DateTime<Local>) -> bool {
        let Some(active) = self.active.clone() else { return false };
        if active.project_id != Some(project_id) || active.is_note(NOTE_PAUSE) || from < active.start || to <= from {
            return false;
        }
        let internal = self.internal_project_id();
        let before = from - active.start;
        if before >= Duration::seconds(1) {
            self.entries.push(TimeEntry {
                project_id,
                start: active.start,
                end: from,
                note: active.note.clone(),
                description: active.description.clone(),
            });
        }
        self.entries.push(TimeEntry {
            project_id: internal,
            start: from,
            end: to,
            note: Some(NOTE_PAUSE.to_owned()),
            ..Default::default()
        });
        if let Some(active) = &mut self.active {
            active.start = to;
            active.carried_secs += before.num_seconds();
            active.last_seen = active.last_seen.max(to);
        }
        true
    }

    fn start_timer(&mut self, project_id: Option<Id>, note: Option<&str>) {
        let now = Local::now();
        self.stop_at(now);
        self.active = Some(ActiveTimer {
            project_id,
            start: now,
            last_seen: now,
            note: note.map(str::to_owned),
            ..Default::default()
        });
    }

    pub fn stop(&mut self) {
        self.stop_at(Local::now());
    }

    fn stop_at(&mut self, end: DateTime<Local>) {
        let Some(active) = self.active.take() else { return };
        // Spring fejlklik over (under ét sekund).
        if end - active.start < Duration::seconds(1) {
            return;
        }
        match active.project_id {
            Some(project_id) => self.entries.push(TimeEntry {
                project_id,
                start: active.start,
                end,
                note: active.note,
                description: active.description,
            }),
            None => {
                let call = UnassignedEntry {
                    start: active.start,
                    end,
                    target: active.target,
                    description: active.description,
                };
                // Er projektet valgt under opkaldet, registreres det med det samme.
                if !self.register_call(&call) {
                    self.unassigned.push(call);
                }
            }
        }
    }

    pub fn target_is_complete(&self, target: &CallTarget) -> bool {
        target.project_id.is_some_and(|id| self.project(id).is_some_and(|p| !p.deleted)) || target.typed().is_some()
    }

    /// Projektet som et mål peger på. Indtastet kunde og projekt genbruges hvis
    /// de findes i forvejen (uanset store/små bogstaver), ellers oprettes de.
    pub fn resolve_target(&mut self, target: &CallTarget) -> Option<Id> {
        if let Some(id) = target.project_id
            && self.project(id).is_some_and(|p| !p.deleted)
        {
            return Some(id);
        }
        let (customer, project) = target.typed()?;
        let existing = self.visible_customers().find(|c| same_name(&c.name, customer)).map(|c| c.id);
        let customer_id = match existing {
            Some(id) => id,
            None => self.create_customer(customer).0,
        };
        let existing = self
            .visible_projects()
            .find(|p| p.customer_id == customer_id && same_name(&p.name, project))
            .map(|p| p.id);
        Some(match existing {
            Some(id) => id,
            None => self.add_project(customer_id, project),
        })
    }

    /// Gemmer opkaldet som registrering, hvis dets mål er udfyldt.
    fn register_call(&mut self, call: &UnassignedEntry) -> bool {
        let Some(project_id) = self.resolve_target(&call.target) else {
            return false;
        };
        self.entries.push(TimeEntry {
            project_id,
            start: call.start,
            end: call.end,
            note: Some(NOTE_PHONE.to_owned()),
            description: call.description.clone(),
        });
        true
    }

    /// Registrerer det første ikke-registrerede opkald på det mål, der er udfyldt
    /// i det. Returnerer false hvis målet ikke er udfyldt.
    pub fn register_first_unassigned(&mut self) -> bool {
        let Some(call) = self.unassigned.first().cloned() else {
            return false;
        };
        let registered = self.register_call(&call);
        if registered {
            self.unassigned.remove(0);
        }
        registered
    }

    pub fn discard_first_unassigned(&mut self) {
        if !self.unassigned.is_empty() {
            self.unassigned.remove(0);
        }
    }

    /// Alle registreringer der startede på den givne dag, inkl. den aktive timer
    /// hvis den hører til et projekt. Sorteret efter starttid.
    pub fn entries_on(&self, date: NaiveDate) -> Vec<DayEntry> {
        let mut list: Vec<DayEntry> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.start.date_naive() == date)
            .map(|(i, e)| DayEntry { index: Some(i), entry: e.clone() })
            .collect();
        if let Some(entry) = self.running_entry()
            && entry.start.date_naive() == date
        {
            list.push(DayEntry { index: None, entry });
        }
        list.sort_by_key(|e| e.entry.start);
        list
    }

/// Huller på mindst `GAP_MIN` mellem dagens registreringer (sorteret efter
    /// start): tidsrum, hvor der ikke er registreret noget.
    pub fn gaps(entries: &[DayEntry]) -> Vec<(DateTime<Local>, DateTime<Local>)> {
        let mut gaps = Vec::new();
        let mut covered_until: Option<DateTime<Local>> = None;
        for e in entries {
            if let Some(until) = covered_until
                && e.entry.start - until >= GAP_MIN
            {
                gaps.push((until, e.entry.start));
            }
            covered_until = Some(covered_until.map_or(e.entry.end, |u| u.max(e.entry.end)));
        }
        gaps
    }

    /// Den kørende timer som registrering frem til nu, hvis den hører til et projekt.
    fn running_entry(&self) -> Option<TimeEntry> {
        let a = self.active.as_ref()?;
        Some(TimeEntry {
            project_id: a.project_id?,
            start: a.start,
            end: Local::now(),
            note: a.note.clone(),
            description: a.description.clone(),
        })
    }

    pub fn add_entry(&mut self, entry: TimeEntry) {
        self.entries.push(entry);
    }

    /// Retter registrering `index`, eller tilføjer en ny med `None`.
    pub fn save_entry(&mut self, index: Option<usize>, entry: TimeEntry) {
        match index {
            Some(index) => self.update_entry(index, entry),
            None => self.add_entry(entry),
        }
    }

    pub fn update_entry(&mut self, index: usize, entry: TimeEntry) {
        if let Some(e) = self.entries.get_mut(index) {
            *e = entry;
        }
    }

    pub fn delete_entry(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
        }
    }

    /// Andre registreringer (inkl. den kørende timer) der overlapper tidsrummet.
    /// `skip` er registreringen selv, når den rettes.
    pub fn overlapping(&self, entry: &TimeEntry, skip: Option<usize>) -> Vec<TimeEntry> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != skip)
            .map(|(_, e)| e.clone())
            .chain(self.running_entry())
            .filter(|e| e.start < entry.end && entry.start < e.end)
            .collect()
    }

    /// Projektets tid i ugen med `date` (mandag til og med `date`), inkl. den aktive timer.
    pub fn project_week_total(&self, project_id: Id, date: NaiveDate) -> Duration {
        let monday = date - Duration::days(date.weekday().num_days_from_monday() as i64);
        (0..=date.weekday().num_days_from_monday() as i64)
            .map(|d| self.project_day_total(monday + Duration::days(d), project_id))
            .fold(Duration::zero(), |sum, d| sum + d)
    }

    /// Al tid brugt på et projekt nogensinde, inkl. den aktive timer.
    pub fn project_total(&self, project_id: Id) -> Duration {
        let finished = self
            .entries
            .iter()
            .filter(|e| e.project_id == project_id)
            .fold(Duration::zero(), |sum, e| sum + e.duration());
        let running = match &self.active {
            Some(a) if a.project_id == Some(project_id) => Local::now() - a.start,
            _ => Duration::zero(),
        };
        finished + running
    }

    /// Afrundet tid for et projekt en dag: op til nærmeste kvarter, eller ned
    /// hvis det er valgt manuelt.
    pub fn rounded(&self, date: NaiveDate, project_id: Id, time: Duration) -> Rounded {
        let (up, down) = (round_up_quarter(time), round_down_quarter(time));
        let chosen_down = self.round_down.contains(&RoundDown { date, project_id });
        Rounded {
            time: if chosen_down { down } else { up },
            down: chosen_down && up != down,
            can_toggle: up != down,
        }
    }

    /// Al registreret tid en dag (inkl. den kørende timer).
    pub fn day_total(&self, date: NaiveDate) -> Duration {
        self.totals_on(date).iter().fold(Duration::zero(), |sum, (_, d)| sum + *d)
    }

    /// Dagens afrundede tid (summen pr. projekt som i Historik → Summeret), men
    /// kun hvis der er rundet noget ned manuelt den dag; ellers `None`.
    pub fn rounded_day_total(&self, date: NaiveDate) -> Option<Duration> {
        let totals = self.totals_on(date);
        let any_down = totals.iter().any(|(id, d)| self.rounded(date, *id, *d).down);
        any_down.then(|| totals.iter().fold(Duration::zero(), |sum, (id, d)| sum + self.rounded(date, *id, *d).time))
    }

    /// Dagens tid på ét projekt (inkl. den kørende timer).
    pub fn project_day_total(&self, date: NaiveDate, project_id: Id) -> Duration {
        self.totals_on(date).into_iter().find(|(id, _)| *id == project_id).map_or(Duration::zero(), |(_, d)| d)
    }

    /// Skifter mellem at runde projektets tid den dag op eller ned.
    /// Er projektets tid den dag krydset af? `rounded` er den afrundede tid nu.
    pub fn billed_state(&self, date: NaiveDate, project_id: Id, rounded: Duration) -> BilledState {
        match self.billed.iter().find(|b| b.date == date && b.project_id == project_id) {
            None => BilledState::No,
            Some(b) if b.minutes == rounded.num_minutes() => BilledState::Yes,
            Some(b) => BilledState::Changed(Duration::minutes(b.minutes)),
        }
    }

    /// Kryds af med den afrundede tid nu (`Some`), eller fjern krydset (`None`).
    pub fn set_billed(&mut self, date: NaiveDate, project_id: Id, rounded: Option<Duration>) {
        self.billed.retain(|b| !(b.date == date && b.project_id == project_id));
        if let Some(time) = rounded {
            self.billed.push(Billed { date, project_id, minutes: time.num_minutes() });
        }
    }

    pub fn toggle_round_down(&mut self, date: NaiveDate, project_id: Id) {
        let item = RoundDown { date, project_id };
        match self.round_down.iter().position(|r| *r == item) {
            Some(i) => {
                self.round_down.remove(i);
            }
            None => self.round_down.push(item),
        }
    }

    /// Samlet tid pr. projekt på en dag, sorteret efter mest tid først.
    pub fn totals_on(&self, date: NaiveDate) -> Vec<(Id, Duration)> {
        let mut totals: Vec<(Id, Duration)> = Vec::new();
        for DayEntry { entry: e, .. } in self.entries_on(date) {
            match totals.iter_mut().find(|(id, _)| *id == e.project_id) {
                Some((_, d)) => *d += e.duration(),
                None => totals.push((e.project_id, e.duration())),
            }
        }
        totals.sort_by_key(|(_, d)| std::cmp::Reverse(*d));
        totals
    }

    /// Om registreringer på projektet hører til udsnittet.
    pub fn in_scope(&self, scope: Scope, project_id: Id) -> bool {
        match scope {
            Scope::Project(id) => project_id == id,
            Scope::Customer(id) => self.project(project_id).is_some_and(|p| p.customer_id == id),
        }
    }

    /// Kundens navn eller "Kunde / Projekt".
    pub fn scope_label(&self, scope: Scope) -> String {
        match scope {
            Scope::Project(id) => self.project_label(id),
            Scope::Customer(id) => self.customer(id).map_or_else(|| "?".to_owned(), |c| c.name.clone()),
        }
    }

    /// Udsnittets registreringer, der startede fra og med `from` til og med
    /// `to` (`None` er ingen grænse), inkl. den kørende timer, sorteret efter start.
    pub fn entries_in(&self, scope: Scope, from: Option<NaiveDate>, to: Option<NaiveDate>) -> Vec<DayEntry> {
        let within = |e: &TimeEntry| {
            let day = e.start.date_naive();
            self.in_scope(scope, e.project_id) && from.is_none_or(|f| day >= f) && to.is_none_or(|t| day <= t)
        };
        let mut list: Vec<DayEntry> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| within(e))
            .map(|(i, e)| DayEntry { index: Some(i), entry: e.clone() })
            .collect();
        if let Some(entry) = self.running_entry()
            && within(&entry)
        {
            list.push(DayEntry { index: None, entry });
        }
        list.sort_by_key(|e| e.entry.start);
        list
    }
}

/// Nøgle til alfabetisk sortering efter dansk alfabet: store og små bogstaver
/// sorteres ens, og æ, ø, å kommer efter z i den rækkefølge (i Unicode ligger
/// å før æ). "{", "|" og "}" kommer lige efter "z" i ASCII.
fn sort_key(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'æ' => '{',
            'ø' => '|',
            'å' => '}',
            c => c,
        })
        .collect()
}

/// Navne sammenlignes uden hensyn til store/små bogstaver (også æ, ø, å).
fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Runder op til nærmeste påbegyndte kvarter. Hele kvarterer og nul ændres ikke.
/// Huller kortere end dette vises ikke i Historik.
pub const GAP_MIN: Duration = Duration::minutes(5);

pub fn round_up_quarter(d: Duration) -> Duration {
    let quarter = 15 * 60;
    let secs = d.num_seconds().max(0);
    Duration::seconds((secs + quarter - 1) / quarter * quarter)
}

/// Runder ned til nærmeste hele kvarter.
pub fn round_down_quarter(d: Duration) -> Duration {
    let quarter = 15 * 60;
    Duration::seconds(d.num_seconds().max(0) / quarter * quarter)
}

/// Timer og minutter, f.eks. "1:15".
/// En længde i ord, f.eks. "33 min" eller "1 t 05 min" ("1 h 05 min" på engelsk).
pub fn format_length(d: Duration) -> String {
    let minutes = d.num_minutes().max(0);
    if minutes < 60 {
        tr!("time.minutes", minutes = minutes)
    } else {
        tr!("time.hours_minutes", hours = minutes / 60, minutes = format!("{:02}", minutes % 60))
    }
}

/// Typen ("Telefon"/"Pause") på brugerens sprog. Gemmes altid på dansk.
pub fn note_label(note: &str) -> String {
    match note {
        NOTE_PHONE => tr!("note.phone"),
        NOTE_PAUSE => tr!("note.pause"),
        other => other.to_owned(),
    }
}

pub fn format_hours_minutes(d: Duration) -> String {
    let mins = d.num_minutes().max(0);
    format!("{}:{:02}", mins / 60, mins % 60)
}

pub fn format_duration(d: Duration) -> String {
    let secs = d.num_seconds().max(0);
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone};

    fn active_since(project_id: Option<Id>, minutes: i64) -> Option<ActiveTimer> {
        Some(ActiveTimer {
            project_id,
            start: Local::now() - Duration::minutes(minutes),
            last_seen: Local::now(),
            note: None,
            ..Default::default()
        })
    }

    #[test]
    fn billed_warns_when_time_changes_afterwards() {
        let mut data = Data::default();
        let day = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        let (hour, more) = (Duration::minutes(60), Duration::minutes(75));
        assert_eq!(data.billed_state(day, 1, hour), BilledState::No);
        data.set_billed(day, 1, Some(hour));
        assert_eq!(data.billed_state(day, 1, hour), BilledState::Yes);
        assert_eq!(data.billed_state(day, 2, hour), BilledState::No);
        assert_eq!(data.billed_state(day, 1, more), BilledState::Changed(hour));
        // Krydses af igen med den nye tid.
        data.set_billed(day, 1, Some(more));
        assert_eq!(data.billed_state(day, 1, more), BilledState::Yes);
        assert_eq!(data.billed.len(), 1);
        data.set_billed(day, 1, None);
        assert_eq!(data.billed_state(day, 1, more), BilledState::No);
    }

    #[test]
    fn scope_picks_customer_or_project_within_period() {
        let mut data = Data::default();
        let c1 = data.add_customer("Kunde 1");
        let a = data.add_project(c1, "A");
        let b = data.add_project(c1, "B");
        let c2 = data.add_customer("Kunde 2");
        let other = data.add_project(c2, "C");
        let at = |day: u32, hour: u32| Local.with_ymd_and_hms(2026, 10, day, hour, 0, 0).unwrap();
        for (project_id, day) in [(a, 1), (b, 2), (other, 2), (a, 3)] {
            data.add_entry(TimeEntry { project_id, start: at(day, 9), end: at(day, 10), ..Default::default() });
        }
        let days = |list: Vec<DayEntry>| list.iter().map(|e| e.entry.start.day()).collect::<Vec<_>>();

        assert_eq!(days(data.entries_in(Scope::Customer(c1), None, None)), [1, 2, 3]);
        assert_eq!(days(data.entries_in(Scope::Project(a), None, None)), [1, 3]);
        let oct = |d| NaiveDate::from_ymd_opt(2026, 10, d);
        assert_eq!(days(data.entries_in(Scope::Customer(c1), oct(2), oct(3))), [2, 3]);
        assert_eq!(days(data.entries_in(Scope::Customer(c2), None, oct(1))), Vec::<u32>::new());
        // Den kørende timer kommer med, uden index.
        data.active = active_since(Some(b), 5);
        let running = data.entries_in(Scope::Project(b), None, None);
        assert!(running.last().is_some_and(|e| e.index.is_none()));
    }

    #[test]
    fn switching_project_saves_previous_entry() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p1 = data.add_project(c, "A");
        let p2 = data.add_project(c, "B");

        data.active = active_since(Some(p1), 5);
        data.start(p2);

        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].project_id, p1);
        assert_eq!(data.active.as_ref().unwrap().project_id, Some(p2));
    }

    #[test]
    fn call_becomes_unassigned_and_can_be_assigned_later() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");

        data.start_call();
        data.active.as_mut().unwrap().start -= Duration::minutes(10);
        data.start(p);

        assert!(data.entries.is_empty());
        assert_eq!(data.unassigned.len(), 1);
        assert_eq!(data.active.as_ref().unwrap().project_id, Some(p));

        assert!(!data.register_first_unassigned(), "intet mål valgt endnu");
        let internal = data.internal_project_id();
        data.unassigned[0].target.project_id = Some(internal);
        assert!(data.register_first_unassigned());
        assert!(data.unassigned.is_empty());
        assert_eq!(data.entries[0].project_id, internal);
        assert_eq!(data.entries[0].note.as_deref(), Some(NOTE_PHONE));
    }

    #[test]
    fn call_with_target_chosen_during_call_is_registered_directly() {
        let mut data = Data::default();
        let c = data.add_customer("Ærø Træ");
        let p = data.add_project(c, "Hjemmeside");

        data.start_call();
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(5);
        // Skrevet med andre store/små bogstaver: skal genbruge kunde og projekt.
        active.target.customer = "ærø træ".to_owned();
        active.target.project = " HJEMMESIDE ".to_owned();
        data.stop();

        assert!(data.unassigned.is_empty());
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].project_id, p);
        assert_eq!(data.customers.len(), 1);
        assert_eq!(data.projects.len(), 1);
    }

    #[test]
    fn typed_new_customer_and_project_are_created() {
        let mut data = Data::default();
        data.start_call();
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(5);
        active.target.customer = "Ny kunde".to_owned();
        active.target.project = "Support".to_owned();
        data.stop();

        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.project_label(data.entries[0].project_id), "Ny kunde / Support");
    }

    #[test]
    fn deleting_project_hides_it_but_keeps_history() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.active = active_since(Some(p), 20);

        data.delete_project(p);

        assert!(data.active.is_none(), "timeren på projektet stoppes");
        assert_eq!(data.entries.len(), 1, "tiden er gemt");
        assert_eq!(data.visible_projects().count(), 0);
        assert_eq!(data.project_label(p), "Kunde / A", "historikken kan stadig vise navnet");
    }

    #[test]
    fn deleting_customer_hides_its_projects_and_typed_name_creates_new() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.delete_customer(c);

        assert_eq!(data.visible_customers().count(), 0);
        assert!(data.project(p).unwrap().deleted);

        let target = CallTarget { customer: "Kunde".into(), project: "A".into(), ..Default::default() };
        let new_id = data.resolve_target(&target).unwrap();
        assert_ne!(new_id, p);
    }

    #[test]
    fn internal_project_cannot_be_deleted() {
        let mut data = Data::default();
        let internal = data.internal_project_id();
        let customer = data.project(internal).unwrap().customer_id;
        data.delete_customer(customer);
        data.delete_project(internal);
        assert!(!data.project(internal).unwrap().deleted);
        assert!(!data.customer(customer).unwrap().deleted);
    }

    #[test]
    fn edit_and_delete_entries_and_find_overlaps() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        let at = |h: u32, m: u32| {
            Local.from_local_datetime(&NaiveDate::from_ymd_opt(2026, 10, 20).unwrap().and_hms_opt(h, m, 0).unwrap())
                .unwrap()
        };
        let entry = |from, to| TimeEntry { project_id: p, start: from, end: to, ..Default::default() };
        data.add_entry(entry(at(8, 0), at(9, 0)));
        data.add_entry(entry(at(10, 0), at(11, 0)));

        // 8:30-10:30 rammer begge; rettes nr. 0 til det, overlapper den kun nr. 1.
        let changed = entry(at(8, 30), at(10, 30));
        assert_eq!(data.overlapping(&changed, None).len(), 2);
        assert_eq!(data.overlapping(&changed, Some(0)).len(), 1);
        // Registreringer der bare støder op til hinanden overlapper ikke.
        assert!(data.overlapping(&entry(at(9, 0), at(10, 0)), None).is_empty());

        data.update_entry(0, changed);
        assert_eq!(data.entries[0].duration(), Duration::hours(2));
        data.delete_entry(1);
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries_on(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap())[0].index, Some(0));
    }

    #[test]
    fn rename_rejects_empty_and_duplicate_names() {
        let mut data = Data::default();
        let a = data.add_customer("Alfa");
        let b = data.add_customer("Beta");
        let p1 = data.add_project(a, "Web");
        let p2 = data.add_project(a, "App");
        let other = data.add_project(b, "Andet");

        assert!(data.rename_customer(a, "  ").is_err());
        assert!(data.rename_customer(a, "beta").is_err(), "navnet bruges af en anden kunde");
        assert!(data.rename_customer(a, "ALFA").is_ok(), "samme kunde må skifte store/små bogstaver");
        assert!(data.rename_customer(a, " Gamma ").is_ok());
        assert_eq!(data.customer(a).unwrap().name, "Gamma");

        assert!(data.rename_project(p1, "app").is_err(), "navnet bruges hos samme kunde");
        assert!(data.rename_project(other, "Web").is_ok(), "samme navn hos en anden kunde er fint");
        assert!(data.rename_project(p2, "Mobil").is_ok());
        assert_eq!(data.project_label(p2), "Gamma / Mobil");
    }

    #[test]
    fn pressing_pause_again_resumes_previous_project() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.active = active_since(Some(p), 30);

        data.toggle_pause();
        let internal = data.internal_project_id();
        assert_eq!(data.active.as_ref().unwrap().project_id, Some(internal));
        assert_eq!(data.resume_project(), Some(p));
        data.active.as_mut().unwrap().start -= Duration::minutes(10);

        data.toggle_pause();
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(p), "tilbage på projektet");
        let shown = active.elapsed().num_minutes();
        assert_eq!(shown, 30, "uret tæller videre fra de 30 min før pausen, ikke fra 0");
        assert!(active.note.is_none());
        assert_eq!(data.entries.len(), 2, "projekttid og pause er gemt");
        assert_eq!(data.entries[1].note.as_deref(), Some(NOTE_PAUSE));
    }

    #[test]
    fn away_time_can_be_converted_to_pause() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        let now = Local::now();
        data.active = Some(ActiveTimer {
            project_id: Some(p),
            start: now - Duration::minutes(60),
            last_seen: now,
            note: None,
            ..Default::default()
        });
        // Væk fra 40 til 10 minutter siden.
        let (from, to) = (now - Duration::minutes(40), now - Duration::minutes(10));
        assert!(data.convert_to_pause(p, from, to));

        let internal = data.internal_project_id();
        assert_eq!(data.entries.len(), 2);
        assert_eq!(data.entries[0].project_id, p);
        assert_eq!(data.entries[0].duration(), Duration::minutes(20));
        assert_eq!(data.entries[1].project_id, internal);
        assert_eq!(data.entries[1].duration(), Duration::minutes(30));
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(p));
        assert_eq!(active.start, to);
        assert_eq!(active.elapsed().num_minutes(), 30, "uret viser 20 + 10 min uden pausen");

        assert!(!data.convert_to_pause(c + 100, from, to), "andet projekt end det der kører");
    }

    #[test]
    fn settings_default_when_missing_from_file() {
        let data: Data = serde_json::from_str(r#"{"next_id":0,"customers":[],"projects":[],"entries":[],"active":null}"#).unwrap();
        assert!(data.settings.away_enabled);
        assert_eq!(data.settings.away_minutes, 15);
    }

    #[test]
    fn note_follows_entry_through_stop_pause_and_call() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");

        data.start(p);
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(10);
        active.description = "Forside".to_owned();
        data.toggle_pause();
        assert_eq!(data.entries[0].description, "Forside");
        assert!(data.active.as_ref().unwrap().description.is_empty(), "pausen har sin egen note");

        data.active.as_mut().unwrap().start -= Duration::minutes(5);
        data.toggle_pause();
        assert_eq!(data.active.as_ref().unwrap().description, "Forside", "noten følger med tilbage");

        data.start_call();
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(3);
        active.description = "Spørgsmål om faktura".to_owned();
        data.stop();
        assert_eq!(data.unassigned[0].description, "Spørgsmål om faktura");
        data.unassigned[0].target.project_id = Some(p);
        assert!(data.register_first_unassigned());
        assert_eq!(data.entries.last().unwrap().description, "Spørgsmål om faktura");
    }

    #[test]
    fn pressing_phone_again_ends_call_and_resumes_project() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.start(p);
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(30);
        active.description = "Forside".to_owned();

        data.toggle_call();
        assert!(data.active.as_ref().unwrap().is_call());
        assert_eq!(data.resume_project(), Some(p));
        data.active.as_mut().unwrap().start -= Duration::minutes(5);

        data.toggle_call();
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(p), "tilbage på projektet");
        assert_eq!(active.description, "Forside", "noten er med");
        assert_eq!(active.elapsed().num_minutes(), 30, "uret tæller videre uden opkaldet");
        assert_eq!(data.entries.len(), 1, "projekttiden før opkaldet");
        assert_eq!(data.unassigned.len(), 1, "opkaldet venter på et projekt");
    }

    #[test]
    fn call_during_pause_returns_to_pause_then_project() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.start(p);
        data.active.as_mut().unwrap().start -= Duration::minutes(20);
        data.toggle_pause();
        data.active.as_mut().unwrap().start -= Duration::minutes(5);

        data.toggle_call();
        data.active.as_mut().unwrap().target.project_id = Some(p);
        data.active.as_mut().unwrap().start -= Duration::minutes(2);
        data.toggle_call();
        assert!(data.active.as_ref().unwrap().is_note(NOTE_PAUSE), "tilbage på pausen");
        assert_eq!(data.entries.last().unwrap().note.as_deref(), Some(NOTE_PHONE), "opkaldet registreret");

        data.toggle_pause();
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(p), "og så tilbage på projektet");
        assert_eq!(active.elapsed().num_minutes(), 20);
    }

    #[test]
    fn call_without_running_timer_just_stops() {
        let mut data = Data::default();
        data.toggle_call();
        data.active.as_mut().unwrap().start -= Duration::minutes(3);
        data.toggle_call();
        assert!(data.active.is_none());
        assert_eq!(data.unassigned.len(), 1);
    }

    #[test]
    fn pause_without_previous_project_just_stops() {
        let mut data = Data::default();
        data.toggle_pause();
        data.active.as_mut().unwrap().start -= Duration::minutes(5);
        data.toggle_pause();
        assert!(data.active.is_none());
        assert_eq!(data.entries.len(), 1);
    }

    #[test]
    fn rounds_up_to_next_quarter() {
        let r = |secs| round_up_quarter(Duration::seconds(secs)).num_minutes();
        assert_eq!(r(0), 0);
        assert_eq!(r(1), 15);
        assert_eq!(r(15 * 60), 15);
        assert_eq!(r(15 * 60 + 1), 30);
        assert_eq!(r(47 * 60), 60);
        assert_eq!(r(61 * 60), 75);
        assert_eq!(format_hours_minutes(Duration::minutes(75)), "1:15");
    }

    #[test]
    fn rounding_can_be_toggled_down_per_project_and_day() {
        let mut data = Data::default();
        let day = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        let other_day = day.succ_opt().unwrap();
        let just_over = Duration::seconds(60 * 60 + 1);

        assert_eq!(data.rounded(day, 1, just_over).time, Duration::minutes(75));
        data.toggle_round_down(day, 1);
        let r = data.rounded(day, 1, just_over);
        assert_eq!(r.time, Duration::minutes(60));
        assert!(r.down);
        assert_eq!(data.rounded(other_day, 1, just_over).time, Duration::minutes(75), "kun den ene dag");
        assert_eq!(data.rounded(day, 2, just_over).time, Duration::minutes(75), "kun det ene projekt");

        let exact = data.rounded(day, 1, Duration::minutes(60));
        assert!(!exact.can_toggle && !exact.down, "præcis et kvarter kan ikke rundes");

        data.toggle_round_down(day, 1);
        assert_eq!(data.rounded(day, 1, just_over).time, Duration::minutes(75));
        assert!(data.round_down.is_empty());
    }

    #[test]
    fn customers_are_sorted_with_internal_first() {
        let mut data = Data::default();
        for name in ["Øster", "beta", "Århus", "Alfa", "Ærø", "Zulu"] {
            data.add_customer(name);
        }
        data.internal_project_id(); // opretter "Intern" sidst
        let names: Vec<&str> = data.visible_customers().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Intern", "Alfa", "beta", "Zulu", "Ærø", "Øster", "Århus"]);
    }

    #[test]
    fn day_totals_and_rounded_only_after_rounding_down() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let (a, b) = (data.add_project(c, "A"), data.add_project(c, "B"));
        let day = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let at = |h, m| Local.from_local_datetime(&day.and_hms_opt(h, m, 0).unwrap()).unwrap();
        data.add_entry(TimeEntry { project_id: a, start: at(8, 0), end: at(9, 5), ..Default::default() });
        data.add_entry(TimeEntry { project_id: b, start: at(9, 5), end: at(9, 25), ..Default::default() });
        data.add_entry(TimeEntry { project_id: a, start: at(10, 0), end: at(10, 30), ..Default::default() });

        assert_eq!(data.day_total(day), Duration::minutes(115));
        assert_eq!(data.project_day_total(day, a), Duration::minutes(95));
        assert_eq!(data.rounded_day_total(day), None, "intet rundet ned");

        data.toggle_round_down(day, a); // 1:35 → 1:30 (ned) ; B 0:20 → 0:30 (op)
        assert_eq!(data.rounded_day_total(day), Some(Duration::minutes(120)));
    }

    #[test]
    fn sort_by_name_or_recently_used() {
        let mut data = Data::default();
        let internal = data.internal_project_id();
        let (alfa, beta) = (data.add_customer("Alfa"), data.add_customer("Beta"));
        let (a1, a2) = (data.add_project(alfa, "Zebra"), data.add_project(alfa, "Abe"));
        let b1 = data.add_project(beta, "Bjørn");
        let names = |d: &Data| d.visible_customers().map(|c| c.name.clone()).collect::<Vec<_>>();
        let projects = |d: &Data, c| d.customer_projects(c).iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&data), ["Intern", "Alfa", "Beta"]);
        assert_eq!(projects(&data, alfa), ["Abe", "Zebra"]);

        let now = Local::now();
        let entry = |id, mins: i64| TimeEntry {
            project_id: id,
            start: now - Duration::minutes(mins + 10),
            end: now - Duration::minutes(mins),
            ..Default::default()
        };
        data.add_entry(entry(a1, 60));
        data.add_entry(entry(b1, 30));
        data.add_entry(entry(a2, 90));
        data.settings.sort = SortOrder::Recent;
        assert_eq!(names(&data), ["Intern", "Beta", "Alfa"], "Beta brugt senest; Intern altid øverst");
        assert_eq!(projects(&data, alfa), ["Zebra", "Abe"]);
        let _ = internal;
    }

    #[test]
    fn customer_template_creates_projects_and_auto_start() {
        let mut data = Data::default();
        data.settings.customer_template = vec![
            TemplateProject { name: "Hotline".into(), auto_start: true },
            TemplateProject { name: "Projekt".into(), auto_start: false },
        ];
        let (c, auto) = data.create_customer("Ny kunde");
        let names: Vec<_> = data.customer_projects(c).iter().map(|p| p.name.clone()).collect();
        assert_eq!(names, ["Hotline", "Projekt"]);
        assert_eq!(data.project_label(auto.unwrap()), "Ny kunde / Hotline");

        // Skrevet kunde+projekt i opkaldsdialogen genbruger skabelonens projekt.
        let target = CallTarget { customer: "Anden kunde".into(), project: "hotline".into(), ..Default::default() };
        let id = data.resolve_target(&target).unwrap();
        assert_eq!(data.project_label(id), "Anden kunde / Hotline");
    }

    #[test]
    fn move_active_keeps_the_clock() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let (a, b) = (data.add_project(c, "A"), data.add_project(c, "B"));
        data.start(a);
        data.active.as_mut().unwrap().start -= Duration::minutes(12);
        data.move_active(b);
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(b));
        assert_eq!(active.elapsed().num_minutes(), 12);
        data.stop();
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].project_id, b, "hele tiden på det nye projekt");
    }

    #[test]
    fn suspend_stops_at_sleep_and_resume_continues() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.start(p);
        let started = Local::now() - Duration::minutes(90);
        let active = data.active.as_mut().unwrap();
        active.start = started;
        active.description = "Forside".into();
        // Dvale 30 min efter start, vågner nu.
        let snapshot = data.suspend(started + Duration::minutes(30)).unwrap();
        assert!(data.active.is_none());
        assert_eq!(data.entries[0].duration(), Duration::minutes(30), "dvaletiden tælles ikke med");

        data.resume(snapshot);
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(p));
        assert_eq!(active.description, "Forside");
        assert_eq!(active.elapsed().num_minutes(), 30, "uret fortsætter fra før dvale");
    }

    #[test]
    fn internal_names_follow_the_language_unless_renamed() {
        let mut data = Data::default();
        let internal = data.internal_project_id();
        data.localize_internal_names_in("en");
        assert_eq!(data.project_label(internal), "Internal / Internal time");
        data.localize_internal_names_in("da");
        assert_eq!(data.project_label(internal), "Intern / Intern tid");

        data.rename_project(internal, "Administration").unwrap();
        data.localize_internal_names_in("en");
        assert_eq!(data.project_label(internal), "Internal / Administration", "eget navn bevares");
    }

    #[test]
    fn pause_is_registered_on_internal_project() {
        let mut data = Data::default();
        data.toggle_pause();
        data.active.as_mut().unwrap().start -= Duration::minutes(15);
        data.stop();

        let internal = data.internal_project_id();
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].project_id, internal);
        assert_eq!(data.entries[0].note.as_deref(), Some(NOTE_PAUSE));
        assert_eq!(data.project_label(internal), "Intern / Intern tid");
    }

    #[test]
    fn leftover_active_timer_is_closed_at_last_seen_on_load() {
        let dir = std::env::temp_dir().join(format!("ts-test-{}", std::process::id()));
        let path = dir.join("data.json");
        let start = Local::now() - Duration::hours(10);
        let last_seen = start + Duration::minutes(30);

        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.active = Some(ActiveTimer { project_id: Some(p), start, last_seen, ..Default::default() });
        data.save(&path).unwrap();

        let loaded = Data::load(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        assert!(loaded.active.is_none());
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].duration(), Duration::minutes(30));
    }

    #[test]
    fn customer_from_the_entry_editor_gets_the_template() {
        let mut data = Data::default();
        // Uden skabelon skal der et første projekt til.
        assert_eq!(data.add_customer_from_editor("Acme", ""), None);
        assert!(data.find_customer("Acme").is_none(), "ingen tom kunde");
        let (web, existed) = data.add_customer_from_editor("Acme", "Web").unwrap();
        assert!(!existed);
        assert_eq!(data.project_label(web), "Acme / Web");

        data.settings.customer_template = vec![
            TemplateProject { name: "Support".into(), auto_start: false },
            TemplateProject { name: "Hotline".into(), auto_start: true },
        ];
        let (picked, _) = data.add_customer_from_editor("Ny", "").unwrap();
        assert_eq!(data.project_label(picked), "Ny / Hotline", "projektet med auto start vælges");
        assert!(data.active.is_none(), "men tiden startes ikke");
        assert_eq!(data.customer_projects(data.find_customer("Ny").unwrap()).len(), 2);

        data.settings.customer_template[1].auto_start = false;
        let (picked, _) = data.add_customer_from_editor("Tredje", "").unwrap();
        assert_eq!(data.project_label(picked), "Tredje / Hotline", "ellers det første i listen (sorteret efter navn)");

        // Findes kunden, oprettes den ikke igen.
        let customers = data.customers.len();
        let (picked, existed) = data.add_customer_from_editor(" ny ", "").unwrap();
        assert!(existed);
        assert_eq!(data.customers.len(), customers);
        assert_eq!(data.project_label(picked).split(" / ").next(), Some("Ny"));
    }

    #[test]
    fn project_time_this_week_counts_from_monday() {
        let mut data = Data::default();
        let c = data.add_customer("K");
        let p = data.add_project(c, "A");
        let at = |day, h| Local.with_ymd_and_hms(2026, 11, day, h, 0, 0).unwrap();
        // Søndag 8. (forrige uge), mandag 9. og onsdag 11.
        for day in [8, 9, 11] {
            data.entries.push(TimeEntry { project_id: p, start: at(day, 8), end: at(day, 10), ..Default::default() });
        }
        let thursday = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        assert_eq!(data.project_week_total(p, thursday), Duration::hours(4));
        let tuesday = NaiveDate::from_ymd_opt(2026, 11, 10).unwrap();
        assert_eq!(data.project_week_total(p, tuesday), Duration::hours(2), "kun til og med dagen");
    }

    #[test]
    fn shared_list_adds_only_what_is_missing() {
        let mut data = Data::default();
        let c = data.add_customer("Acme");
        data.add_project(c, "Web");
        let list: Vec<(String, String)> = [("acme", "web"), ("Acme", "App"), ("Ny", ""), ("Ny", "Drift"), ("", "x")]
            .into_iter()
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .collect();
        assert_eq!(data.import_project_list(&list), (1, 2), "Ny + App og Drift");
        assert_eq!(data.import_project_list(&list), (0, 0), "anden gang er alt der");

        // Eksporten indeholder ikke den interne kunde.
        data.internal_project_id();
        let exported = data.project_list();
        assert!(exported.contains(&("Acme".into(), "App".into())));
        assert!(exported.iter().all(|(customer, _)| customer != &data.customers[data.customers.len() - 1].name));
    }

    #[test]
    fn favorites_budget_and_work_time() {
        let mut data = Data::default();
        let c = data.add_customer("K");
        let (a, b) = (data.add_project(c, "B-projekt"), data.add_project(c, "A-projekt"));
        data.toggle_favorite(a);
        data.toggle_favorite(b);
        let names: Vec<&str> = data.favorites().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["A-projekt", "B-projekt"], "sorteret efter navn");
        data.toggle_favorite(a);
        assert_eq!(data.favorites().len(), 1);

        let today = Local::now().date_naive();
        let at = |h, m| today.and_hms_opt(h, m, 0).unwrap().and_local_timezone(Local).unwrap();
        data.entries.push(TimeEntry { project_id: a, start: at(8, 0), end: at(10, 0), ..Default::default() });
        let pause = data.internal_project_id();
        data.entries.push(TimeEntry {
            project_id: pause,
            start: at(10, 0),
            end: at(10, 30),
            note: Some(NOTE_PAUSE.into()),
            ..Default::default()
        });
        assert_eq!(data.work_day_total(today), Duration::hours(2), "pauser tæller ikke med i dagsmålet");

        let mut settings = Settings::default();
        let day = |d| NaiveDate::from_ymd_opt(2026, 11, d).unwrap(); // 9. er en mandag
        assert_eq!(settings.goal_for(day(9)), None, "slået fra som standard");
        settings.daily_goal = true;
        assert_eq!(settings.goal_for(day(9)), Some(Duration::minutes(450)), "mandag 7:30");
        assert_eq!(settings.goal_for(day(13)), Some(Duration::minutes(420)), "fredag 7:00");
        assert_eq!(settings.goal_for(day(14)), None, "intet mål om lørdagen");

        assert_eq!(data.budget(a), None);
        data.set_budget(a, Some(150));
        assert_eq!(data.budget(a), Some((Duration::hours(2), Duration::minutes(150))));
        data.set_budget(a, Some(0));
        assert_eq!(data.budget(a), None, "0 fjerner budgettet");
    }

    #[test]
    fn forgotten_time_starts_back_but_not_before_the_last_entry() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        let now = Local::now();
        let wanted = now - Duration::minutes(20);
        data.start_from(p, wanted);
        assert_eq!(data.active.as_ref().unwrap().start, wanted);

        // Sluttede den seneste registrering for 10 minutter siden, starter tiden dér.
        data.active = None;
        let last_end = now - Duration::minutes(10);
        data.entries.push(TimeEntry { project_id: p, start: now - Duration::hours(1), end: last_end, ..Default::default() });
        data.start_from(p, wanted);
        assert_eq!(data.active.as_ref().unwrap().start, last_end);
        assert_eq!(data.entries.len(), 1, "intet overlap");
    }

    #[test]
    fn gaps_between_entries_are_found() {
        let at = |h, m| Local.with_ymd_and_hms(2026, 11, 12, h, m, 0).unwrap();
        let entry = |from: DateTime<Local>, to| DayEntry {
            index: Some(0),
            entry: TimeEntry { project_id: 1, start: from, end: to, ..Default::default() },
        };
        let entries = [
            entry(at(8, 0), at(10, 0)),
            entry(at(9, 0), at(10, 30)),  // overlapper: dækker til 10:30
            entry(at(10, 33), at(11, 0)), // 3 min: for lidt til at vise
            entry(at(12, 0), at(13, 0)),
        ];
        assert_eq!(Data::gaps(&entries), vec![(at(11, 0), at(12, 0))]);
    }

    #[test]
    fn running_time_can_be_split_onto_another_project() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let (a, b) = (data.add_project(c, "A"), data.add_project(c, "B"));
        let start = Local::now() - Duration::minutes(90);
        data.active = Some(ActiveTimer {
            project_id: Some(a),
            start,
            last_seen: start,
            description: "rettelser".into(),
            ..Default::default()
        });

        let at = start + Duration::minutes(60);
        data.split_active(at, b);
        assert_eq!(data.entries.len(), 1);
        let first = &data.entries[0];
        assert_eq!((first.project_id, first.start, first.end), (a, start, at));
        assert_eq!(first.description, "rettelser", "noten bliver på den første del");
        let active = data.active.as_ref().unwrap();
        assert_eq!((active.project_id, active.start), (Some(b), at));
        assert!(active.elapsed() < Duration::minutes(31), "uret viser kun den flyttede tid");

        // Fra starten: det hele flyttes, intet gemmes.
        let active_start = active.start;
        data.split_active(active_start, a);
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.active.as_ref().unwrap().project_id, Some(a));
    }

    #[test]
    fn timer_keeps_running_after_an_update() {
        let dir = std::env::temp_dir().join(format!("ts-test-update-{}", std::process::id()));
        let path = dir.join("data.json");
        let start = Local::now() - Duration::hours(1);

        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.active = Some(ActiveTimer { project_id: Some(p), start, last_seen: Local::now(), ..Default::default() });
        data.updating = true;
        data.save(&path).unwrap();
        let loaded = Data::load(&path).unwrap();
        assert_eq!(loaded.active.as_ref().map(|a| a.start), Some(start), "kører videre");
        assert!(!loaded.updating);

        // For længe siden: så afsluttes den som efter et nedbrud.
        data.active = Some(ActiveTimer { project_id: Some(p), start, last_seen: start, ..Default::default() });
        data.save(&path).unwrap();
        let loaded = Data::load(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(loaded.active.is_none());
    }

    #[test]
    fn old_data_file_without_new_fields_still_loads() {
        let json = r#"{"next_id":2,"customers":[{"id":1,"name":"K"}],
            "projects":[{"id":2,"customer_id":1,"name":"P"}],
            "entries":[{"project_id":2,"start":"2026-10-20T10:00:00+02:00","end":"2026-10-20T11:00:00+02:00"}],
            "active":null}"#;
        let data: Data = serde_json::from_str(json).unwrap();
        assert_eq!(data.entries.len(), 1);
        assert!(data.unassigned.is_empty());
    }
}
