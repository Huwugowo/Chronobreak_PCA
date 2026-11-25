use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, ensure};
use windows::Win32::Foundation::HWND;
use windows::Win32::Security::{
    Cryptography::{CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW},
    WinTrust::*,
};
use windows::core::PCWSTR;

use crate::fs_guard::{Node, wide};

pub fn install(scratch: &Path) -> Result<()> {
    let _parents = Node::boundaries(scratch)?;
    let bootstrapper = scratch.join("MicrosoftEdgeWebView2Setup.exe");
    let file = Node::executable(&bootstrapper)?;
    let filename = wide(bootstrapper.as_os_str());
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(filename.as_ptr()),
        hFile: file.raw_handle(),
        ..Default::default()
    };
    let mut trust = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: the file, contained pointers and provider state stay live through verification.
    let verified = unsafe { WinVerifyTrustEx(HWND::default(), &mut action, &mut trust) } == 0;
    let publisher = if verified {
        // SAFETY: the trust provider owns these structures until the closing call.
        unsafe { microsoft_publisher(&trust) }
    } else {
        false
    };
    trust.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: this releases any state allocated by the preceding verification.
    unsafe { WinVerifyTrustEx(HWND::default(), &mut action, &mut trust) };
    ensure!(
        publisher,
        "Microsoft's WebView2 bootstrapper signature did not verify."
    );
    let status = Command::new(&bootstrapper)
        .args(["/silent", "/install"])
        .creation_flags(0x0800_0000)
        .status()?;
    ensure!(
        status.success(),
        "WebView2 could not be installed ({status})."
    );
    Ok(())
}

unsafe fn microsoft_publisher(trust: &WINTRUST_DATA) -> bool {
    // SAFETY: the caller holds a successful and still open provider state.
    unsafe {
        let provider = WTHelperProvDataFromStateData(trust.hWVTStateData);
        if provider.is_null() {
            return false;
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
        if signer.is_null() {
            return false;
        }
        let cert = WTHelperGetProvCertFromChain(signer, 0);
        if cert.is_null() {
            return false;
        }
        let mut name = [0; 128];
        let length = CertGetNameStringW(
            (*cert).pCert,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            None,
            Some(&mut name),
        );
        (2..=name.len() as u32).contains(&length)
            && String::from_utf16_lossy(&name[..length as usize - 1]) == "Microsoft Corporation"
    }
}
