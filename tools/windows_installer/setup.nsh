!include LogicLib.nsh
!include FileFunc.nsh

Var SetupBase
Var SetupScratch
Var ScratchHandle
Var CoordinatorHandle
Var SetupStatus
Var SetupMessage
Var FixtureArg
Var SetupVerified

; The installed and outer launchers share the exact same scratch and helper checks.
!macro SetupFunctions Prefix
Function ${Prefix}OpenScratch
    StrCpy $SetupStatus 10
    StrCpy $SetupMessage "Chronobreak setup could not create a private extraction folder."
    StrCpy $SetupBase "$LOCALAPPDATA"
    StrCpy $FixtureArg ""
    !ifdef CHRONOBREAK_SETUP_FIXTURE
        ReadEnvStr $SetupBase CHRONOBREAK_SETUP_FIXTURE_ROOT
        StrCmp $SetupBase "" done
        IfFileExists "$SetupBase\.setup-fixture" 0 done
        StrCpy $FixtureArg '$\"$SetupBase$\"'
    !endif
    StrCpy $0 "$SetupBase"
    boundaries:
        System::Call 'kernel32::GetFileAttributesW(w r0)i.r1'
        IntCmp $1 -1 done
        IntOp $2 $1 & 0x410
        IntCmp $2 0x10 boundary_ok done done
    boundary_ok:
        ${GetRoot} "$0" $2
        StrCmp "$0" "$2" make_scratch
        ${GetParent} "$0" $0
        StrCmp $0 "" done
        StrCmp "$0" "$2" 0 boundaries
        StrCpy $0 "$0\"
        System::Call 'kernel32::GetFileAttributesW(w r0)i.r1'
        IntCmp $1 -1 done
        IntOp $2 $1 & 0x410
        IntCmp $2 0x10 make_scratch done done
    make_scratch:
        System::Call 'ole32::CoCreateGuid(g .r0)i.r1'
        IntCmp $1 0 0 done done
        StrCpy $1 $0 8 1
        StrCpy $2 $0 4 10
        StrCpy $3 $0 4 15
        StrCpy $4 $0 4 20
        StrCpy $5 $0 12 25
        StrCpy $0 "$SetupBase\Chronobreak-setup-$1$2$3$4$5"
        System::Call 'kernel32::CreateDirectoryW(w r0, p 0)i.r1'
        IntCmp $1 0 done
        StrCpy $SetupScratch "$0"
        System::Call 'kernel32::CreateFileW(w r0, i 0x80, i 3, p 0, i 3, i 0x02200000, p 0)p.r1'
        IntCmp $1 -1 done
        StrCpy $ScratchHandle "$1"
        System::Call 'kernel32::GetFileAttributesW(w r0)i.r1'
        IntOp $2 $1 & 0x410
        IntCmp $2 0x10 0 done done
        StrCpy $SetupStatus 0
    done:
FunctionEnd

; Verify a build-bound helper before starting it. Hold the read-only file handle
; (sharing read only) until the last invocation completes.
Function ${Prefix}VerifyCoordinator
    StrCpy $SetupStatus 11
    StrCpy $SetupMessage "The bundled Chronobreak setup helper could not be verified."
    StrCpy $0 "$SetupScratch\chronobreak-setup.exe"
    System::Call 'kernel32::CreateFileW(w r0, i 0x80000000, i 1, p 0, i 3, i 0x00200000, p 0)p.r1'
    IntCmp $1 -1 done
    StrCpy $CoordinatorHandle "$1"
    System::Call 'kernel32::GetFileSize(p r1,p 0)i.r2'
    IntCmp $2 ${CHRONOBREAK_COORDINATOR_BYTES} 0 done done
    System::Alloc ${CHRONOBREAK_COORDINATOR_BYTES}
    Pop $3
    IntCmp $3 0 done
    System::Call 'kernel32::ReadFile(p r1, p r3, i r2, *i .r4, p 0)i.r5'
    IntCmp $5 0 free_buffer
    IntCmp $4 $2 0 free_buffer free_buffer
    System::Call 'advapi32::CryptAcquireContextW(*p .r6, p 0, p 0, i 24, i 0xf0000000)i.r5'
    IntCmp $5 0 free_buffer
    System::Call 'advapi32::CryptCreateHash(p r6, i 0x800c, p 0, i 0, *p .r7)i.r5'
    IntCmp $5 0 release_provider
    System::Call 'advapi32::CryptHashData(p r7, p r3, i r2, i 0)i.r5'
    IntCmp $5 0 release_hash
    System::Call 'advapi32::CryptGetHashParam(p r7, i 2, p r3, *i 32, i 0)i.r5'
    IntCmp $5 0 release_hash
    System::Call 'crypt32::CryptBinaryToStringW(p r3, i 32, i 0x4000000c, w .r8, *i 65)i.r5'
    IntCmp $5 0 release_hash
    StrCmp $8 "${CHRONOBREAK_COORDINATOR_SHA256}" 0 release_hash
    StrCpy $SetupVerified 1
    StrCpy $SetupStatus 0
    release_hash:
        System::Call 'advapi32::CryptDestroyHash(p r7)'
    release_provider:
        System::Call 'advapi32::CryptReleaseContext(p r6, i 0)'
    free_buffer:
        System::Free $3
    done:
FunctionEnd

Function ${Prefix}CloseScratch
    StrCmp $SetupVerified 1 0 close_helper
    IfFileExists "$SetupScratch\payload\payload.json" 0 close_helper
    nsExec::ExecToStack '$\"$SetupScratch\chronobreak-setup.exe$\" clean-extraction $\"$SetupScratch$\"'
    Pop $0
    Pop $1
    close_helper:
    StrCmp $CoordinatorHandle "" +2
        System::Call 'kernel32::CloseHandle(p $CoordinatorHandle)'
    StrCmp $SetupScratch "" done
    StrCmp $ScratchHandle "" done
    SetOutPath "$SetupBase"
    Delete "$SetupScratch\chronobreak-setup.exe"
    Delete "$SetupScratch\envelope.json"
    Delete "$SetupScratch\uninstall.exe"
    Delete "$SetupScratch\chronobreak-link.lnk"
    Delete "$SetupScratch\MicrosoftEdgeWebView2Setup.exe"
    StrCmp $ScratchHandle "" +2
        System::Call 'kernel32::CloseHandle(p $ScratchHandle)'
    RMDir "$SetupScratch"
    done:
FunctionEnd

Function ${Prefix}ShowFailure
    DetailPrint "$SetupMessage"
    !ifdef CHRONOBREAK_SETUP_FIXTURE
        IfFileExists "$SetupBase\.setup-fixture" 0 +4
        FileOpen $0 "$SetupBase\setup-error.log" w
        FileWrite $0 "$SetupStatus $SetupMessage"
        FileClose $0
    !endif
    IfSilent silent
    MessageBox MB_OK|MB_ICONEXCLAMATION "$SetupMessage"
    silent:
    SetErrorLevel $SetupStatus
    Abort
FunctionEnd
!macroend
