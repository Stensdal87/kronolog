; Inno Setup-script til Windows-installeren. Bygges af GitHub Actions
; (.github/workflows/release.yml) med: ISCC /DAppVersion=x.y.z installer\kronolog.iss
;
; Installerer kun for den aktuelle bruger, så der ikke kræves administratorrettigheder.
; Data i %APPDATA%\KronoLog bliver liggende ved afinstallation og opdatering.
;
; Programmet hed tidligere "Time Snapper". AppId er det samme, så KronoLog
; opdaterer den eksisterende installation (i dens mappe) i stedet for at lægge
; sig ved siden af; den gamle exe, genvejene og den gamle automatiske start
; fjernes (se [InstallDelete] og CurStepChanged). Programmet flytter selv
; dataene fra %APPDATA%\TimeSnapper ved første start.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
; AppId må aldrig ændres; den bruges til at genkende en eksisterende installation ved opdatering.
AppId={{421CB1D1-D7AC-49F7-8A0A-E05F04F75468}
AppName=KronoLog
AppVersion={#AppVersion}
AppVerName=KronoLog {#AppVersion}
AppPublisher=Stensdal87
AppPublisherURL=https://github.com/Stensdal87/kronolog
DefaultDirName={localappdata}\Programs\KronoLog
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=KronoLog-Setup-{#AppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\KronoLog.exe
UninstallDisplayName=KronoLog
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Luk et kørende KronoLog (eller Time Snapper) ved opdatering.
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
danish.AutoStart=Start KronoLog, når jeg logger på Windows
english.AutoStart=Start KronoLog when I sign in to Windows
danish.Other=Andet:
english.Other=Other:

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "{cm:AutoStart}"; GroupDescription: "{cm:Other}"; Flags: unchecked; Check: not IsUpgrade

[Files]
Source: "..\target\release\kronolog.exe"; DestDir: "{app}"; DestName: "KronoLog.exe"; Flags: ignoreversion

; Fra før navneskiftet: den gamle exe og genvejene "Time Snapper".
[InstallDelete]
Type: files; Name: "{app}\TimeSnapper.exe"
Type: files; Name: "{autoprograms}\Time Snapper.lnk"
Type: files; Name: "{autodesktop}\Time Snapper.lnk"

[Icons]
Name: "{autoprograms}\KronoLog"; Filename: "{app}\KronoLog.exe"
Name: "{autodesktop}\KronoLog"; Filename: "{app}\KronoLog.exe"; Tasks: desktopicon

; Automatisk start ved logon. Programmet selv slår den til og fra samme sted
; (☰ Indstillinger → Start), så de to altid er enige.
[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "KronoLog"; ValueData: """{app}\KronoLog.exe"""; Tasks: autostart

; Version 0.1 og 0.2 lagde en genvej i Start-mappen. Programmet flytter den selv
; over til værdien ovenfor ved første start (autostart::migrate_old_shortcut).
[UninstallDelete]
Type: files; Name: "{userstartup}\Time Snapper.lnk"

[Run]
Filename: "{app}\KronoLog.exe"; Description: "{cm:LaunchProgram,KronoLog}"; Flags: nowait postinstall skipifsilent
; Opdatering inde fra programmet (#26): installeren køres stille med /AUTOUPDATE=1,
; og programmet, der lukkede for at blive opdateret, startes igen.
Filename: "{app}\KronoLog.exe"; Flags: nowait; Check: IsAutoUpdate

[Code]
// Er KronoLog (eller Time Snapper) installeret i forvejen? (Afinstallationsnøglen fra AppId.)
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

// Var automatisk start slået til under det gamle navn, peger den på den
// slettede TimeSnapper.exe. Flyt den over til KronoLog, så programmet stadig
// starter ved næste logon, også selvom det ikke åbnes efter opdateringen.
procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and RegValueExists(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Time Snapper') then
  begin
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Time Snapper');
    RegWriteStringValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'KronoLog', '"' + ExpandConstant('{app}\KronoLog.exe') + '"');
  end;
end;

// Fjern automatisk start ved afinstallation, også hvis den er slået til inde i programmet.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'KronoLog');
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Time Snapper');
  end;
end;
