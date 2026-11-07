//! File system ACL helpers: grant / revoke access for a SID on a path.

use std::path::Path;

use windows_sys::Win32::Foundation::{LocalFree, ERROR_SUCCESS};
use windows_sys::Win32::Security::Authorization::{
    GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, GRANT_ACCESS,
    NO_MULTIPLE_TRUSTEE, REVOKE_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    EqualSid, GetAce, ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
};

use super::util::{wide, Sid};
use crate::{Error, Result};

/// Read, write, execute, delete — everything a program needs in its own
/// working directory (FILE_ALL_ACCESS).
pub const ACCESS_FULL: u32 = 0x001F_01FF;
/// FILE_GENERIC_READ | FILE_GENERIC_EXECUTE.
pub const ACCESS_READ_EXECUTE: u32 = 0x0012_0089 | 0x0012_00A0;

const CONTAINER_AND_OBJECT_INHERIT: u32 = 0x3; // OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;

fn apply(path: &Path, sid: &Sid, access: u32, mode: i32, inherit: bool) -> Result<()> {
    let wpath = wide(path.as_os_str());
    unsafe {
        let mut old_dacl: *mut ACL = std::ptr::null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let rc = GetNamedSecurityInfoW(
            wpath.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut old_dacl,
            std::ptr::null_mut(),
            &mut sd,
        );
        if rc != ERROR_SUCCESS {
            return Err(Error::os(format!("GetNamedSecurityInfo({})", path.display()), rc));
        }

        let ea = EXPLICIT_ACCESS_W {
            grfAccessPermissions: access,
            grfAccessMode: mode as _,
            grfInheritance: if inherit { CONTAINER_AND_OBJECT_INHERIT } else { 0 },
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: sid.as_ptr() as *mut u16,
            },
        };
        let mut new_dacl: *mut ACL = std::ptr::null_mut();
        let rc = SetEntriesInAclW(1, &ea, old_dacl, &mut new_dacl);
        if rc != ERROR_SUCCESS {
            LocalFree(sd as _);
            return Err(Error::os("SetEntriesInAcl", rc));
        }
        let rc = SetNamedSecurityInfoW(
            wpath.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            new_dacl,
            std::ptr::null(),
        );
        LocalFree(new_dacl as _);
        LocalFree(sd as _);
        if rc != ERROR_SUCCESS {
            return Err(Error::os(format!("SetNamedSecurityInfo({})", path.display()), rc));
        }
    }
    Ok(())
}

/// Grant `access` on `path` to `sid`; with `inherit` the grant propagates to
/// everything created below a directory.
pub fn grant(path: &Path, sid: &Sid, access: u32, inherit: bool) -> Result<()> {
    apply(path, sid, access, GRANT_ACCESS as i32, inherit)
}

/// Remove every ACE for `sid` from `path`.
pub fn revoke(path: &Path, sid: &Sid) -> Result<()> {
    apply(path, sid, 0, REVOKE_ACCESS as i32, false)
}

/// Does the DACL of `path` contain an allow ACE for `sid` covering `access`?
pub fn has_grant(path: &Path, sid: &Sid, access: u32) -> Result<bool> {
    let wpath = wide(path.as_os_str());
    unsafe {
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let rc = GetNamedSecurityInfoW(
            wpath.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut sd,
        );
        if rc != ERROR_SUCCESS {
            return Err(Error::os(format!("GetNamedSecurityInfo({})", path.display()), rc));
        }
        let mut found = false;
        if !dacl.is_null() {
            for i in 0..(*dacl).AceCount as u32 {
                let mut ace: *mut core::ffi::c_void = std::ptr::null_mut();
                if GetAce(dacl, i, &mut ace) == 0 {
                    continue;
                }
                let a = ace as *const ACCESS_ALLOWED_ACE;
                if (*a).Header.AceType != ACCESS_ALLOWED_ACE_TYPE {
                    continue;
                }
                let ace_sid = std::ptr::addr_of!((*a).SidStart) as *mut core::ffi::c_void;
                if EqualSid(ace_sid, sid.as_ptr()) != 0 && (*a).Mask & access == access {
                    found = true;
                    break;
                }
            }
        }
        LocalFree(sd as _);
        Ok(found)
    }
}
