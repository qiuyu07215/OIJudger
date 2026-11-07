use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, OwnedHandle};

use windows_sys::Win32::Foundation::{LocalFree, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::FreeSid;
use windows_sys::Win32::Security::PSID;

use crate::{Error, Result};

/// NUL-terminated UTF-16 copy of `s`.
pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

/// Take ownership of a raw handle returned by a Win32 call.
pub fn owned(h: HANDLE, context: &str) -> Result<OwnedHandle> {
    if h.is_null() || h == INVALID_HANDLE_VALUE {
        return Err(Error::last_os(context));
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(h) })
}

/// How a SID pointer has to be released.
#[derive(Clone, Copy)]
pub enum SidFree {
    FreeSid,
    LocalFree,
}

/// An owned SID.
pub struct Sid {
    ptr: PSID,
    free: SidFree,
}

unsafe impl Send for Sid {}
unsafe impl Sync for Sid {}

impl Sid {
    /// # Safety
    /// `ptr` must be a valid SID that has to be released with `free`.
    pub unsafe fn from_raw(ptr: PSID, free: SidFree) -> Self {
        Sid { ptr, free }
    }
    pub fn as_ptr(&self) -> PSID {
        self.ptr
    }

    /// Parse a string SID such as `S-1-15-2-1`.
    pub fn parse(s: &str) -> Result<Sid> {
        use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
        let w = wide(s);
        let mut p: PSID = std::ptr::null_mut();
        if unsafe { ConvertStringSidToSidW(w.as_ptr(), &mut p) } == 0 {
            return Err(Error::last_os(format!("ConvertStringSidToSid({s})")));
        }
        Ok(unsafe { Sid::from_raw(p, SidFree::LocalFree) })
    }
}

impl Drop for Sid {
    fn drop(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        unsafe {
            match self.free {
                SidFree::FreeSid => {
                    FreeSid(self.ptr);
                }
                SidFree::LocalFree => {
                    LocalFree(self.ptr as _);
                }
            }
        }
    }
}

/// Quote one argument following the MSVC CRT / CommandLineToArgvW rules.
pub fn quote_arg(arg: &OsStr, out: &mut Vec<u16>) {
    let a: Vec<u16> = arg.encode_wide().collect();
    let needs = a.is_empty() || a.iter().any(|&c| c == b' ' as u16 || c == b'\t' as u16 || c == b'"' as u16);
    if !needs {
        out.extend_from_slice(&a);
        return;
    }
    out.push(b'"' as u16);
    let mut backslashes = 0usize;
    for &c in &a {
        if c == b'\\' as u16 {
            backslashes += 1;
        } else {
            if c == b'"' as u16 {
                out.extend(std::iter::repeat(b'\\' as u16).take(backslashes * 2 + 1));
            } else {
                out.extend(std::iter::repeat(b'\\' as u16).take(backslashes));
            }
            backslashes = 0;
            out.push(c);
        }
    }
    out.extend(std::iter::repeat(b'\\' as u16).take(backslashes * 2));
    out.push(b'"' as u16);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    fn q(s: &str) -> String {
        let mut v = Vec::new();
        quote_arg(OsStr::new(s), &mut v);
        OsString::from_wide(&v).into_string().unwrap()
    }

    #[test]
    fn quoting() {
        assert_eq!(q("abc"), "abc");
        assert_eq!(q(""), "\"\"");
        assert_eq!(q("a b"), "\"a b\"");
        assert_eq!(q("a\"b"), "\"a\\\"b\"");
        assert_eq!(q("C:\\dir with space\\"), "\"C:\\dir with space\\\\\"");
    }
}
