;===============================================================================
; DS5 Bridge Companion (Rust) - Windows installer
;
; Per-user / per-machine installer. The setup runs asInvoker: launching it
; NEVER shows a UAC prompt. Choosing "install for all users" elevates on
; demand by relaunching the setup elevated (ExecShell runas /ALLUSERS
; /ELEVATED); the elevated continuation skips the wizard pages and goes
; straight to copying files.
;
; MultiUser.nsh still provides the per-scope install directories, shell
; contexts and mode functions, but its built-in scope page is not used:
; it hides itself from non-elevated users, which would make the scope
; unselectable. A custom page replaces it (ScopePagePre/ScopePageLeave).
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
;   Setup.exe /S              per-user install, no UAC prompt
;   Setup.exe /S /ALLUSERS    per-machine install, one UAC prompt
;   uninstall.exe /S          removes according to where it is located
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
; MultiUser.nsh backend (mixed-mode directories and shell contexts)
;
; MULTIUSER_EXECUTIONLEVEL User => RequestExecutionLevel "user" (asInvoker),
; so Windows never prompts for UAC when the setup starts. Elevation for the
; "all users" scope happens later, via an explicit runas relaunch. The
; command-line parser (/CurrentUser, /AllUsers) is intentionally NOT enabled:
; MultiUser would abort on an unelevated /AllUsers, and its gating on
; privileges does not fit the relaunch flow. Scope flags are parsed by hand.
;-------------------------------------------------------------------------------
!define MULTIUSER_EXECUTIONLEVEL User
!define MULTIUSER_USE_PROGRAMFILES64
!define MULTIUSER_INSTALLMODE_INSTDIR "${PRODUCT_NAME}"
!define MULTIUSER_MUI

!include "MultiUser.nsh"
!include "nsDialogs.nsh"

!ifndef BST_CHECKED
  !define BST_CHECKED 1
!endif
!ifndef BST_UNCHECKED
  !define BST_UNCHECKED 0
!endif

;-------------------------------------------------------------------------------
; Runtime variables
;-------------------------------------------------------------------------------
Var ElevCont           ; 1 = elevated continuation (/ELEVATED): skip wizard pages
Var CmdAllUsers        ; 1 = /ALLUSERS was passed on the command line
Var RadioUser          ; scope page radio handles
Var RadioAll
Var ScopeDefaultAll    ; 1 = preselect "all users" (existing per-machine install)

;-------------------------------------------------------------------------------
; MUI2 pages
;-------------------------------------------------------------------------------
!define MUI_ICON "${ICON_FILE}"
!define MUI_UNICON "${ICON_FILE}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${PRODUCT_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ${PRODUCT_NAME}"

; Skips a page at runtime when this copy is the elevated continuation of an
; earlier run (the scope was already decided there).
Function SkipPagesOnContinuation
  ${if} $ElevCont == 1
    Abort
  ${endif}
FunctionEnd

!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipPagesOnContinuation
!insertmacro MUI_PAGE_WELCOME
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipPagesOnContinuation
!insertmacro MUI_PAGE_LICENSE "..\..\..\LICENSE"
Page custom ScopePagePre ScopePageLeave
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
!macro WRITE_UNINSTALL_REG HIVE
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "DisplayIcon" "$INSTDIR\${PRODUCT_EXE}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "Publisher" "${PUBLISHER}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "URLInfoAbout" "${PROJECT_URL}"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "InstallLocation" "$INSTDIR"
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr ${HIVE} "${UNINSTALL_REG}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  WriteRegDWORD ${HIVE} "${UNINSTALL_REG}" "NoModify" 1
  WriteRegDWORD ${HIVE} "${UNINSTALL_REG}" "NoRepair" 1
  WriteRegDWORD ${HIVE} "${UNINSTALL_REG}" "EstimatedSize" $0
!macroend

