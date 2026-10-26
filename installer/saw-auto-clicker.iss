; The per-user installer, carrying both builds of the app and installing the
; one native to the machine. CI compiles it once from both build targets
; (.github/workflows/build.yml):
;
;   iscc /DAppVersion=<version> /DAppExeX64=<path to the x64 saw-app.exe>
;        /DAppExeArm64=<path to the arm64 saw-app.exe>
;        /O<output dir> /Fsaw-auto-clicker-installer installer\saw-auto-clicker.iss

#ifndef AppVersion
  #error Pass the version: /DAppVersion=<workspace version>
#endif
#ifndef AppExeX64
  #error Pass the x64 app: /DAppExeX64=<path to saw-app.exe>
#endif
#ifndef AppExeArm64
  #error Pass the arm64 app: /DAppExeArm64=<path to saw-app.exe>
#endif

; x64os leaves out Windows on ARM, which could run the x64 build only under
; emulation; it gets the arm64 build instead.
#define Architectures "x64os arm64"

#define AppName "saw-auto-clicker"
; A stable name, so an upgrade replaces the exe in place and the start-at-login
; value keeps pointing at it.
#define AppExeName "saw-auto-clicker.exe"
; The value the app's Start at login writes, under the per-user Run key
; (crates/app/src/win/autostart.rs).
#define RunKey "Software\Microsoft\Windows\CurrentVersion\Run"
#define RunValue "saw-auto-clicker"

[Setup]
; The AppId the earlier per-architecture installers shared, so this one
; upgrades either of them in place.
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
ArchitecturesAllowed={#Architectures}
ArchitecturesInstallIn64BitMode={#Architectures}
; The app's single-instance mutex (crates/app/src/win/instance.rs): setup and
; uninstall ask the user to quit a running copy first.
AppMutex=Local\saw-auto-clicker
SetupIconFile=..\crates\app\assets\saw-auto-clicker.ico
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2
SolidCompression=yes

[Messages]
; Where a new user learns that the app lives in the tray.
FinishedLabel=Setup has finished installing [name] on your computer.%n%n[name] runs in the system tray, by the clock, as a saw-blade icon. Click it to enable or disable auto-clicking, change settings, or quit. Windows may hide a new icon behind the ^ arrow; drag it onto the taskbar to keep it in view.
FinishedLabelNoIcons=Setup has finished installing [name] on your computer.%n%n[name] runs in the system tray, by the clock, as a saw-blade icon. Click it to enable or disable auto-clicking, change settings, or quit. Windows may hide a new icon behind the ^ arrow; drag it onto the taskbar to keep it in view.

[Tasks]
Name: startatlogin; Description: "Start at login"; Flags: unchecked

[Files]
; Only x64os and arm64 are allowed, so a machine that is not Arm64 is x64.
Source: "{#AppExeX64}"; DestDir: "{app}"; DestName: "{#AppExeName}"; Check: not IsArm64; Flags: ignoreversion
Source: "{#AppExeArm64}"; DestDir: "{app}"; DestName: "{#AppExeName}"; Check: IsArm64; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"

[Registry]
; The same string the app writes (crates/app/src/win/autostart.rs), so its tray
; menu shows Start at login checked and a start at sign-in stays in the tray.
; Uninstall removes it only when it still names this install (below).
Root: HKCU; Subkey: "{#RunKey}"; ValueType: string; ValueName: "{#RunValue}"; ValueData: """{app}\{#AppExeName}"" --at-login"; Tasks: startatlogin

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: postinstall nowait skipifsilent

[Code]
// Leaves a value pointing at another copy, such as a portable exe, alone.
// CompareText ignores ASCII case, as the app's own comparison does. Versions
// before 0.5.0 wrote the path without --at-login.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command, Quoted: String;
begin
  Quoted := '"' + ExpandConstant('{app}\{#AppExeName}') + '"';
  if (CurUninstallStep = usUninstall)
    and RegQueryStringValue(HKCU, '{#RunKey}', '{#RunValue}', Command)
    and ((CompareText(Command, Quoted + ' --at-login') = 0)
      or (CompareText(Command, Quoted) = 0)) then
    RegDeleteValue(HKCU, '{#RunKey}', '{#RunValue}');
end;
