//! Genvejstaster: handlinger, tastekombinationer og brugerens valg.
//!
//! Hver handling kan have en genvej i programmet (virker når Time Snapper har
//! fokus) og en global genvej (virker overalt; se `hotkeys.rs`).

use eframe::egui::{self, Key};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    SwitchProject,
    Pause,
    Call,
    Stop,
    ToggleWindow,
    Fold,
    Search,
    TimerTab,
    HistoryTab,
    Export,
    Help,
}

impl Action {
    pub const ALL: [Action; 11] = [
        Action::SwitchProject,
        Action::Pause,
        Action::Call,
        Action::Stop,
        Action::ToggleWindow,
        Action::Fold,
        Action::Search,
        Action::TimerTab,
        Action::HistoryTab,
        Action::Export,
        Action::Help,
    ];

    pub fn label(self) -> String {
        match self {
            Action::SwitchProject => tr!("action.switch_project"),
            Action::Pause => tr!("action.pause"),
            Action::Call => tr!("action.call"),
            Action::Stop => tr!("action.stop"),
            Action::ToggleWindow => tr!("action.toggle_window"),
            Action::Fold => tr!("action.fold"),
            Action::Search => tr!("action.search"),
            Action::TimerTab => tr!("action.timer_tab"),
            Action::HistoryTab => tr!("action.history_tab"),
            Action::Export => tr!("action.export"),
            Action::Help => tr!("action.help"),
        }
    }

    /// Om handlingen giver mening som global genvej (uden for programmet).
    pub fn can_be_global(self) -> bool {
        matches!(
            self,
            Action::SwitchProject | Action::Pause | Action::Call | Action::Stop | Action::ToggleWindow
        )
    }

    fn default_local(self) -> Option<KeyCombo> {
        let ctrl = |key| Some(KeyCombo { ctrl: true, alt: false, shift: false, key });
        match self {
            Action::SwitchProject => ctrl(Key::K),
            Action::Pause => ctrl(Key::P),
            Action::Call => ctrl(Key::T),
            Action::Stop => ctrl(Key::S),
            Action::ToggleWindow => None,
            Action::Fold => ctrl(Key::M),
            Action::Search => ctrl(Key::F),
            Action::TimerTab => ctrl(Key::Num1),
            Action::HistoryTab => ctrl(Key::Num2),
            Action::Export => ctrl(Key::E),
            Action::Help => Some(KeyCombo { ctrl: false, alt: false, shift: false, key: Key::F1 }),
        }
    }

    fn default_global(self) -> Option<KeyCombo> {
        let ctrl_alt = |key| Some(KeyCombo { ctrl: true, alt: true, shift: false, key });
        match self {
            Action::SwitchProject => ctrl_alt(Key::K),
            Action::Pause => ctrl_alt(Key::P),
            Action::Call => ctrl_alt(Key::T),
            Action::ToggleWindow => ctrl_alt(Key::Space),
            // Stop har ingen global standard, så tiden ikke stoppes ved et uheld.
            _ => None,
        }
    }
}

/// En tastekombination, f.eks. Ctrl+Alt+P. Gemmes som tekst i datafilen.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct KeyCombo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Key,
}

impl KeyCombo {
    pub fn has_modifier(&self) -> bool {
        self.ctrl || self.alt || self.shift
    }

    /// F1–F24 kan bruges alene; andre taster skal have Ctrl, Alt eller Shift.
    pub fn is_function_key(&self) -> bool {
        self.key.name().strip_prefix('F').is_some_and(|n| n.parse::<u8>().is_ok())
    }

    fn matches(&self, key: Key, modifiers: egui::Modifiers) -> bool {
        self.key == key && self.ctrl == modifiers.ctrl && self.alt == modifiers.alt && self.shift == modifiers.shift
    }
}

impl std::fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        let space = tr!("key.space");
        let key = match self.key {
            Key::Space => space.as_str(),
            key => key.name(),
        };
        parts.push(key);
        write!(f, "{}", parts.join("+"))
    }
}

impl From<KeyCombo> for String {
    fn from(combo: KeyCombo) -> Self {
        let mut s = String::new();
        if combo.ctrl {
            s.push_str("Ctrl+");
        }
        if combo.alt {
            s.push_str("Alt+");
        }
        if combo.shift {
            s.push_str("Shift+");
        }
        s.push_str(combo.key.name());
        s
    }
}

impl TryFrom<String> for KeyCombo {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let mut combo = KeyCombo { ctrl: false, alt: false, shift: false, key: Key::A };
        let mut key = None;
        for part in text.split('+') {
            match part {
                "Ctrl" => combo.ctrl = true,
                "Alt" => combo.alt = true,
                "Shift" => combo.shift = true,
                name => key = Key::from_name(name),
            }
        }
        combo.key = key.ok_or_else(|| tr!("key.unknown", shortcut = text))?;
        Ok(combo)
    }
}

/// En handlings genveje som brugeren har valgt dem.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Binding {
    pub action: Action,
    pub local: Option<KeyCombo>,
    pub global: Option<KeyCombo>,
}

/// Alle genveje. Handlinger brugeren ikke har ændret, bruger standarden.
#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(transparent)]
pub struct Shortcuts {
    changed: Vec<Binding>,
}

