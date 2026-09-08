; Txtify — Inno Setup installer for Windows
; Requires Inno Setup 6.x
; Builds installer that ships pure-Rust binary (<15MB) + optional sidecar/

#define MyAppName "Txtify"
#define MyAppVersion "0.1.2"
#define MyAppPublisher "Txtify Contributors"
#define MyAppURL "https://github.com/jeremymiribung-blip/txtify"
#define MyAppExeName "txtify.exe"

[Setup]
AppId={{8E2B1F4A-7C9D-4E6F-9A2B-1F4A7C9D4E6F}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
LicenseFile=..\..\LICENSE
OutputDir=..\..\target\installer
OutputBaseFilename=txtify-{#MyAppVersion}-windows-x86_64-setup
Compression=lzma
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
SetupIconFile=..\..\sidecar\README.md
UninstallDisplayIcon={app}\{#MyAppExeName}
ChangesEnvironment=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "addtopath"; Description: "Add txtify to PATH"; GroupDescription: "Additional options:"; Flags: unchecked
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "..\..\target\release\txtify.exe"; DestDir: "{app}"; Flags: ignoreversion
; Sidecar is optional — embed if present (GLM-OCR ~1GB not bundled by default)
Source: "..\..\sidecar\txtify_sidecar.py"; DestDir: "{app}\sidecar"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\..\sidecar\requirements.txt"; DestDir: "{app}\sidecar"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\..\sidecar\README.md"; DestDir: "{app}\sidecar"; Flags: ignoreversion skipifsourcedoesntexist
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE*"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Comment: "Hybrid document-to-text converter"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; Add to PATH if user opts-in (HKCU, no admin)
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{olddata};{app}"; Tasks: addtopath; Check: NeedsAddPath('{app}')

[Run]
Filename: "{app}\{#MyAppExeName}"; Parameters: "doctor"; Description: "Run 'txtify doctor' to check system"; Flags: postinstall skipifsilent nowait
Filename: "{app}\{#MyAppExeName}"; Parameters: "shell status"; Description: "Check shell integration status"; Flags: postinstall skipifsilent nowait

[UninstallRun]
; Clean shell integration on uninstall (HKCU)
Filename: "{app}\{#MyAppExeName}"; Parameters: "shell uninstall"; Flags: runhidden; RunOnceId: "CleanShell"

[Code]
function NeedsAddPath(Param: string): boolean;
var
  OrigPath: string;
begin
  if not RegQueryStringValue(HKEY_CURRENT_USER, 'Environment', 'Path', OrigPath) then
    Result := True
  else
    Result := Pos(';' + Uppercase(Param) + ';', ';' + Uppercase(OrigPath) + ';') = 0;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    WizardForm.StatusLabel.Caption := 'Txtify installed. Run "txtify doctor" to verify dependencies.';
end;
