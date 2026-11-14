; Dockview installer (Inno Setup 6). Build it with tools\build_installer.ps1.
;
; It installs for the current user by default (no administrator needed, into
; %LOCALAPPDATA%\Programs\Dockview); the wizard offers "for all users" too.
; Settings and the log live in %APPDATA%\Dockview and are never touched by an
; update; uninstalling asks whether to delete them.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\dockview.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
; Never change this id: it is how Windows knows an update from another program.
AppId={{26C04BD3-242C-49E2-A47B-9EB9D0FC3522}
AppName=Dockview
AppVersion={#AppVersion}
AppVerName=Dockview {#AppVersion}
AppPublisher=Dockview
DefaultDirName={autopf}\Dockview
DefaultGroupName=Dockview
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#OutputDir}
OutputBaseFilename=Dockview-Setup-{#AppVersion}
SetupIconFile=..\crates\dockview\assets\icon.ico
UninstallDisplayIcon={app}\Dockview.exe
UninstallDisplayName=Dockview
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; An update replaces the running program: close it first, start it again after.
CloseApplications=yes
RestartApplications=yes
VersionInfoVersion={#AppVersion}
VersionInfoProductName=Dockview
MinVersion=10.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "Dockview.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Dockview"; Filename: "{app}\Dockview.exe"
Name: "{autodesktop}\Dockview"; Filename: "{app}\Dockview.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\Dockview.exe"; Description: "{cm:LaunchProgram,Dockview}"; Flags: nowait postinstall skipifsilent

[Code]
// On uninstall, the settings and the log stay unless the user says otherwise
// (a silent uninstall never deletes them).
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  DataDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    DataDir := ExpandConstant('{userappdata}\Dockview');
    if DirExists(DataDir) and not UninstallSilent() then
      if MsgBox('Delete the Dockview settings and log too?' + #13#10 + DataDir,
                mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(DataDir, True, True, True);
  end;
end;
