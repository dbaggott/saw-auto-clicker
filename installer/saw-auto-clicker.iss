; The per-user installer for one architecture. CI compiles it once per build
; target (.github/workflows/build.yml):
;
;   iscc /DAppVersion=<version> /DArch=x64|arm64 /DAppExe=<path to saw-app.exe>
;        /O<output dir> /Fsaw-auto-clicker-installer installer\saw-auto-clicker.iss

#ifndef AppVersion
  #error Pass the version: /DAppVersion=<workspace version>
#endif
#ifndef AppExe
  #error Pass the app: /DAppExe=<path to saw-app.exe>
#endif
#ifndef Arch
  #error Pass the architecture: /DArch=x64 or /DArch=arm64
#endif

; Each installer runs only on the architecture its exe was built for. x64os
; leaves out Windows on ARM, where the x64 build would run under emulation
; when a native build exists.
#if Arch == "x64"
  #define Architecture "x64os"
#elif Arch == "arm64"
  #define Architecture "arm64"
#else
  #error Arch must be x64 or arm64
#endif

#define AppName "saw-auto-clicker"
; A stable name, so an upgrade replaces the exe in place and the start-at-login
; value keeps pointing at it.
#define AppExeName "saw-auto-clicker.exe"
; The value the app's Start at login writes, under the per-user Run key
; (crates/app/src/win/autostart.rs).
#define RunKey "Software\Microsoft\Windows\CurrentVersion\Run"
#define RunValue "saw-auto-clicker"

[Setup]
; One AppId for both architectures, so installing either upgrades whichever
; is already there rather than adding a second entry.
AppId={{D675A522-B891-431A-ABDF-927F9B5987FD}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisherURL=https://github.com/dbaggott/saw-auto-clicker
; The app never needs administrator rights, so neither does installing it.
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
; Start at login stays unticked on an upgrade too, so reinstalling never turns
; back on what the user turned off in the tray menu.
UsePreviousTasks=no
ArchitecturesAllowed={#Architecture}
ArchitecturesInstallIn64BitMode={#Architecture}
; The app's single-instance mutex (crates/app/src/win/instance.rs): setup and
; uninstall ask the user to quit a running copy first.
AppMutex=Local\saw-auto-clicker
SetupIconFile=..\crates\app\assets\saw-auto-clicker.ico
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2
SolidCompression=yes

[Tasks]
Name: startatlogin; Description: "Start at login"; Flags: unchecked

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; DestName: "{#AppExeName}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"

[Registry]
; The same string the app writes, quoted, so its tray menu shows Start at login
; checked. Uninstall removes it only when it still names this install (below).
Root: HKCU; Subkey: "{#RunKey}"; ValueType: string; ValueName: "{#RunValue}"; ValueData: """{app}\{#AppExeName}"""; Tasks: startatlogin

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: postinstall nowait skipifsilent

[Code]
// Leaves a value pointing at another copy, such as a portable exe, alone.
// CompareText ignores ASCII case, as the app's own comparison does.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if (CurUninstallStep = usUninstall)
    and RegQueryStringValue(HKCU, '{#RunKey}', '{#RunValue}', Command)
    and (CompareText(Command, '"' + ExpandConstant('{app}\{#AppExeName}') + '"') = 0) then
    RegDeleteValue(HKCU, '{#RunKey}', '{#RunValue}');
end;
