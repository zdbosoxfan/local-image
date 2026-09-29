#ifndef PackageDir
  #define PackageDir "..\dist\package"
#endif
#ifndef InstallerDir
  #define InstallerDir "..\dist\installer"
#endif
#ifndef PrerequisiteDir
  #define PrerequisiteDir "..\dist\prerequisites"
#endif
#ifndef AppIdentity
  #define AppIdentity "LocalRemove.Windows"
#endif
#ifndef ProjectIdentity
  #define ProjectIdentity "LocalRemove.Project"
#endif
#ifndef AppPathLimit
  #define AppPathLimit 155
#endif

[Setup]
AppId={#AppIdentity}
AppName=Local Image
AppVersion=0.6.0
AppPublisher=Local Image
AppPublisherURL=https://github.com/zdbosoxfan/local-remove
DefaultDirName={autopf}\Local Image
DefaultGroupName=Local Image
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#InstallerDir}
OutputBaseFilename=Local-Image-Setup-0.6.0
SetupIconFile=..\desktop\icon\local-image.ico
UninstallDisplayIcon={app}\Local Image.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=no
RestartApplications=no
DisableProgramGroupPage=yes
AllowNoIcons=yes
DisableDirPage=no
ChangesAssociations=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#PrerequisiteDir}\MicrosoftEdgeWebView2Setup.exe"; Flags: dontcopy
#ifndef PlanTest
Source: "{#PackageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
#endif

[InstallDelete]
Type: files; Name: "{app}\Local Remove.exe"
Type: files; Name: "{autodesktop}\Local Remove.lnk"; Check: LegacyShortcutTargetsApp('{autodesktop}\Local Remove.lnk')
Type: files; Name: "{autoprograms}\Local Remove\Local Remove.lnk"; Check: LegacyShortcutTargetsApp('{autoprograms}\Local Remove\Local Remove.lnk')
Type: files; Name: "{autoprograms}\Local Remove\AI connection settings.lnk"; Check: LegacyShortcutTargetsApp('{autoprograms}\Local Remove\AI connection settings.lnk')

[UninstallDelete]
Type: files; Name: "{app}\installation-defaults.json"

[Icons]
Name: "{group}\Local Image"; Filename: "{app}\Local Image.exe"
Name: "{group}\AI connection settings"; Filename: "{app}\Local Image.exe"; Parameters: "--configure"
Name: "{autodesktop}\Local Image"; Filename: "{app}\Local Image.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.lremove\OpenWithProgids"; ValueType: string; ValueName: "{#ProjectIdentity}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\{#ProjectIdentity}"; ValueType: string; ValueData: "Local Image editable project"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\{#ProjectIdentity}\DefaultIcon"; ValueType: string; ValueData: "{app}\Local Image.exe,0"
Root: HKA; Subkey: "Software\Classes\{#ProjectIdentity}\shell\open\command"; ValueType: string; ValueData: """{app}\Local Image.exe"" ""%1"""

[Run]
Filename: "{app}\Local Image.exe"; Description: "Open Local Image"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Code]
#ifdef PlanTest
function InitializeSetup(): Boolean;
begin
  Result := ExpandConstant('{param:PLANONLY|0}') = '1';
end;
#endif
var
  AiChoice: TInputOptionWizardPage;
  StorageChoice: TInputOptionWizardPage;
  StorageFolders: TWizardPage;
  StorageEdits: array[0..1] of TNewEdit;
  StorageButtons: array[0..1] of TNewButton;
  RequestedMode: String;
  RequestedStorage: array[0..1] of String;

function JsonString(Value: String): String;
begin
  StringChangeEx(Value, '\', '\\', True);
  StringChangeEx(Value, '"', '\"', True);
  StringChangeEx(Value, #13, '\r', True);
  StringChangeEx(Value, #10, '\n', True);
  Result := '"' + Value + '"';
end;

function SetupMode(): String;
begin
  if AiChoice.SelectedValueIndex = 1 then Result := 'portable'
  else if AiChoice.SelectedValueIndex = 2 then Result := 'later'
  else Result := 'discover';
end;

function FolderValue(Index: Integer): String;
begin
  Result := Trim(StorageEdits[Index].Text);
  if (Length(Result) >= 2) and (Copy(Result, 1, 1) = '"') and
     (Copy(Result, Length(Result), 1) = '"') then Result := Copy(Result, 2, Length(Result) - 2);
end;

function StorageFolder(Index: Integer): String;
begin
  Result := '';
  if StorageChoice.SelectedValueIndex = 1 then begin
    Result := FolderValue(Index);
    if Result <> '' then Result := RemoveBackslashUnlessRoot(ExpandFileName(Result));
  end;
end;

function InstallationDefaults(): String;
begin
  Result := '{"schema":1,"setup_mode":' + JsonString(SetupMode()) +
    ',"model_directory":' + JsonString(StorageFolder(0)) +
    ',"managed_ai_directory":' + JsonString(StorageFolder(1)) + '}';
end;

function InsideFolder(const Path, Parent: String): Boolean;
begin
  Result := (CompareText(RemoveBackslashUnlessRoot(Path), RemoveBackslashUnlessRoot(Parent)) = 0) or
    (CompareText(Copy(AddBackslash(Path), 1, Length(AddBackslash(Parent))), AddBackslash(Parent)) = 0);
end;

function LegacyShortcutTargetsApp(const Shortcut: String): Boolean;
var
  Shell, Link: Variant;
  Target: String;
begin
  Result := False;
  if not FileExists(ExpandConstant(Shortcut)) then exit;
  try
    Shell := CreateOleObject('WScript.Shell');
    Link := Shell.CreateShortcut(ExpandConstant(Shortcut));
    Target := Link.TargetPath;
    Target := Trim(Target);
    if Target = '' then exit;
    Target := ExpandFileName(Target);
    // A separate installation must not remove another checkout's shortcuts.
    Result := InsideFolder(Target, ExpandConstant('{app}'));
  except
    Log('Could not inspect a legacy shortcut; leaving it unchanged.');
  end;
end;

function ValidateStorageSyntax(Value: String): String;
var
  Character: Integer;
begin
  Result := '';
  Value := Trim(Value);
  if (Length(Value) >= 2) and (Value[1] = '"') and (Value[Length(Value)] = '"') then
    Value := Copy(Value, 2, Length(Value) - 2);
  if Value = '' then exit;
  if Length(Value) < 3 then begin
    Result := 'Choose an absolute local drive folder for models and ComfyUI.';
    exit;
  end;
  if (Value[2] <> #58) or (Value[3] <> #92) or
     not (((Value[1] >= 'A') and (Value[1] <= 'Z')) or ((Value[1] >= 'a') and (Value[1] <= 'z'))) then begin
    Result := 'Choose an absolute local drive folder for models and ComfyUI.';
    exit;
  end;
  // Check Unicode characters directly; Pos with an ANSI literal can
  // otherwise treat non-Latin folder names as question marks.
  for Character := 1 to Length(Value) do
    if (Value[Character] = #34) or (Value[Character] = #60) or (Value[Character] = #62) or
       (Value[Character] = #124) or (Value[Character] = #42) or (Value[Character] = #63) or
       (Ord(Value[Character]) < 32) or ((Character <> 2) and (Value[Character] = #58)) then begin
      Result := 'The AI folder name contains an invalid character.';
      exit;
    end;
end;

function ValidateStorage(): String;
var
  Index: Integer;
  Value: String;
begin
  Result := '';
  if (RequestedMode <> '') and (RequestedMode <> 'discover') and
     (RequestedMode <> 'portable') and (RequestedMode <> 'later') then begin
    Result := 'AISETUP must be discover, portable or later.';
    exit;
  end;
  for Index := 0 to 1 do begin
    // Inno's directory page normalizes assigned values. Check raw command-line
    // input first so drive-relative paths cannot silently become another folder.
    Result := ValidateStorageSyntax(RequestedStorage[Index]);
    if Result <> '' then exit;
    Value := FolderValue(Index);
    if (StorageChoice.SelectedValueIndex = 1) and (Value <> '') then begin
      Result := ValidateStorageSyntax(Value);
      if Result <> '' then exit;
      Value := StorageFolder(Index);
      if InsideFolder(Value, WizardDirValue()) or InsideFolder(Value, ExpandConstant('{commonpf}')) or
         InsideFolder(Value, ExpandConstant('{commonpf32}')) or InsideFolder(Value, ExpandConstant('{win}')) then begin
        Result := 'Choose AI storage outside Program Files, Windows and the app installation folder. Leave a field blank to use each user''s AppData.';
        exit;
      end;
    end;
  end;
end;

function ValidateApplicationFolder(): String;
begin
  Result := '';
  // Leave room for bundled library/license paths under Windows' directory limit.
  if Length(RemoveBackslashUnlessRoot(ExpandFileName(WizardDirValue()))) > {#AppPathLimit} then
    Result := 'Choose a shorter application folder (' + IntToStr({#AppPathLimit}) + ' characters or fewer including the drive). Bundled libraries need room for their subfolders. Models and ComfyUI can use separate storage folders.';
end;

procedure BrowseStorageFolder(Sender: TObject);
var
  Index: Integer;
  Directory, Prompt: String;
begin
  if Sender = StorageButtons[0] then Index := 0 else Index := 1;
  Directory := FolderValue(Index);
  if Index = 0 then Prompt := 'Choose a folder for AI model files'
  else Prompt := 'Choose a parent folder for portable ComfyUI';
  if BrowseForFolder(Prompt, Directory, True) then StorageEdits[Index].Text := Directory;
end;

procedure InitializeWizard();
var
  ReportPath, Report, Scope: String;
  Index: Integer;
  Prompt, Hint: TNewStaticText;
begin
  AiChoice := CreateInputOptionPage(wpSelectDir, 'Local AI setup', 'Choose how to connect AI tools',
    'Model weights are separate downloads. You can finish this setup after opening the app.', True, False);
  AiChoice.Add('Find an existing ComfyUI installation on this PC');
  AiChoice.Add('Set up a dedicated portable ComfyUI installation');
  AiChoice.Add('Set up AI later; start with local editing');
  RequestedMode := ExpandConstant('{param:AISETUP|}');
  AiChoice.SelectedValueIndex := 0;
  if RequestedMode = 'portable' then AiChoice.SelectedValueIndex := 1;
  if RequestedMode = 'later' then AiChoice.SelectedValueIndex := 2;
  StorageChoice := CreateInputOptionPage(AiChoice.ID, 'AI storage', 'Choose where large AI files will live',
    'App settings, recovery images and browser data always stay in the launching user''s AppData. Existing user settings are preserved during updates.', True, False);
  StorageChoice.Add('Use each user''s AppData folders (recommended)');
  StorageChoice.Add('Choose model and portable ComfyUI folders');
  StorageChoice.SelectedValueIndex := 0;
  // Inno's standard directory page requires every field to contain a path.
  // A custom page keeps either field optional and resolves defaults per user.
  StorageFolders := CreateCustomPage(StorageChoice.ID, 'AI folders', 'Choose folders on this PC');
  for Index := 0 to 1 do begin
    Prompt := TNewStaticText.Create(WizardForm);
    Prompt.Parent := StorageFolders.Surface;
    Prompt.Left := 0;
    Prompt.Top := ScaleY(16 + Index * 78);
    if Index = 0 then Prompt.Caption := 'Model files:'
    else Prompt.Caption := 'Portable ComfyUI parent folder:';
    Prompt.AutoSize := True;
    StorageEdits[Index] := TNewEdit.Create(WizardForm);
    StorageEdits[Index].Parent := StorageFolders.Surface;
    StorageEdits[Index].Left := 0;
    StorageEdits[Index].Top := Prompt.Top + Prompt.Height + ScaleY(8);
    StorageEdits[Index].Width := StorageFolders.SurfaceWidth - ScaleX(100);
    StorageEdits[Index].Height := ScaleY(23);
    StorageEdits[Index].TabOrder := Index * 2;
    StorageButtons[Index] := TNewButton.Create(WizardForm);
    StorageButtons[Index].Parent := StorageFolders.Surface;
    StorageButtons[Index].Left := StorageFolders.SurfaceWidth - ScaleX(88);
    StorageButtons[Index].Top := StorageEdits[Index].Top;
    StorageButtons[Index].Width := ScaleX(88);
    StorageButtons[Index].Height := StorageEdits[Index].Height;
    StorageButtons[Index].Caption := 'Browse...';
    StorageButtons[Index].TabOrder := Index * 2 + 1;
    StorageButtons[Index].OnClick := @BrowseStorageFolder;
  end;
  Hint := TNewStaticText.Create(WizardForm);
  Hint.Parent := StorageFolders.Surface;
  Hint.Left := 0;
  Hint.Top := ScaleY(180);
  Hint.Width := StorageFolders.SurfaceWidth;
  Hint.Height := StorageFolders.SurfaceHeight - Hint.Top;
  Hint.AutoSize := False;
  Hint.WordWrap := True;
  Hint.Caption := 'Choose writable local folders. Leave either field blank to use each user''s AppData. These choices apply to new profiles; existing users can change folders in Settings.';
  RequestedStorage[0] := ExpandConstant('{param:MODELDIR|}');
  RequestedStorage[1] := ExpandConstant('{param:AIDIR|}');
  StorageEdits[0].Text := RequestedStorage[0];
  StorageEdits[1].Text := RequestedStorage[1];
  if (StorageEdits[0].Text <> '') or (StorageEdits[1].Text <> '') then StorageChoice.SelectedValueIndex := 1;
  // Plan-only mode validates the real wizard/CLI choices without installing,
  // registering associations or changing the existing app. Used by release QA.
  if ExpandConstant('{param:PLANONLY|0}') = '1' then begin
    ReportPath := ExpandConstant('{param:REPORT|}');
    if ReportPath = '' then RaiseException('PLANONLY requires REPORT.');
    if IsAdminInstallMode() then Scope := 'all-users' else Scope := 'current-user';
    Report := ValidateStorage();
    if Report = '' then Report := ValidateApplicationFolder();
    Report := '{"installation_directory":' + JsonString(WizardDirValue()) +
      ',"scope":' + JsonString(Scope) + ',"error":' + JsonString(Report) +
      ',"validation_inputs":{"model":' + JsonString(FolderValue(0)) + ',"model_prefix":' + JsonString(Copy(FolderValue(0), 2, 2)) +
      ',"runtime":' + JsonString(FolderValue(1)) + ',"runtime_prefix":' + JsonString(Copy(FolderValue(1), 2, 2)) + '}' +
      ',"defaults":' + InstallationDefaults() + '}';
    ForceDirectories(ExtractFileDir(ReportPath));
    if not SaveStringToFile(ReportPath, Utf8Encode(Report), False) then RaiseException('Could not write the setup plan.');
    Abort();
  end;
end;

function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := (PageID = StorageFolders.ID) and (StorageChoice.SelectedValueIndex = 0);
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  Error: String;
begin
  Result := True;
  if (CurPageID = StorageFolders.ID) or (CurPageID = wpSelectDir) then begin
    if CurPageID = wpSelectDir then Error := ValidateApplicationFolder()
    else Error := ValidateStorage();
    Result := Error = '';
    if not Result then MsgBox(Error, mbInformation, MB_OK);
  end;
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfo, MemoDirInfo, MemoTypeInfo,
  MemoComponentsInfo, MemoGroupInfo, MemoTasksInfo: String): String;
var
  Models, Runtime: String;
begin
  Models := StorageFolder(0);
  Runtime := StorageFolder(1);
  if Models = '' then Models := 'Each user''s AppData\Local Image\models';
  if Runtime = '' then Runtime := 'Each user''s AppData\Local Image\ai';
  Result := MemoDirInfo + NewLine + NewLine + 'AI setup: ' + SetupMode() + NewLine +
    'Models: ' + Models + NewLine + 'Portable ComfyUI parent: ' + Runtime + NewLine +
    'Existing profiles and downloaded models are kept.' + NewLine + MemoTasksInfo;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    if not SaveStringToFile(ExpandConstant('{app}\installation-defaults.json'), Utf8Encode(InstallationDefaults()), False) then
      RaiseException('Could not save installation choices. Retry setup.');
end;

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

function BackendFilesUnlocked(const Directory: String): Boolean;
var
  Stream: TFileStream;
  Backend: String;
begin
  Result := True;
  Backend := Directory + '\backend\LocalRemoveBackend.exe';
  if FileExists(Backend) then begin
    try
      Stream := TFileStream.Create(Backend, fmOpenReadWrite or fmShareExclusive);
      Stream.Free();
    except
      Result := False;
    end;
  end;
end;

function StopInstalledBackend(const Directory: String; OriginalUser: Boolean): Boolean;
var
  ResultCode: Integer;
  Launcher: String;
begin
  Result := True;
  Launcher := '';
  if FileExists(Directory + '\Local Image.exe') then Launcher := Directory + '\Local Image.exe'
  else if FileExists(Directory + '\Local Remove.exe') then
    Launcher := Directory + '\Local Remove.exe';
  if Launcher <> '' then begin
    try
      if OriginalUser then Result := ExecAsOriginalUser(Launcher, '--shutdown-backend', Directory,
        SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0)
      else Result := Exec(Launcher, '--shutdown-backend', Directory,
        SW_HIDE, ewWaitUntilTerminated, ResultCode) and (ResultCode = 0);
    except
      Result := False;
    end;
  end;
  Result := Result and BackendFilesUnlocked(Directory);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Result := ValidateStorage();
  if Result = '' then Result := ValidateApplicationFolder();
  if Result <> '' then exit;
  if CheckForMutexes('Local\LocalRemoveDesktop') then
    Result := 'Save your edits and close Local Image before installing this update.'
  else if not StopInstalledBackend(ExpandConstant('{app}'), True) then
    Result := 'Close Local Image for all Windows users and wait for background work to finish, then retry. Idle background services close within 75 seconds.';
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
    MsgBox('Save your edits and close Local Image before uninstalling.', mbInformation, MB_OK);
    exit;
  end;
  Result := StopInstalledBackend(ExpandConstant('{app}'), False);
  if not Result then MsgBox('Close Local Image for all Windows users and wait for background work to finish, then retry. Idle background services close within 75 seconds.', mbInformation, MB_OK);
end;

// User settings, recovery sessions, and saved projects are deliberately retained.
