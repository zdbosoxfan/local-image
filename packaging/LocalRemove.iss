#ifndef PackageDir
  #define PackageDir "..\dist\package"
#endif
#ifndef InstallerDir
  #define InstallerDir "..\dist\installer"
#endif
#ifndef PrerequisiteDir
  #define PrerequisiteDir "..\dist\prerequisites"
#endif

[Setup]
AppId=LocalRemove.Windows
AppName=Local Remove
AppVersion=0.3.1
AppPublisher=Local Remove
AppPublisherURL=https://github.com/zdbosoxfan/local-remove
DefaultDirName={localappdata}\Programs\Local Remove
DefaultGroupName=Local Remove
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#InstallerDir}
OutputBaseFilename=Local-Remove-Setup-0.3.1
SetupIconFile=..\desktop\icon\local-remove.ico
UninstallDisplayIcon={app}\Local Remove.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=no
RestartApplications=no
DisableProgramGroupPage=yes
ChangesAssociations=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#PrerequisiteDir}\MicrosoftEdgeWebView2Setup.exe"; Flags: dontcopy
Source: "{#PackageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Local Remove"; Filename: "{app}\Local Remove.exe"
Name: "{group}\AI connection settings"; Filename: "{app}\Local Remove.exe"; Parameters: "--configure"
Name: "{autodesktop}\Local Remove"; Filename: "{app}\Local Remove.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Classes\.lremove\OpenWithProgids"; ValueType: string; ValueName: "LocalRemove.Project"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\LocalRemove.Project"; ValueType: string; ValueData: "Local Remove editable project"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\LocalRemove.Project\DefaultIcon"; ValueType: string; ValueData: "{app}\Local Remove.exe,0"
Root: HKCU; Subkey: "Software\Classes\LocalRemove.Project\shell\open\command"; ValueType: string; ValueData: """{app}\Local Remove.exe"" ""%1"""

[Run]
Filename: "{app}\Local Remove.exe"; Description: "Open Local Remove"; Flags: nowait postinstall skipifsilent

[Code]
function HasWebView2(): Boolean;
var
  Version: String;
begin
  Result := RegQueryStringValue(HKLM32, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version)
            and (Version <> '') and (Version <> '0.0.0.0');
  if not Result then
    Result := RegQueryStringValue(HKCU, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version)
              and (Version <> '') and (Version <> '0.0.0.0');
end;

function StopInstalledBackend(const Directory: String): Boolean;
var
  ResultCode: Integer;
begin
  Result := True;
  if FileExists(Directory + '\Local Remove.exe') then
    Result := Exec(Directory + '\Local Remove.exe', '--shutdown-backend', Directory,
                   SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Result := '';
  if CheckForMutexes('Local\LocalRemoveDesktop') then
    Result := 'Save your edits and close Local Remove before installing this update.'
  else if not StopInstalledBackend(ExpandConstant('{app}')) then
    Result := 'Local Remove is still working. Wait for it to finish, then retry.';
  if (Result = '') and not HasWebView2() then begin
    ExtractTemporaryFile('MicrosoftEdgeWebView2Setup.exe');
    if not Exec(ExpandConstant('{tmp}\MicrosoftEdgeWebView2Setup.exe'), '/silent /install', '',
                SW_HIDE, ewWaitUntilTerminated, ResultCode) or not HasWebView2() then
      Result := 'Microsoft WebView2 could not be installed. Connect to the internet and retry setup.';
  end;
end;

function InitializeUninstall(): Boolean;
begin
  Result := False;
  if CheckForMutexes('Local\LocalRemoveDesktop') then begin
    MsgBox('Save your edits and close Local Remove before uninstalling.', mbInformation, MB_OK);
    exit;
  end;
  Result := StopInstalledBackend(ExpandConstant('{app}'));
  if not Result then MsgBox('Local Remove is still working. Wait for it to finish, then retry.', mbInformation, MB_OK);
end;

// User settings, recovery sessions, and saved projects are deliberately retained.
