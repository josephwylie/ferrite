#ifndef MyAppVersion
  #error MyAppVersion must be provided by package-windows.ps1
#endif
#ifndef MyAppBinary
  #error MyAppBinary must be provided by package-windows.ps1
#endif
#ifndef MyAppRoot
  #error MyAppRoot must be provided by package-windows.ps1
#endif
#ifndef MyAppOutputDir
  #error MyAppOutputDir must be provided by package-windows.ps1
#endif
#ifndef MyAppOutputBaseFilename
  #error MyAppOutputBaseFilename must be provided by package-windows.ps1
#endif

[Setup]
AppId={{A8963198-15B0-4DA5-841F-860DE3DBE544}
AppName=Ferrite
AppVersion={#MyAppVersion}
AppPublisher=Ferrite contributors
AppPublisherURL=https://github.com/josephwylie/ferrite
AppSupportURL=https://github.com/josephwylie/ferrite/issues
AppUpdatesURL=https://github.com/josephwylie/ferrite/releases/latest
VersionInfoVersion={#MyAppVersion}
VersionInfoProductName=Ferrite
VersionInfoDescription=Ferrite installer
VersionInfoCompany=Ferrite contributors
DefaultDirName={autopf}\Ferrite
DefaultGroupName=Ferrite
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#MyAppOutputDir}
OutputBaseFilename={#MyAppOutputBaseFilename}
SetupIconFile={#MyAppRoot}\crates\ferrite\assets\app-icon.ico
UninstallDisplayIcon={app}\ferrite.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
SignTool=authenticode
SignedUninstaller=yes
LicenseFile={#MyAppRoot}\LICENSE-MIT

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "{#MyAppBinary}"; DestDir: "{app}"; DestName: "ferrite.exe"; Flags: ignoreversion

[Icons]
Name: "{group}\Ferrite"; Filename: "{app}\ferrite.exe"
Name: "{autodesktop}\Ferrite"; Filename: "{app}\ferrite.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\ferrite.exe"; Description: "Launch Ferrite"; Flags: nowait postinstall skipifsilent
