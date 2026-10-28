; Kawaii Radio (native) — the Windows installer.
; It installs where the first (Tauri) version lived (per user, %LOCALAPPDATA%\Kawaii Radio, the same exe name and the
; same uninstall entry), so it is also that version's update: the old updater runs it with /P /UPDATE /R, and the
; native updater with /S /UPDATE. An update is silent and starts the app again.
;   makensis /DVERSION=0.8.0 installer.nsi   (release.py does it)

Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define NAME "Kawaii Radio"
!define EXE "kawaii-radio.exe"
!define UNINST "Software\Microsoft\Windows\CurrentVersion\Uninstall\Kawaii Radio"

Name "${NAME}"
OutFile "..\dist\KawaiiRadio-${VERSION}-setup.exe"
InstallDir "$LOCALAPPDATA\Kawaii Radio"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "Kawaii Radio ${VERSION}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${NAME}"
VIAddVersionKey "FileDescription" "${NAME} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "Kawaii Uz"
VIAddVersionKey "LegalCopyright" "Kawaii Uz"

!define MUI_ICON "..\app\icons\icon.ico"
!define MUI_UNICON "..\app\icons\icon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Kawaii Radio'ni ochish"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var Update

Function .onInit
  ${GetParameters} $0
  StrCpy $Update 0
  ClearErrors
  ${GetOptions} $0 "/UPDATE" $1
  ${IfNot} ${Errors}
    StrCpy $Update 1
    SetSilent silent
  ${EndIf}
  ClearErrors
  ${GetOptions} $0 "/P" $1
  ${IfNot} ${Errors}
    SetSilent silent
  ${EndIf}
FunctionEnd

Section "Install"
  ; the running copy (the old version's updater has just quit it; the native one quits itself)
  nsExec::Exec 'taskkill /F /IM ${EXE}'
  Sleep 700
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File "target\release\${EXE}"
  ; what the web version needed and this one does not
  Delete "$INSTDIR\WebView2Loader.dll"
  RMDir /r "$INSTDIR\cache"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Kawaii Radio.lnk" "$INSTDIR\${EXE}"
  ${If} $Update == 0
    CreateShortcut "$DESKTOP\Kawaii Radio.lnk" "$INSTDIR\${EXE}"
  ${EndIf}
  WriteRegStr HKCU "${UNINST}" "DisplayName" "${NAME}"
  WriteRegStr HKCU "${UNINST}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST}" "Publisher" "Kawaii Uz"
  WriteRegStr HKCU "${UNINST}" "InstallLocation" '"$INSTDIR"'
  WriteRegStr HKCU "${UNINST}" "DisplayIcon" '"$INSTDIR\${EXE}"'
  WriteRegStr HKCU "${UNINST}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINST}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNINST}" "EstimatedSize" $0
  ; an update starts the app again by itself
  ${If} ${Silent}
    ExecShell "" "$INSTDIR\${EXE}"
  ${EndIf}
SectionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /F /IM ${EXE}'
  Sleep 500
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Kawaii Radio.lnk"
  Delete "$DESKTOP\Kawaii Radio.lnk"
  DeleteRegKey HKCU "${UNINST}"
SectionEnd
