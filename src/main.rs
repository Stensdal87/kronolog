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
mod backup;
mod data;
mod entry_editor;
mod export;
mod export_dialog;
mod hotkeys;
mod i18n;
mod idle;
mod mic;
mod project_list;
mod shortcuts;
mod single_instance;
mod tray;
mod updater;

use eframe::egui;

const APP_NAME: &str = "KronoLog";

/// Programmets tidligere navne, nyeste først: navnet i datastien og navnet,
/// eframe gemte vinduesindstillingerne under.
const OLD_NAMES: [(&str, &str); 2] = [("TimeSnapper", "Time Snapper"), ("TimeTracker", "Time Tracker")];

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
    // Linux: app-id'et kobler vinduet til kronolog.desktop (ikon i proceslinjen).
    // Det bestemmer også eframes gemmemappe; "kronolog" er den samme som datamappen.
    #[cfg(target_os = "linux")]
    let options = eframe::NativeOptions {
        viewport: options.viewport.with_app_id("kronolog"),
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
        Box::new(|cc| Ok(Box::new(app::KronoLogApp::new(&cc.egui_ctx, path).with_instance(instance)))),
    )
}

/// Programmet hed tidligere "Time Snapper" og før det "Time Tracker". Findes
/// der ingen KronoLog-data endnu, kopieres datamappen (data, sikkerhedskopier,
/// opkaldslog) og vinduesindstillingerne fra det nyeste gamle navn. De gamle
/// filer bliver liggende som en ekstra sikkerhedskopi.
fn migrate_from_old_name() {
    let new_data = data::Data::default_path();
    if new_data.exists() {
        return;
    }
    for (data_name, window_name) in OLD_NAMES {
        let old_data = data::Data::path_for(data_name);
        let (Some(old_dir), Some(new_dir)) = (old_data.parent(), new_data.parent()) else { return };
        if !old_data.exists() {
            continue;
        }
        let _ = copy_dir(old_dir, new_dir);
        // På Windows ligger vinduesindstillingerne for sig (på Linux er de i datamappen).
        if let (Some(old), Some(new)) = (eframe::storage_dir(window_name), eframe::storage_dir(APP_NAME)) {
            let (old, new) = (old.join("app.ron"), new.join("app.ron"));
            if old.exists() && !new.exists() {
                let _ = std::fs::create_dir_all(new.parent().unwrap_or(&new));
                let _ = std::fs::copy(&old, &new);
            }
        }
        return;
    }
}

/// Kopierer en mappe med undermapper. Filerne, der viser om programmet kører
/// (`instance.*`), kommer ikke med; de hører til den gamle kopi.
fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if !entry.file_name().to_string_lossy().starts_with("instance.") && !target.exists() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use eframe::egui::FontFamily;
    use eframe::egui::epaint::text::{FontDefinitions, Fonts, TextOptions};

    /// Alle tegn i programmets tekster skal findes i de indbyggede skrifttyper.
    /// Ellers henter egui dem fra systemet, og på Windows bliver de ofte til
    /// tomme firkanter (det skete med ☰ og 🗕), selvom de ser fine ud på Linux.
    #[test]
    fn old_data_folder_is_copied_with_backups_but_not_instance_files() {
        let root = std::env::temp_dir().join(format!("kl-migrate-{}", std::process::id()));
        let (old, new) = (root.join("timesnapper"), root.join("kronolog"));
        std::fs::create_dir_all(old.join("backups")).unwrap();
        for file in ["data.json", "app.ron", "mic-log.txt", "instance.lock", "instance.port", "backups/data-2026-11-11.json"] {
            std::fs::write(old.join(file), file).unwrap();
        }
        super::copy_dir(&old, &new).unwrap();
        for file in ["data.json", "app.ron", "mic-log.txt", "backups/data-2026-11-11.json"] {
            assert_eq!(std::fs::read_to_string(new.join(file)).unwrap(), file);
        }
        assert!(!new.join("instance.lock").exists() && !new.join("instance.port").exists());
        assert!(old.join("data.json").exists(), "de gamle filer bliver liggende");
        std::fs::remove_dir_all(&root).ok();
    }

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
