# KronoLog ⏱

Et lille, simpelt tidsregistreringsprogram til at tage tid på projekter for kunder,
med plads til ro i arbejdsdagen. Det kan ligge øverst på skærmen uden at være i vejen.

Navnet er et ordspil: en *log* over tiden og *kronologi*. Programmet hed tidligere
Time Snapper; opdaterer du derfra, flytter KronoLog selv dine data over.

## Funktioner

**Tid**
- Kunder og projekter. Klik på et projekt for at starte tiden; når du skifter projekt, gemmes tiden automatisk
- Rundt ur med den aktuelle tidstagning og dagens tid, og en menu med dagens og projektets samlede tid
- **📞 Telefon** og **☕ Pause**, der fortsætter på projektet fra før, når de afsluttes
- Tag telefonen automatisk, når f.eks. Teams bruger mikrofonen
- Flyt den kørende tid til et andet projekt (⇄), også kun den sidste del, hvis du glemte at skifte
- "Glemt at starte tiden?": en påmindelse, når du arbejder uden at tage tid, og tiden kan startes bagud
- Dagsmål pr. ugedag som en grøn ring i uret, og timebudget pr. projekt med advarsel
- ⭐ Favoritter øverst i listen

**Historik**
- Pr. dag (summeret eller alle registreringer) og pr. uge som et timeregnskab
- Afrunding til kvarter, afkrydsning når tiden er overført, og huller i dagen, der kan udfyldes
- Ret, slet, tilføj og del registreringer op; noter på tidsregistreringer
- Registreringer for en enkelt kunde eller et projekt (højreklik)
- Eksportér en periode til Excel (.xlsx) eller CSV

**Overblik og ro**
- Kan foldes sammen til en lille bjælke, vandret eller lodret i siden af skærmen
- Guidede åndedrætsøvelser i pauserne, egne øvelser og point for dage i træk
- Pausepåmindelse, og spørger om tiden skal være pause, når du har været væk

**Praktisk**
- Hjælp (F1) med søgning og "Vidste du…?"-tips
- Ikon i systembakken med en rød prik, mens tiden kører
- Genvejstaster, også globale på Windows
- Daglig sikkerhedskopi af dine data (de sidste 14 dage)
- Del kunde- og projektlisten med kolleger som en CSV-fil
- Søger selv efter nye versioner og installerer dem på Windows
- Dansk og engelsk, lyst/mørkt tema, "hold øverst" og automatisk start ved logon

## Byg og kør

Kræver [Rust](https://rustup.rs/).

```sh
cargo run --release
```

Programmet er skrevet med [egui](https://github.com/emilk/egui) og kører på både Linux og Windows.

## Installation

Hent den nyeste version under [Releases](https://github.com/Stensdal87/kronolog/releases):

- **Windows:** `KronoLog-Setup-<version>.exe` (installerer for din bruger, uden administratorrettigheder)
- **Linux:** `KronoLog-<version>-linux-x86_64.tar.gz`. Pak den ud og kør `./install.sh` (installerer for din bruger i `~/.local/bin`, uden root)

## Data

Alt gemmes lokalt i én JSON-fil, med daglige sikkerhedskopier i mappen `backups` ved siden af:

- Linux: `~/.local/share/kronolog/data.json`
- Windows: `%APPDATA%\KronoLog\data\data.json`

## Licens

[MIT](LICENSE)
