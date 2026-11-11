//! Deling af kunde- og projektlisten mellem kolleger som CSV: én linje pr.
//! projekt, `Kunde;Projekt`. Filen kan også laves eller rettes i Excel.
//! Ved indlæsning accepteres semikolon, komma og tabulator, med eller uden
//! overskrift og citationstegn.

/// Listen som CSV med overskrift. Semikolon, så Excel med dansk opsætning
/// åbner den i to kolonner.
pub fn to_csv(list: &[(String, String)]) -> String {
    let mut csv = format!("{};{}\r\n", tr!("common.customer"), tr!("common.project"));
    for (customer, project) in list {
        csv.push_str(&format!("{};{}\r\n", quote(customer), quote(project)));
    }
    csv
}

fn quote(field: &str) -> String {
    if field.contains([';', ',', '"', '\n']) { format!("\"{}\"", field.replace('"', "\"\"")) } else { field.to_owned() }
}

/// Par af (kunde, projekt) fra en CSV-fil. Tomme linjer og en overskrift
/// ("Kunde;Projekt" eller "Customer;Project") springes over.
pub fn parse_csv(text: &str) -> Vec<(String, String)> {
    let text = text.trim_start_matches('\u{FEFF}');
    let delimiter = [';', '\t', ',']
        .into_iter()
        .find(|d| text.lines().next().is_some_and(|l| l.contains(*d)))
        .unwrap_or(';');
    let headers = ["kunde", "customer"];
    text.lines()
        .map(|line| split(line, delimiter))
        .filter(|fields| !fields.is_empty() && !fields[0].trim().is_empty())
        .filter(|fields| !headers.contains(&fields[0].trim().to_lowercase().as_str()))
        .map(|fields| (fields[0].trim().to_owned(), fields.get(1).map(|p| p.trim().to_owned()).unwrap_or_default()))
        .collect()
}

/// Felterne i en linje; tekst i citationstegn kan indeholde skilletegnet.
fn split(line: &str, delimiter: char) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                fields.last_mut().expect("altid ét felt").push('"');
            }
            '"' => quoted = !quoted,
            c if c == delimiter && !quoted => fields.push(String::new()),
            c => fields.last_mut().expect("altid ét felt").push(c),
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_survives_a_round_trip() {
        let list = vec![
            ("Kunde A".to_owned(), "Support".to_owned()),
            ("Kunde; B".to_owned(), "Projekt \"X\"".to_owned()),
            ("Kunde C".to_owned(), String::new()),
        ];
        assert_eq!(parse_csv(&to_csv(&list)), list);
    }

    #[test]
    fn reads_files_made_in_excel_with_other_separators() {
        let comma = "Customer,Project\nAcme,Web\n\nAcme,App\n";
        assert_eq!(
            parse_csv(comma),
            vec![("Acme".into(), "Web".into()), ("Acme".into(), "App".into())]
        );
        let tabs = "\u{FEFF}Kunde\tProjekt\nFirma\tDrift\nKun kunde\n";
        assert_eq!(parse_csv(tabs), vec![("Firma".into(), "Drift".into()), ("Kun kunde".into(), String::new())]);
    }
}
