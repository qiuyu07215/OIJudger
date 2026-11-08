//! LemonLime's compiler configurations on this computer.
//!
//! A `.cdf` only names a configuration per compiler (`{"g++": "CCF"}`); the
//! arguments live in LemonLime's settings, which Qt keeps in the registry
//! under `HKCU\Software\LemonLime\lemon\v1.2\CompilerSettings\<n>` (arrays
//! are numbered subkeys starting at 1).

use judge_core::Lang;

/// The arguments of configuration `config` of LemonLime compiler `compiler`.
pub fn compiler_arguments(compiler: &str, config: &str) -> Option<String> {
    for i in 1.. {
        let key = format!(r"Software\LemonLime\lemon\v1.2\CompilerSettings\{i}");
        let name = reg::string(&key, "CompilerName")?;
        if !name.eq_ignore_ascii_case(compiler) {
            continue;
        }
        for j in 1.. {
            let ckey = format!(r"{key}\Configuration\{j}");
            let cname = reg::string(&ckey, "Name")?;
            if cname == config {
                return reg::string(&ckey, "CompilerArguments");
            }
        }
    }
    None
}

/// The language a LemonLime compiler entry is for ("g++" → C++, "gcc" → C).
pub fn compiler_lang(compiler: &str) -> Option<Lang> {
    let n = compiler.to_ascii_lowercase();
    if n.contains("++") || n.contains("cpp") || n.contains("cxx") {
        Some(Lang::Cpp)
    } else if n.contains("gcc") || n == "c" || n.starts_with("clang") {
        Some(Lang::C)
    } else {
        None
    }
}

/// LemonLime arguments without the output / source placeholders
/// (`-o %s %s.*`), which OIJudger adds itself.
pub fn strip_placeholders(args: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut it = args.split_whitespace().peekable();
    while let Some(a) = it.next() {
        if a == "-o" && it.peek().is_some_and(|n| n.contains("%s")) {
            it.next();
            continue;
        }
        if a.contains("%s") {
            continue;
        }
        out.push(a);
    }
    out.join(" ")
}

#[cfg(windows)]
mod reg {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// A string value of a key under HKEY_CURRENT_USER.
    pub fn string(key: &str, value: &str) -> Option<String> {
        let (k, v) = (wide(key), wide(value));
        let mut len: u32 = 0;
        // Safety: valid NUL-terminated strings; the first call only asks for the size.
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                k.as_ptr(),
                v.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut len,
            )
        };
        if r != ERROR_SUCCESS || len == 0 {
            return None;
        }
        let mut buf = vec![0u16; (len as usize).div_ceil(2)];
        // Safety: `buf` holds `len` bytes.
        let r = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                k.as_ptr(),
                v.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            )
        };
        if r != ERROR_SUCCESS {
            return None;
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]))
    }
}

#[cfg(not(windows))]
mod reg {
    pub fn string(_key: &str, _value: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders() {
        assert_eq!(
            strip_placeholders("-o %s %s.* -lm -O2 -std=c++14 -static -Wl,--stack=2147483647"),
            "-lm -O2 -std=c++14 -static -Wl,--stack=2147483647"
        );
        assert_eq!(strip_placeholders("%s.* -o %s -O2"), "-O2");
        assert_eq!(compiler_lang("g++"), Some(Lang::Cpp));
        assert_eq!(compiler_lang("gcc"), Some(Lang::C));
        assert_eq!(compiler_lang("fpc"), None);
    }
}
