#ifndef ProductVersion
  #error ProductVersion is required (for example, v002)
#endif
#ifndef StageDir
  #error StageDir is required
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif

[Setup]
AppId={{D168A6D4-1A3C-4775-A736-3F88A12DCD14}
AppName=CellTray
AppVersion={#ProductVersion}
AppVerName=CellTray {#ProductVersion}
AppPublisher=Baz Studios
AppPublisherURL=https://github.com/Baz-Studios-LLC/CellTray
AppSupportURL=https://github.com/Baz-Studios-LLC/CellTray/issues
AppUpdatesURL=https://github.com/Baz-Studios-LLC/CellTray/releases
DefaultDirName={localappdata}\Programs\CellTray
DefaultGroupName=CellTray
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
WizardStyle=modern
SetupIconFile=..\assets\icons\celltray.ico
UninstallDisplayIcon={app}\celltray-widgets.exe
UninstallDisplayName=CellTray {#ProductVersion}
LicenseFile=..\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=CellTray-{#ProductVersion}-Setup-x64
Compression=lzma2
SolidCompression=yes
AppMutex=Local\CellTrayWidgets
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; Flags: unchecked
Name: "startup"; Description: "Start CellTray when I sign in to &Windows"; Flags: unchecked

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\CellTray"; Filename: "{app}\celltray-widgets.exe"
Name: "{group}\Uninstall CellTray"; Filename: "{uninstallexe}"
Name: "{userdesktop}\CellTray"; Filename: "{app}\celltray-widgets.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "CellTray"; ValueData: """{app}\celltray-widgets.exe"""; Tasks: startup; Flags: uninsdeletevalue

[Run]
Filename: "{app}\celltray-widgets.exe"; Description: "Launch CellTray"; Flags: nowait postinstall skipifsilent