impl Shortcuts {
    fn binding(&self, action: Action) -> Binding {
        self.changed.iter().find(|b| b.action == action).cloned().unwrap_or(Binding {
            action,
            local: action.default_local(),
            global: action.default_global(),
        })
    }

    pub fn local(&self, action: Action) -> Option<KeyCombo> {
        self.binding(action).local
    }

    pub fn global(&self, action: Action) -> Option<KeyCombo> {
        self.binding(action).global.filter(|_| action.can_be_global())
    }

    pub fn set_local(&mut self, action: Action, combo: Option<KeyCombo>) {
        let mut binding = self.binding(action);
        binding.local = combo;
        self.store(binding);
    }

    pub fn set_global(&mut self, action: Action, combo: Option<KeyCombo>) {
        let mut binding = self.binding(action);
        binding.global = combo;
        self.store(binding);
    }

    fn store(&mut self, binding: Binding) {
        self.changed.retain(|b| b.action != binding.action);
        let is_default =
            binding.local == binding.action.default_local() && binding.global == binding.action.default_global();
        if !is_default {
            self.changed.push(binding);
        }
    }

    pub fn reset(&mut self) {
        self.changed.clear();
    }

    /// Alle globale genveje, der er sat.
    pub fn globals(&self) -> Vec<(Action, KeyCombo)> {
        Action::ALL.iter().filter_map(|&a| Some((a, self.global(a)?))).collect()
    }

    /// Den handling (ud over `except`), der allerede bruger kombinationen.
    pub fn local_conflict(&self, combo: KeyCombo, except: Action) -> Option<Action> {
        Action::ALL.into_iter().find(|&a| a != except && self.local(a) == Some(combo))
    }

    pub fn global_conflict(&self, combo: KeyCombo, except: Action) -> Option<Action> {
        Action::ALL.into_iter().find(|&a| a != except && self.global(a) == Some(combo))
    }

    /// Finder og fjerner tastetryk i programmet, der matcher en genvej.
    /// Genveje uden Ctrl/Alt ignoreres, mens man skriver i et tekstfelt.
    pub fn take_local_actions(&self, ctx: &egui::Context) -> Vec<Action> {
        let typing = ctx.egui_wants_keyboard_input();
        let mut actions = Vec::new();
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let egui::Event::Key { key, pressed: true, modifiers, .. } = event else { return true };
                let hit = Action::ALL.into_iter().find(|&a| {
                    self.local(a)
                        .is_some_and(|c| c.matches(*key, *modifiers) && !(typing && !c.ctrl && !c.alt))
                });
                match hit {
                    Some(action) => {
                        actions.push(action);
                        false
                    }
                    None => true,
                }
            });
        });
        actions
    }
}

/// Læser en ny tastekombination, mens brugeren optager en genvej.
pub enum Recorded {
    Combo(KeyCombo),
    Clear,
    Cancel,
}

pub fn record_key(ctx: &egui::Context) -> Option<Recorded> {
    ctx.input_mut(|input| {
        let mut result = None;
        input.events.retain(|event| {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = event else { return true };
            if result.is_none() {
                result = Some(match key {
                    Key::Escape if !modifiers.any() => Recorded::Cancel,
                    Key::Delete | Key::Backspace if !modifiers.any() => Recorded::Clear,
                    &key => Recorded::Combo(KeyCombo {
                        ctrl: modifiers.ctrl,
                        alt: modifiers.alt,
                        shift: modifiers.shift,
                        key,
                    }),
                });
            }
            false
        });
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_round_trips_through_text() {
        let combo = KeyCombo { ctrl: true, alt: true, shift: false, key: Key::Space };
        let text: String = combo.into();
        assert_eq!(text, "Ctrl+Alt+Space");
        assert_eq!(KeyCombo::try_from(text).unwrap(), combo);
        assert_eq!(combo.to_string(), format!("Ctrl+Alt+{}", tr!("key.space")));
        assert!(KeyCombo::try_from("Ctrl+Hokuspokus".to_owned()).is_err());
    }

    #[test]
    fn defaults_changes_and_conflicts() {
        let mut s = Shortcuts::default();
        let ctrl_k = KeyCombo { ctrl: true, alt: false, shift: false, key: Key::K };
        assert_eq!(s.local(Action::SwitchProject), Some(ctrl_k));
        assert_eq!(s.global(Action::Fold), None, "kun visse handlinger kan være globale");

        let f9 = KeyCombo { ctrl: false, alt: false, shift: false, key: Key::F9 };
        assert!(f9.is_function_key() && !f9.has_modifier());
        s.set_global(Action::Pause, Some(f9));
        assert_eq!(s.global(Action::Pause), Some(f9));
        assert_eq!(s.global_conflict(f9, Action::Call), Some(Action::Pause));
        assert_eq!(s.local_conflict(ctrl_k, Action::Pause), Some(Action::SwitchProject));

        // Gemmes kun det der afviger fra standarden.
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("F9") && !json.contains("SwitchProject"), "{json}");
        let loaded: Shortcuts = serde_json::from_str(&json).unwrap();
        assert!(loaded == s);

        s.set_global(Action::Pause, Action::Pause.default_global());
        assert!(s == Shortcuts::default(), "tilbage til standard gemmes ikke");
    }
}
