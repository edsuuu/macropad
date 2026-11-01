; Instalador do Macropad: o app (macropad.exe) por usuário, sem administrador, em %LOCALAPPDATA%\Programs\Macropad,
; com atalho no Iniciar. Gerado por tools\build-installer.ps1 em target\installer\Macropad-Setup.exe, com a versão do
; Cargo.toml passada em /DVersion.
#ifndef Version
  #define Version "0.0.0"
#endif

[Setup]
AppId=edsuuu.Macropad
AppName=Macropad
AppVersion={#Version}
AppPublisher=edsuuu
AppPublisherURL=https://github.com/edsuuu/macropad
DefaultDirName={autopf}\Macropad
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\target\installer
OutputBaseFilename=Macropad-Setup
SetupIconFile=..\assets\icon.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=Macropad
UninstallDisplayIcon={app}\macropad.exe

[Languages]
Name: "ptbr"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "desktopicon"; Description: "Atalho na área de trabalho"; Flags: unchecked

[Files]
Source: "..\target\release\macropad.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Macropad"; Filename: "{app}\macropad.exe"
Name: "{autodesktop}\Macropad"; Filename: "{app}\macropad.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\macropad.exe"; Description: "Abrir o Macropad"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; O escutador dos comandos (macropad.exe --escutar) fica rodando e no início do Windows: para e sai do Run.
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM macropad.exe"; Flags: runhidden; RunOnceId: "Parar"
Filename: "{sys}\reg.exe"; Parameters: "delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Macropad /f"; Flags: runhidden; RunOnceId: "TirarDoInicio"
