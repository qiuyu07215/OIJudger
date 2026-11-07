//! Human readable descriptions for process exit codes / NTSTATUS values.

pub fn describe_exit_code(code: u32) -> String {
    let what = match code {
        0 => return "Exited normally".into(),
        0xC000_0005 => "Access violation (invalid memory access)",
        0xC000_00FD => "Stack overflow",
        0xC000_0094 => "Integer division by zero",
        0xC000_0095 => "Integer overflow",
        0xC000_008C => "Array bounds exceeded",
        0xC000_008E => "Floating point division by zero",
        0xC000_0090 => "Floating point invalid operation",
        0xC000_0091 => "Floating point overflow",
        0xC000_001D => "Illegal instruction",
        0xC000_0096 => "Privileged instruction",
        0xC000_0409 => "Stack buffer overrun / abort() / fast-fail",
        0xC000_0374 => "Heap corruption",
        0xC000_0017 => "Out of memory (allocation failed)",
        0xC000_012D => "Commitment limit reached (out of memory)",
        0xC000_0135 => "Required DLL not found",
        0xC000_0139 => "Entry point not found",
        0xC000_0142 => "DLL initialization failed",
        0xC000_013A => "Terminated (Ctrl+C)",
        0x8000_0003 => "Breakpoint (__debugbreak / assertion)",
        0x4000_0015 => "abort() called (fatal app exit)",
        3 => "abort() called / assertion failed (exit code 3)",
        _ if code >= 0xC000_0000 => return format!("Runtime error (NTSTATUS 0x{code:08X})"),
        _ => return format!("Exited with code {}", code as i32),
    };
    format!("{what} (0x{code:08X})")
}

/// Exit codes that mean "the process was killed by an exception" rather than
/// "the program chose to return a non-zero value".
pub fn is_crash_code(code: u32) -> bool {
    code >= 0xC000_0000 || code == 0x8000_0003 || code == 0x4000_0015
}
