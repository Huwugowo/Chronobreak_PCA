Unicode true
RequestExecutionLevel user
CRCCheck force
ShowUninstDetails show
SetCompressor /SOLID lzma
!include "$%CHRONOBREAK_RELEASE_STAGE%\payload.nsh"
!include "${CHRONOBREAK_INSTALLER_SCRIPTS}\setup.nsh"

Name "Chronobreak"
OutFile "${CHRONOBREAK_PAYLOAD_STAGE}\uninstaller-authoring.exe"
VIProductVersion "${CHRONOBREAK_WINDOWS_VERSION}.0"
VIAddVersionKey "ProductName" "Chronobreak"
VIAddVersionKey "FileDescription" "Chronobreak uninstaller"
VIAddVersionKey "FileVersion" "${CHRONOBREAK_WINDOWS_VERSION}"
VIAddVersionKey "LegalCopyright" "Chronobreak contributors"
!uninstfinalize 'cmd.exe /d /c copy /b /y "%1" "${CHRONOBREAK_PAYLOAD_STAGE}\uninstall.exe"' = 0

UninstPage uninstConfirm
UninstPage instfiles
!insertmacro SetupFunctions "un."

Section
    WriteUninstaller "$TEMP\uninstall.exe"
SectionEnd

Section "Uninstall"
    Call un.OpenScratch
    StrCmp $SetupStatus 0 0 failed
    SetOutPath "$SetupScratch"
    ClearErrors
    File "${CHRONOBREAK_PAYLOAD_STAGE}\chronobreak-setup.exe"
    IfErrors failed
    Call un.VerifyCoordinator
    StrCmp $SetupStatus 0 0 failed
    nsExec::ExecToStack '$\"$SetupScratch\chronobreak-setup.exe$\" uninstall $FixtureArg'
    Pop $SetupStatus
    Pop $SetupMessage
    Call un.CloseScratch
    StrCmp $SetupStatus 0 complete
    StrCmp $SetupStatus "error" 0 +3
        StrCpy $SetupStatus 13
        StrCpy $SetupMessage "Chromobreak could not start the uninstall helper."
    Call un.ShowFailure
    failed:
        StrCmp $SetupStatus 0 0 +2
            StrCpy $SetupStatus 10
        Call un.CloseScratch
        Call un.ShowFailure
    complete:
    StrCmp $SetupMessage "" +2
        DetailPrint "$SetupMessage"
    SetErrorLevel 0
SectionEnd
