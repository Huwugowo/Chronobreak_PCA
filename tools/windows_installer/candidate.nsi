Unicode true
RequestExecutionLevel user
CRCCheck force
ShowInstDetails show
SetCompressor /SOLID lzma
Name "{{product_name}} {{version}}"
OutFile "{{out_file}}"
InstallDir "$LOCALAPPDATA\League Replay"
VIProductVersion "{{version_with_build}}"
VIAddVersionKey "ProductName" "Chronobreak"
VIAddVersionKey "FileDescription" "Chronobreak Windows setup"
VIAddVersionKey "FileVersion" "{{version}}"
VIAddVersionKey "LegalCopyright" "Chronobreak contributors"

!include "$%CHRONOBREAK_RELEASE_STAGE%\payload.nsh"
!include "${CHRONOBREAK_INSTALLER_SCRIPTS}\setup.nsh"
!include MUI2.nsh
!include x64.nsh
!include WordFunc.nsh

!define MUI_WELCOMEFINISHPAGE_BITMAP "${NSISDIR}\Contrib\Graphics\Wizard\nsis.bmp"
!define MUI_WELCOMEPAGE_TEXT "Install or repair Chronobreak for your Windows account. Close Chronobreak and quit its tray recorder before continuing. Setup preserves your recordings, clips, and settings."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro SetupFunctions ""

!define WEBVIEW_KEY "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

Function .onInit
    ${IfNot} ${IsNativeAMD64}
        StrCpy $SetupStatus 10
        StrCpy $SetupMessage "Chronobreak requires Windows on an x64 processor."
        Call ShowFailure
    ${EndIf}
    StrCmp "$INSTDIR" "$LOCALAPPDATA\League Replay" +4
        StrCpy $SetupStatus 10
        StrCpy $SetupMessage "Chronobreak setup uses its fixed per-user location."
        Call ShowFailure
FunctionEnd

Function FindWebView
    SetRegView 32
    ReadRegStr $0 HKLM "${WEBVIEW_KEY}" "pv"
    ${If} $0 != ""
        ${VersionCompare} "$0" "0.0.0.0" $1
        StrCmp $1 1 found
    ${EndIf}
    ReadRegStr $0 HKCU "${WEBVIEW_KEY}" "pv"
    ${If} $0 != ""
        ${VersionCompare} "$0" "0.0.0.0" $1
        StrCmp $1 1 found
    ${EndIf}
    StrCpy $0 ""
    found:
FunctionEnd

Function EnsureWebView
    StrCpy $SetupStatus 12
    StrCpy $SetupMessage "Microsoft WebView2 is required. Connect to the internet, then run Chronobreak setup again."
    !ifdef CHRONOBREAK_SETUP_FIXTURE
        ReadEnvStr $1 CHRONOBREAK_SETUP_TEST_WEBVIEW
        StrCmp $1 "failed" failed
    !endif
    Call FindWebView
    StrCmp $0 "" 0 ready
    DetailPrint "Downloading Microsoft WebView2."
    StrCpy $0 "$SetupScratch\MicrosoftEdgeWebView2Setup.exe"
    System::Call 'urlmon::URLDownloadToFileW(p 0, w "https://go.microsoft.com/fwlink/p/?LinkId=2124703", w r0, i 0, p 0)i.r1'
    IntCmp $1 0 0 failed failed
    DetailPrint "Installing Microsoft WebView2."
    nsExec::ExecToStack '$\"$SetupScratch\chronobreak-setup.exe$\" install-webview $\"$SetupScratch$\"'
    Pop $1
    Pop $2
    ${If} $1 != 0
        DetailPrint "$2"
        Goto failed
    ${EndIf}
    Call FindWebView
    StrCmp $0 "" failed
    ready:
        StrCpy $SetupStatus 0
    failed:
FunctionEnd

Section
    Call OpenScratch
    StrCmp $SetupStatus 0 0 failed
    SetOutPath "$SetupScratch"
    ClearErrors
    File "${CHRONOBREAK_PAYLOAD_STAGE}\chronobreak-setup.exe"
    File "${CHRONOBREAK_PAYLOAD_STAGE}\envelope.json"
    File "${CHRONOBREAK_PAYLOAD_STAGE}\uninstall.exe"
    IfErrors extract_failed
    Call VerifyCoordinator
    StrCmp $SetupStatus 0 0 failed
    Call EnsureWebView
    StrCmp $SetupStatus 0 0 failed
    SetOutPath "$SetupScratch\payload"
    ClearErrors
    File /r "${CHRONOBREAK_PAYLOAD_STAGE}\payload\*"
    IfErrors extract_failed
    SetOutPath "$SetupBase"
    nsExec::ExecToStack '$\"$SetupScratch\chronobreak-setup.exe$\" install $FixtureArg $\"$SetupScratch$\"'
    Pop $SetupStatus
    Pop $SetupMessage
    StrCmp $SetupStatus "error" 0 +3
        StrCpy $SetupStatus 13
        StrCpy $SetupMessage "Chronobreak could not start the setup helper."
    StrCmp $SetupStatus 0 complete failed
    extract_failed:
        StrCpy $SetupStatus 10
        StrCpy $SetupMessage "Chronobreak setup could not extract its package."
    failed:
        Call CloseScratch
        Call ShowFailure
    complete:
        StrCmp $SetupMessage "" +2
            DetailPrint "$SetupMessage"
        Call CloseScratch
        SetErrorLevel 0
SectionEnd
