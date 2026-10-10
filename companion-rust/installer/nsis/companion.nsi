;===============================================================================
; DS5 Bridge Companion (Rust) - Windows installer
;
; Per-user / per-machine installer using the MultiUser.nsh add-on, so the
; "Install for me only / for all users" choice is made in the UI (and via
; /CurrentUser or /AllUsers on the command line for silent runs).
;
; Preferred build:  ..\build.ps1   (builds the release binary, then packages)
;
; Direct build (from this directory):
;   makensis /DPRODUCT_VERSION=0.1.0 ^
;            /DPRODUCT_VERSION_QUAD=0.1.0.0 ^
;            /DRELEASE_EXE=..\..\target\release\companion-rust.exe ^
;            companion.nsi
;
; Silent usage:
;   Setup.exe /S /CurrentUser     (per-user, no admin rights needed)
;   Setup.exe /S /AllUsers        (per-machine, requires elevation)
;   uninstall.exe /S /CurrentUser | /S /AllUsers
;===============================================================================

Unicode true
SetCompressor /SOLID LZMA
ManifestDPIAware true

;-------------------------------------------------------------------------------
; Overridable build-time settings
;-------------------------------------------------------------------------------
!ifndef RELEASE_EXE
  !define RELEASE_EXE "..\..\target\release\companion-rust.exe"
!endif
!ifndef PRODUCT_VERSION
  !define PRODUCT_VERSION "0.1.0"
!endif
!ifndef PRODUCT_VERSION_QUAD
  !define PRODUCT_VERSION_QUAD "${PRODUCT_VERSION}.0.0"
!endif
!ifndef OUTFILE
  !define OUTFILE "..\..\artifacts\installer\DS5-Bridge-Companion-Setup-${PRODUCT_VERSION}.exe"
!endif

;-------------------------------------------------------------------------------
; Product identity
;-------------------------------------------------------------------------------
!define PRODUCT_NAME "DS5 Bridge Companion"
!define PRODUCT_EXE "DS5BridgeCompanion.exe"     ; installed file name
!define DEV_EXE "companion-rust.exe"             ; dev-build file name (also killed if running)
!define PUBLISHER "SundayMoments"
!define PROJECT_URL "https://github.com/SundayMoments/DS5_Bridge"
!define UNINSTALL_KEY "DS5BridgeCompanion"
!define UNINSTALL_REG "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINSTALL_KEY}"
!define ICON_FILE "..\..\assets\app.ico"

;-------------------------------------------------------------------------------
; MultiUser.nsh - mixed-mode install (current user <-> all users)
;   Highest  : admins get one UAC prompt up front and can pick either scope;
;              standard users are never prompted and can only install for
;              themselves (the "all users" option is disabled for them).
;-------------------------------------------------------------------------------
!define MULTIUSER_EXECUTIONLEVEL Highest
!define MULTIUSER_USE_PROGRAMFILES64
!define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCT_NAME}"
!define MULTIUSER_INSTALLMODE_DEFAULT_CURRENTUSER
!define MULTIUSER_INSTALLMODE_COMMANDLINE
!define MULTIUSER_INSTALLMODEPAGE_SHOWUSERNAME
!define MULTIUSER_MUI

!include "MultiUser.nsh"