;-------------------------------------------------------------------------------
; Applies the "all users" scope without relying on MultiUser's privilege gate
; (used by the elevated continuation, where we know we are elevated).
;-------------------------------------------------------------------------------
!macro APPLY_ALLUSERS_SCOPE
  StrCpy $MultiUser.InstallMode "AllUsers"
  StrCpy $INSTDIR "$PROGRAMFILES64\${MULTIUSER_INSTALLMODE_INSTDIR}"
  SetShellVarContext all
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
  SetRegView 64

  ; Parse our own flags before anything else. /ALLUSERS = scope intent,
  ; /ELEVATED = we are the relaunched elevated continuation.
  StrCpy $ElevCont 0
  StrCpy $CmdAllUsers 0
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/ELEVATED" $R1
  ${IfNot} ${Errors}
    StrCpy $ElevCont 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $R0 "/ALLUSERS" $R1
  ${IfNot} ${Errors}
    StrCpy $CmdAllUsers 1
  ${EndIf}

  !insertmacro MULTIUSER_INIT   ; default scope: current user (never aborts)

  ${if} $ElevCont == 1
    ; Elevated continuation: the scope was decided before the UAC prompt.
    !insertmacro APPLY_ALLUSERS_SCOPE
  ${elseif} $CmdAllUsers == 1
    UserInfo::GetAccountType
    Pop $R9
    ${if} $R9 == "Admin"
    ${orif} $R9 == "Power"
      ; Started "as administrator" already: use the scope directly.
      !insertmacro APPLY_ALLUSERS_SCOPE
    ${else}
      ; Not elevated: relaunch with UAC, then quit this copy. The elevated
      ; child receives /ELEVATED and skips the wizard pages.
      StrCpy $R7 "/ALLUSERS /ELEVATED"
      IfSilent init_elev_add_s
      Goto init_elev_args_done
    init_elev_add_s:
      StrCpy $R7 "/ALLUSERS /ELEVATED /S"
    init_elev_args_done:
      ClearErrors
      ExecShell 'runas' "$EXEFILE" $R7
      IfErrors init_elev_failed 0
      Quit
    init_elev_failed:
      IfSilent init_elev_no_msg
      MessageBox MB_OK|MB_ICONEXCLAMATION "Windows did not grant administrator permission.$\n$\nInstalling for all users is not possible right now."
    init_elev_no_msg:
      SetErrorLevel 5
      Quit
    ${endif}
  ${endif}

  !insertmacro CHECK_RUNNING ""
FunctionEnd

Function ScopePagePre
  ${if} $ElevCont == 1
    Abort   ; scope already decided by the elevated continuation
  ${endif}

  !insertmacro MUI_HEADER_TEXT "Choose Users" "Choose for which users you want to install $(^NameDA)."

  nsDialogs::Create 1018
  Pop $0
  ${NSD_CreateLabel} 0 2u 100% 42u "Select whether you want to install $(^NameDA) only for yourself or for all users of this computer. Installing for everyone requires administrator permission. $(^ClickNext)"
  Pop $0
  ${NSD_CreateRadioButton} 15u 50u -15u 10u "Install just for me"
  Pop $RadioUser
  ${NSD_CreateRadioButton} 15u 70u -15u 10u "Install for anyone using this computer"
  Pop $RadioAll

  ; Preselect "all users" when only a per-machine install exists (repair /
  ; upgrade of that install), otherwise default to per-user.
  StrCpy $ScopeDefaultAll 0
  ReadRegStr $R9 HKLM "${UNINSTALL_REG}" "InstallLocation"
  ${if} $R9 != ""
    ReadRegStr $R8 HKCU "${UNINSTALL_REG}" "InstallLocation"
    ${if} $R8 == ""
      StrCpy $ScopeDefaultAll 1
    ${endif}
  ${endif}
  ${if} $ScopeDefaultAll == 1
    ${NSD_Check} $RadioAll
  ${else}
    ${NSD_Check} $RadioUser
  ${endif}

  nsDialogs::Show
FunctionEnd

