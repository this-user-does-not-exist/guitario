; Guitario's Windows installer (Inno Setup 6). `cargo xtask package` compiles it when ISCC is
; installed, with /DVersion=… and /DSource=<folder with guitario.exe>; GitHub's Windows runners
; have Inno Setup. Per-user by default (no administrator), or for everyone if chosen.

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Source
  #define Source "..\..\target\release"
#endif
#ifndef Output
  #define Output "..\..\dist"
#endif

[Setup]
AppId={{6E1C4C7B-5B2F-4D8C-9A57-2F1C0E8B4A31}
AppName=Guitario
AppVersion={#Version}
AppPublisher=Guitario contributors
AppPublisherURL=https://github.com/timothydaly1/guitario
DefaultDirName={autopf}\Guitario
DefaultGroupName=Guitario
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
OutputDir={#Output}
OutputBaseFilename=Guitario-{#Version}-windows-x86_64-setup
SetupIconFile=..\..\assets\app-icon\guitario.ico
UninstallDisplayIcon={app}\guitario.exe
LicenseFile=..\..\LICENSE-MIT
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Files]
Source: "{#Source}\guitario.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\guitario-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE-MIT"; DestDir: "{app}"
Source: "..\..\LICENSE-APACHE"; DestDir: "{app}"
Source: "..\..\NOTICE"; DestDir: "{app}"

[Icons]
Name: "{group}\Guitario"; Filename: "{app}\guitario.exe"
Name: "{autodesktop}\Guitario"; Filename: "{app}\guitario.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "gpdefault"; Description: "Open Guitar Pro files (.gp, .gp5 …) with Guitario by default"; GroupDescription: "File types:"; Flags: unchecked

[Registry]
; Guitario's own scores open with it.
Root: HKA; Subkey: "Software\Classes\.gt"; ValueType: string; ValueName: ""; ValueData: "Guitario.Score"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gt"; ValueType: string; ValueName: ""; ValueData: "Guitario.Score"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Guitario.Score"; ValueType: string; ValueName: ""; ValueData: "Guitario Score"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Guitario.Score\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\guitario.exe,0"
Root: HKA; Subkey: "Software\Classes\Guitario.Score\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\guitario.exe"" ""%1"""
; Guitar Pro, MIDI and MusicXML files list Guitario under Open With; Guitar Pro's become
; Guitario's by default only if asked.
Root: HKA; Subkey: "Software\Classes\Guitario.Tablature"; ValueType: string; ValueName: ""; ValueData: "Tablature"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Guitario.Tablature\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\guitario.exe,0"
Root: HKA; Subkey: "Software\Classes\Guitario.Tablature\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\guitario.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.gp\OpenWithProgids"; ValueType: string; ValueName: "Guitario.Tablature"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gpx\OpenWithProgids"; ValueType: string; ValueName: "Guitario.Tablature"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gp5\OpenWithProgids"; ValueType: string; ValueName: "Guitario.Tablature"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gp4\OpenWithProgids"; ValueType: string; ValueName: "Guitario.Tablature"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gp3\OpenWithProgids"; ValueType: string; ValueName: "Guitario.Tablature"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mid\OpenWithProgids"; ValueType: string; ValueName: "Guitario.Tablature"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.musicxml\OpenWithProgids"; ValueType: string; ValueName: "Guitario.Tablature"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.mxl\OpenWithProgids"; ValueType: string; ValueName: "Guitario.Tablature"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gp"; ValueType: string; ValueName: ""; ValueData: "Guitario.Tablature"; Tasks: gpdefault; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gp5"; ValueType: string; ValueName: ""; ValueData: "Guitario.Tablature"; Tasks: gpdefault; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.gpx"; ValueType: string; ValueName: ""; ValueData: "Guitario.Tablature"; Tasks: gpdefault; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".gt"; ValueData: ""; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".gt"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".gp"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".gpx"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".gp5"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".gp4"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".gp3"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".mid"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".musicxml"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\guitario.exe\SupportedTypes"; ValueType: string; ValueName: ".mxl"; ValueData: ""

[Run]
Filename: "{app}\guitario.exe"; Description: "{cm:LaunchProgram,Guitario}"; Flags: nowait postinstall skipifsilent