;-------------------------------------------------------------------------------
; MUI2 pages
;-------------------------------------------------------------------------------
!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${PRODUCT_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ${PRODUCT_NAME}"

; No directory page is offered (like the Electron companion), so spell out
; where each scope installs.
!define MULTIUSER_INSTALLMODEPAGE_TEXT_TOP "Select whether you want to install $(^NameDA) only for yourself or for all users of this computer. A per-user install goes to your profile (LocalAppData\Programs), a per-machine install goes to Program Files. The location cannot be changed afterwards. $(^ClickNext)"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\..\..\LICENSE"
!insertmacro MULTIUSER_PAGE_INSTALLMODE
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

;-------------------------------------------------------------------------------
; General attributes
;-------------------------------------------------------------------------------
Name "${PRODUCT_NAME}"
BrandingText "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${PRODUCT_NAME}"

VIProductVersion "${PRODUCT_VERSION_QUAD}"
VIAddVersionKey /LANG=0 "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=0 "FileDescription" "${PRODUCT_NAME} Setup"
VIAddVersionKey /LANG=0 "FileVersion" "${PRODUCT_VERSION}"
VIAddVersionKey /LANG=0 "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey /LANG=0 "CompanyName" "${PUBLISHER}"
VIAddVersionKey /LANG=0 "LegalCopyright" "DS5 Bridge contributors. Licensed under AGPL-3.0."
VIAddVersionKey /LANG=0 "OriginalFilename" "DS5-Bridge-Companion-Setup-${PRODUCT_VERSION}.exe"

;-------------------------------------------------------------------------------
; Uninstall registry entry (duplicated per hive - the hive is chosen at
; runtime, which NSIS WriteRegStr cannot do with a variable root key)
;-------------------------------------------------------------------------------
!macro WRITE_UNINSTALL_REG HIVE MODE
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "DisplayIcon" "$INSTDIR\${PRODUCT_EXE}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "Publisher" "${PUBLISHER}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "URLInfoAbout" "${PROJECT_URL}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "InstallLocation" "$INSTDIR"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S ${MODE}"
  WriteRegDWORD ${HIVE} "${UNINSTALL_REG}" "NoModify" 1
  WriteRegDWORD ${HIVE} "${UNINSTALL_REG}" "NoRepair" 1
  WriteRegDWORD ${HIVE} "${UNINSTALL_REG}" "EstimatedSize" $0
!macroend

;-------------------------------------------------------------------------------
; Running-instance helpers
;
; The companion hides to the tray instead of quitting, so a running instance
; keeps a (hidden) top-level window whose title is ${PRODUCT_NAME}. FindWindow
; matches hidden windows too. The process is then closed via taskkill; both the
; installed and the dev-build file name are targeted.
;-------------------------------------------------------------------------------
!macro CHECK_RUNNING UNPREFIX
  System::Call 'user32::FindWindowW(0, t "${PRODUCT_NAME}") p.r0'
  StrCmp $0 "" ${UNPREFIX}not_running
  StrCmp $0 "0" ${UNPREFIX}not_running
  IfSilent ${UNPREFIX}do_kill
  MessageBox MB_YESNO|MB_ICONQUESTION "${PRODUCT_NAME} appears to be running (it may be hidden in the system tray). Setup must close it before continuing.$\n$\nClose it now?" IDYES ${UNPREFIX}do_kill
  Abort
${UNPREFIX}do_kill:
  DetailPrint "Closing running ${PRODUCT_NAME}..."
  nsExec::ExecToStack '"taskkill" /F /IM "${PRODUCT_EXE}"'
  Pop $0
  Pop $0
  nsExec::ExecToStack '"taskkill" /F /IM "${DEV_EXE}"'
  Pop $0
  Pop $0
  Sleep 1000
${UNPREFIX}not_running:
!macroend

;-------------------------------------------------------------------------------
; Installer
;-------------------------------------------------------------------------------
Function .onInit
  !insertmacro MULTIUSER_INIT
  SetRegView 64
  !insertmacro CHECK_RUNNING ""
FunctionEnd

Section "Install"
  SetRegView 64
  SetOutPath $INSTDIR
  DetailPrint "Installing ${PRODUCT_NAME} ${PRODUCT_VERSION}"
  DetailPrint "Destination: $INSTDIR"

  File /oname=${PRODUCT_EXE} "${RELEASE_EXE}"
  File /oname=LICENSE.txt "..\..\..\LICENSE"
  File /oname=NOTICE.txt "..\..\..\NOTICE"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}.lnk" "$INSTDIR\${PRODUCT_EXE}" "" "$INSTDIR\${PRODUCT_EXE}" 0
  CreateShortCut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\${PRODUCT_EXE}" "" "$INSTDIR\${PRODUCT_EXE}" 0

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  DetailPrint "Installed size: $0 KB ($1 files)"

  ${if} $MultiUser.InstallMode == "AllUsers"
    !insertmacro WRITE_UNINSTALL_REG "HKLM" "/AllUsers"
  ${else}
    !insertmacro WRITE_UNINSTALL_REG "HKCU" "/CurrentUser"
  ${endif}

  DetailPrint "Install mode: $MultiUser.InstallMode"
SectionEnd

;-------------------------------------------------------------------------------
; Uninstaller
;-------------------------------------------------------------------------------
Function un.onInit
  !insertmacro MULTIUSER_UNINIT
  SetRegView 64

  ; MultiUser picks a default mode from the current privileges; correct it by
  ; looking at which hive holds this install location (install dir is fixed,
  ; so $EXEDIR identifies the scope).
  ReadRegStr $R7 HKLM "${UNINSTALL_REG}" "InstallLocation"
  ReadRegStr $R8 HKCU "${UNINSTALL_REG}" "InstallLocation"
  ${if} $R7 == $EXEDIR
    Call un.MultiUser.InstallMode.AllUsers
  ${elseif} $R8 == $EXEDIR
    Call un.MultiUser.InstallMode.CurrentUser
  ${endif}

  ${if} $INSTDIR == ""
    Abort
  ${endif}

  !insertmacro CHECK_RUNNING "un_"
FunctionEnd

Section "Uninstall"
  SetRegView 64
  DetailPrint "Removing ${PRODUCT_NAME} from $INSTDIR"

  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}.lnk"

  Delete "$INSTDIR\${PRODUCT_EXE}"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\NOTICE.txt"
  Delete "$INSTDIR\uninstall.exe"

  ${if} $MultiUser.InstallMode == "AllUsers"
    DeleteRegKey HKLM "${UNINSTALL_REG}"
  ${else}
    DeleteRegKey HKCU "${UNINSTALL_REG}"
  ${endif}

  ; Config data (%AppData%\DS5Bridge\DS5BridgeCompanion) is intentionally kept.
  RMDir "$INSTDIR"
  IfErrors 0 +2
    DetailPrint "Note: $INSTDIR was not removed (it still contains files)."
SectionEnd
