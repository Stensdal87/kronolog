//! Daglige sikkerhedskopier af datafilen.
//!
//! Første gang der gemmes på en dag, kopieres den gemte datafil (gårsdagens
//! sidste tilstand) til `backups/data-ÅÅÅÅ-MM-DD.json` ved siden af den. De
//! nyeste `KEEP` kopier beholdes. Før en gendannelse gemmes den nuværende fil
//! som `data-før-gendannelse-…json`, som ikke slettes af oprydningen.

use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate};

/// Så mange daglige kopier beholdes.
pub const KEEP: usize = 14;

const PREFIX: &str = "data-";

/// En sikkerhedskopi på disken.
#[derive(Clone, Debug, PartialEq)]
pub struct Backup {
    pub date: NaiveDate,
    pub path: PathBuf,
}

/// Mappen med kopierne.
pub fn folder(data_path: &Path) -> PathBuf {
    data_path.with_file_name("backups")
}

/// Laver dagens kopi, hvis den ikke findes, og rydder op i gamle. Kaldes før
/// der gemmes, så kopien er filen, som den var, før dagens ændringer.
pub fn daily(data_path: &Path, today: NaiveDate) -> Result<(), String> {
    if !data_path.exists() {
        return Ok(());
    }
    let dir = folder(data_path);
    let target = dir.join(format!("{PREFIX}{}.json", today.format("%Y-%m-%d")));
    if target.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::copy(data_path, &target).map_err(|e| e.to_string())?;
    for old in list(data_path).into_iter().skip(KEEP) {
        let _ = std::fs::remove_file(old.path);
    }
    Ok(())
}

/// De daglige kopier, nyeste først.
pub fn list(data_path: &Path) -> Vec<Backup> {
    let Ok(entries) = std::fs::read_dir(folder(data_path)) else { return Vec::new() };
    let mut backups: Vec<Backup> = entries
        .filter_map(|e| {
            let path = e.ok()?.path();
            let name = path.file_name()?.to_str()?;
            let date = name.strip_prefix(PREFIX)?.strip_suffix(".json")?;
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            Some(Backup { date, path })
        })
        .collect();
    backups.sort_by_key(|b| std::cmp::Reverse(b.date));
    backups
}

/// Gemmer den nuværende datafil, før en kopi gendannes oven i den.
pub fn save_before_restore(data_path: &Path) -> Result<PathBuf, String> {
    let dir = folder(data_path);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = dir.join(format!("data-før-gendannelse-{}.json", Local::now().format("%Y-%m-%d_%H%M%S")));
    std::fs::copy(data_path, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_copy_per_day_and_only_the_newest_are_kept() {
        let dir = std::env::temp_dir().join(format!("ts-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.join("data.json");
        let day = |n| NaiveDate::from_ymd_opt(2026, 11, 1).unwrap() + chrono::Duration::days(n);

        // Ingen datafil endnu: intet at kopiere.
        daily(&data, day(0)).unwrap();
        assert!(list(&data).is_empty());

        std::fs::write(&data, "dag 0").unwrap();
        daily(&data, day(0)).unwrap();
        std::fs::write(&data, "ændret samme dag").unwrap();
        daily(&data, day(0)).unwrap();
        let backups = list(&data);
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read_to_string(&backups[0].path).unwrap(), "dag 0", "dagens første tilstand");

        for n in 1..20 {
            daily(&data, day(n)).unwrap();
        }
        let backups = list(&data);
        assert_eq!(backups.len(), KEEP);
        assert_eq!(backups[0].date, day(19), "nyeste først");
        assert_eq!(backups[KEEP - 1].date, day(20 - KEEP as i64));

        let before = save_before_restore(&data).unwrap();
        daily(&data, day(30)).unwrap();
        assert!(before.exists(), "kopien før en gendannelse ryddes ikke op");
        std::fs::remove_dir_all(&dir).ok();
    }
}
