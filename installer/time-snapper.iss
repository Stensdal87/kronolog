; Inno Setup-script til Windows-installeren. Bygges af GitHub Actions
; (.github/workflows/release.yml) med: ISCC /DAppVersion=x.y.z installer\time-snapper.iss
;
; Installerer kun for den aktuelle bruger, så der ikke kræves administratorrettigheder.
; Data i %APPDATA%\TimeSnapper bliver liggende ved afinstallation og opdatering.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
; AppId må aldrig ændres; den bruges til at genkende en eksisterende installation ved opdatering.
AppId={{421CB1D1-D7AC-49F7-8A0A-E05F04F75468}
AppName=Time Snapper
AppVersion={#AppVersion}
AppVerName=Time Snapper {#AppVersion}
AppPublisher=Stensdal87
AppPublisherURL=https://github.com/Stensdal87/time-snapper
DefaultDirName={localappdata}\Programs\Time Snapper
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=TimeSnapper-Setup-{#AppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\TimeSnapper.exe
UninstallDisplayName=Time Snapper
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Luk et kørende Time Snapper ved opdatering.
CloseApplications=yes
; Vælg sprog efter Windows uden at spørge (spørg kun, hvis intet passer).
ShowLanguageDialog=auto
; Valgene fra sidste installation (f.eks. skrivebordsikon) huskes ved opdatering.
; Automatisk start vises kun ved første installation (se IsUpgrade): derefter
; styres den inde i programmet, og en opdatering må hverken slå den til eller fra.

; Installeren vælger selv sprog efter Windows (dansk ellers engelsk).
[Languages]
Name: "danish"; MessagesFile: "compiler:Languages\Danish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
danish.AutoStart=Start Time Snapper, når jeg logger på Windows
english.AutoStart=Start Time Snapper when I sign in to Windows
danish.Other=Andet:
english.Other=Other:

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "{cm:AutoStart}"; GroupDescription: "{cm:Other}"; Flags: unchecked; Check: not IsUpgrade

[Files]
Source: "..\target\release\time_snapper.exe"; DestDir: "{app}"; DestName: "TimeSnapper.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Time Snapper"; Filename: "{app}\TimeSnapper.exe"
Name: "{autodesktop}\Time Snapper"; Filename: "{app}\TimeSnapper.exe"; Tasks: desktopicon

; Automatisk start ved logon. Programmet selv slår den til og fra samme sted
; (☰ Indstillinger → Start), så de to altid er enige.
[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Time Snapper"; ValueData: """{app}\TimeSnapper.exe"""; Tasks: autostart

; Version 0.1 og 0.2 lagde en genvej i Start-mappen. Programmet flytter den selv
; over til værdien ovenfor ved første start (autostart::migrate_old_shortcut).
[UninstallDelete]
Type: files; Name: "{userstartup}\Time Snapper.lnk"

[Run]
Filename: "{app}\TimeSnapper.exe"; Description: "{cm:LaunchProgram,Time Snapper}"; Flags: nowait postinstall skipifsilent
; Opdatering inde fra programmet (#26): installeren køres stille med /AUTOUPDATE=1,
; og programmet, der lukkede for at blive opdateret, startes igen.
Filename: "{app}\TimeSnapper.exe"; Flags: nowait; Check: IsAutoUpdate

[Code]
// Er Time Snapper installeret i forvejen? (Afinstallationsnøglen fra AppId.)
function IsUpgrade: Boolean;
var
  Key: String;
begin
  Key := 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{421CB1D1-D7AC-49F7-8A0A-E05F04F75468}_is1';
  Result := RegKeyExists(HKEY_CURRENT_USER, Key) or RegKeyExists(HKEY_LOCAL_MACHINE, Key);
end;

// Startet af programmet selv for at opdatere (se src/updater.rs).
function IsAutoUpdate: Boolean;
begin
  Result := ExpandConstant('{param:AUTOUPDATE|0}') = '1';
end;

// Fjern automatisk start ved afinstallation, også hvis den er slået til inde i programmet.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Time Snapper');
end;
