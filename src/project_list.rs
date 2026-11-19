//! Deling af kunde- og projektlisten mellem kolleger som CSV: én linje pr.
//! projekt, `Kunde;Projekt;Kundelink;Projektlink` (links fra #41; filer med
//! kun de to første kolonner kan stadig indlæses). Filen kan også laves eller
//! rettes i Excel. Ved indlæsning accepteres semikolon, komma og tabulator,
//! med eller uden overskrift og citationstegn.

/// Én linje i den delte liste. Kundens link står på hver af kundens linjer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListRow {
    pub customer: String,
    pub project: String,
    pub customer_link: String,
    pub project_link: String,
}

impl ListRow {
    #[cfg(test)]
    pub fn new(customer: &str, project: &str) -> Self {
        Self { customer: customer.to_owned(), project: project.to_owned(), ..Default::default() }
    }
}

/// Listen som CSV med overskrift. Semikolon, så Excel med dansk opsætning
/// åbner den i kolonner.
pub fn to_csv(list: &[ListRow]) -> String {
    let mut csv = format!(
        "{};{};{};{}\r\n",
        tr!("common.customer"),
        tr!("common.project"),
        tr!("share.col_customer_link"),
        tr!("share.col_project_link")
    );
    for row in list {
        let fields = [&row.customer, &row.project, &row.customer_link, &row.project_link];
        csv.push_str(&fields.map(|f| quote(f)).join(";"));
        csv.push_str("\r\n");
    }
    csv
}

fn quote(field: &str) -> String {
    if field.contains([';', ',', '"', '\n']) { format!("\"{}\"", field.replace('"', "\"\"")) } else { field.to_owned() }
}

/// Linjerne i en CSV-fil. Tomme linjer og en overskrift ("Kunde;Projekt…"
/// eller "Customer;Project…") springes over; manglende kolonner er tomme.
pub fn parse_csv(text: &str) -> Vec<ListRow> {
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
        .map(|fields| {
            let field = |i: usize| fields.get(i).map(|f| f.trim().to_owned()).unwrap_or_default();
            ListRow { customer: field(0), project: field(1), customer_link: field(2), project_link: field(3) }
        })
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
            ListRow {
                customer_link: "https://firma.my.salesforce.com/a?x=1;y=2".into(),
                project_link: "https://firma.my.salesforce.com/p,1".into(),
                ..ListRow::new("Kunde A", "Support")
            },
            ListRow::new("Kunde; B", "Projekt \"X\""),
            ListRow::new("Kunde C", ""),
        ];
        assert_eq!(parse_csv(&to_csv(&list)), list);
    }

    #[test]
    fn reads_files_made_in_excel_with_other_separators() {
        let comma = "Customer,Project\nAcme,Web\n\nAcme,App\n";
        assert_eq!(parse_csv(comma), vec![ListRow::new("Acme", "Web"), ListRow::new("Acme", "App")]);
        let tabs = "\u{FEFF}Kunde\tProjekt\nFirma\tDrift\nKun kunde\n";
        assert_eq!(parse_csv(tabs), vec![ListRow::new("Firma", "Drift"), ListRow::new("Kun kunde", "")]);
    }
}
