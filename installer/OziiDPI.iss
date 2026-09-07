#define MyAppName "OziiDPI"
#define MyAppVersion "1.1.1"
#define MyAppPublisher "ozii"
#define MyAppExeName "OziiDPI.exe"

[Setup]
AppId={{D820AA0D-8A46-4CB5-92D4-A2C8163293E4}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription={#MyAppName} Kurulum
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}
VersionInfoVersion={#MyAppVersion}
DefaultDirName={localappdata}\Programs\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist\installer
OutputBaseFilename=OziiDPI-Setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#MyAppExeName}
CloseApplications=force
RestartApplications=no
SetupLogging=yes

[Languages]
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[Files]
Source: "..\dist\product\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\OziiDPI"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\OziiDPI"; Filename: "{app}\{#MyAppExeName}"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "OziiDPI"; ValueData: """{app}\{#MyAppExeName}"" --startup"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Google\Chrome\NativeMessagingHosts\com.ozii.dpi"; ValueType: string; ValueName: ""; ValueData: "{app}\chrome-native-host.json"; Flags: uninsdeletekey

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "OziiDPI'yi şimdi çalıştır"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{app}\ozii-cli.exe"; Parameters: "stop"; Flags: runhidden waituntilterminated skipifdoesntexist; RunOnceId: "StopBackend"

[Code]
function InitializeSetup(): Boolean;
begin
  Result := True;
end;
