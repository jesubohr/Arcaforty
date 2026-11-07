#ifndef AppVersion
  #error AppVersion must be supplied by build-installer.ps1
#endif
#ifndef RuntimeDll
  #error RuntimeDll must be supplied by build-installer.ps1
#endif

[Setup]
AppId={{75F2BB5C-C615-43BC-8FE8-2E123DF50B54}
AppName=Arcaforty
AppVersion={#AppVersion}
VersionInfoVersion={#AppVersion}
VersionInfoProductVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\Arcaforty
DefaultGroupName=Arcaforty
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
OutputDir=..\dist
OutputBaseFilename=Arcaforty-Setup-{#AppVersion}-x64
SetupIconFile=..\assets\logo.ico
UninstallDisplayIcon={app}\arcaforty.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\target\release\arcaforty.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#RuntimeDll}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\staging\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Arcaforty"; Filename: "{app}\arcaforty.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Arcaforty"; Filename: "{app}\arcaforty.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\arcaforty.exe"; Description: "Open Arcaforty"; Flags: nowait postinstall skipifsilent unchecked
