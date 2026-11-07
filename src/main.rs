// Skjul konsolvinduet på Windows i release-builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Oversættelser i locales/app.yml (dansk og engelsk side om side). Mangler en
// tekst på et sprog, bruges den danske.
rust_i18n::i18n!("locales", fallback = "da");

/// Oversat tekst som `String` (klar til egui): `tr!("nøgle")`, eller
/// `tr!("nøgle", navn = værdi)` til pladsholderen `%{navn}` i teksten.
macro_rules! tr {
    ($($arg:tt)*) => {
        rust_i18n::t!($($arg)*).to_string()
    };
}

mod app;
mod autostart;
mod data;
mod entry_editor;
mod export;
mod export_dialog;
mod hotkeys;
mod i18n;
mod idle;
mod mic;
mod shortcuts;
mod single_instance;
mod tray;
mod updater;

use eframe::egui;

const APP_NAME: &str = "Time Snapper";

fn main() -> eframe::Result {
    migrate_from_old_name();
    let path = data::Data::default_path();
    // Kører programmet allerede, er det nu bedt om at vise sig; denne kopi stopper.
    let Some(instance) = single_instance::start(&single_instance::port_file(&path)) else {
        return Ok(());
    };
    // Stod bjælken fremme ved lukning, oprettes vinduet uden titellinje fra
    // starten; fjernes den bagefter, kryber bjælken op.
    let bar = data::Data::load(&path).is_ok_and(|d| d.compact);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(!bar)
            .with_title(APP_NAME)
            .with_inner_size([320.0, 520.0])
            .with_min_inner_size([260.0, 300.0])
            // Et lille værktøjsvindue; maksimeret giver det ingen mening (#14).
            .with_maximize_button(false)
            .with_icon(tray::app_icon())
            .with_always_on_top(),
        ..Default::default()
    };
    // Wayland lader ikke programmer lægge sig øverst, så på Linux kører vi via
    // X11/XWayland, hvor vinduesmanageren respekterer det.
    // Linux: app-id'et kobler vinduet til timesnapper.desktop (ikon i proceslinjen).
    // Det bestemmer også eframes gemmemappe; "timesnapper" giver den samme som før.
    #[cfg(target_os = "linux")]
    let options = eframe::NativeOptions {
        viewport: options.viewport.with_app_id("timesnapper"),
        ..options
    };
    #[cfg(target_os = "linux")]
    let options = eframe::NativeOptions {
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        })),
        ..options
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::TimeSnapperApp::new(&cc.egui_ctx, path).with_instance(instance)))),
    )
}

/// Programmet hed tidligere "Time Tracker". Kopiér data og vinduesindstillinger
/// til de nye placeringer, hvis de ikke findes der endnu. De gamle filer
/// bliver liggende som sikkerhedskopi.
fn migrate_from_old_name() {
    let mut files = vec![(data::Data::path_for("TimeTracker"), data::Data::default_path())];
    if let (Some(old), Some(new)) = (eframe::storage_dir("Time Tracker"), eframe::storage_dir(APP_NAME)) {
        files.push((old.join("app.ron"), new.join("app.ron")));
    }
    for (old, new) in files {
        if old.exists() && !new.exists() {
            if let Some(dir) = new.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::copy(&old, &new);
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::FontFamily;
    use eframe::egui::epaint::text::{FontDefinitions, Fonts, TextOptions};

    /// Alle tegn i programmets tekster skal findes i de indbyggede skrifttyper.
    /// Ellers henter egui dem fra systemet, og på Windows bliver de ofte til
    /// tomme firkanter (det skete med ☰ og 🗕), selvom de ser fine ud på Linux.
    #[test]
    fn all_ui_characters_are_in_bundled_fonts() {
        let mut fonts = Fonts::new(TextOptions::default(), FontDefinitions::default());
        let mut view = fonts.with_pixels_per_point(1.0);
        // Tegn i tekst med fast bredde (f.eks. "1:00 ↓") kommer fra Hack.
        let mut available = view.characters(&FontFamily::Proportional).clone();
        available.extend(view.characters(&FontFamily::Monospace).clone());

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = vec![root.join("src"), root.join("locales")];
        let mut missing = Vec::new();
        while let Some(path) = files.pop() {
            if path.is_dir() {
                files.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs" && e != "yml") {
                continue;
            }
            for (n, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                // Kun tegn inde i strenge ("…" eller '…').
                let mut in_string = false;
                for c in line.chars() {
                    if c == '"' {
                        in_string = !in_string;
                    } else if in_string && !c.is_ascii() && !available.contains_key(&c) && c != '\u{FE0F}' {
                        missing.push(format!("{c} (U+{:04X}) i {}:{}", c as u32, path.display(), n + 1));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "Tegn uden indbygget skrifttype:\n{}", missing.join("\n"));
    }
}