Function ScopePageLeave
  ${NSD_GetState} $RadioAll $0
  ${if} $0 == ${BST_CHECKED}
    ${if} $MultiUser.Privileges == "Admin"
    ${orif} $MultiUser.Privileges == "Power"
      Call MultiUser.InstallMode.AllUsers
    ${else}
      ; Not elevated: restart this setup elevated. The elevated copy skips
      ; the wizard pages (welcome, license, scope) and installs directly.
      ClearErrors
      ExecShell 'runas' "$EXEFILE" "/ALLUSERS /ELEVATED"
      IfErrors scope_elev_failed 0
      Quit
    scope_elev_failed:
      MessageBox MB_OK|MB_ICONEXCLAMATION "Windows did not grant administrator permission.$\n$\nInstalling for all users is not possible right now. Choose Install just for me instead, or approve the administrator prompt."
      Abort   ; stay on this page so the scope can be changed
    ${endif}
  ${else}
    Call MultiUser.InstallMode.CurrentUser
  ${endif}
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
    !insertmacro WRITE_UNINSTALL_REG "HKLM"
  ${else}
    !insertmacro WRITE_UNINSTALL_REG "HKCU"
  ${endif}

  DetailPrint "Install mode: $MultiUser.InstallMode"
SectionEnd

;-------------------------------------------------------------------------------
; Uninstaller
;
; The scope is identified by where the uninstaller lives (install dirs are
; fixed), so no flags are needed: a per-machine uninstall that is not elevated
; relaunches itself with UAC; /ELEVATED marks the elevated child so it never
; relaunches again.
;-------------------------------------------------------------------------------
Function un.onInit
  !insertmacro MULTIUSER_UNINIT
  SetRegView 64

  StrCpy $ElevCont 0
  ${un.GetParameters} $R0
  ClearErrors
  ${un.GetOptions} $R0 "/ELEVATED" $R1
  ${IfNot} ${Errors}
    StrCpy $ElevCont 1
  ${EndIf}

  ; Where does this uninstaller live?
  StrCpy $R9 "$PROGRAMFILES64\${MULTIUSER_INSTALLMODE_INSTDIR}"
  StrCpy $R8 "$LocalAppData\Programs\${MULTIUSER_INSTALLMODE_INSTDIR}"
  StrCpy $R7 ""
  ${if} $INSTDIR == $R9
    StrCpy $R7 "AllUsers"
  ${elseif} $INSTDIR == $R8
    StrCpy $R7 "CurrentUser"
  ${endif}

  ${if} $R7 == "AllUsers"
    ${if} $ElevCont == 1
      ; Elevated continuation: MultiUser applies the mode (we are elevated).
      Call un.MultiUser.InstallMode.AllUsers
    ${else}
      UserInfo::GetAccountType
      Pop $R6
      ${if} $R6 == "Admin"
      ${orif} $R6 == "Power"
        Call un.MultiUser.InstallMode.AllUsers
      ${else}
        ; Per-machine removal needs elevation: relaunch ourselves with UAC.
        ; Deliberately $INSTDIR\uninstall.exe, not $EXEFILE: this copy is the
        ; bootstrap and is already running from %TEMP%. Relaunching the
        ; on-disk uninstaller re-bootstraps with the correct $INSTDIR (scope
        ; detection) and lets the elevated worker delete itself cleanly.
        StrCpy $R5 "/ELEVATED"
        IfSilent un_elev_add_s
        Goto un_elev_args_done
      un_elev_add_s:
        StrCpy $R5 "/ELEVATED /S"
      un_elev_args_done:
        ClearErrors
        ExecShell 'runas' "$INSTDIR\uninstall.exe" $R5
        IfErrors un_elev_failed 0
        Quit
      un_elev_failed:
        IfSilent un_elev_no_msg
        MessageBox MB_OK|MB_ICONEXCLAMATION "Windows did not grant administrator permission.$\n$\n${PRODUCT_NAME} was not removed."
      un_elev_no_msg:
        SetErrorLevel 5
        Quit
      ${endif}
    ${endif}
  ${elseif} $R7 == "CurrentUser"
    Call un.MultiUser.InstallMode.CurrentUser
  ${endif}
  ; Unknown layout ($R7 empty): keep MultiUser's default (current user).

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
  IfErrors 0 un_dir_removed
    DetailPrint "Note: $INSTDIR was not removed (it still contains files)."
  un_dir_removed:
SectionEnd
