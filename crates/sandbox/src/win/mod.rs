pub mod acl;
pub mod cpu;
mod util;

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, OwnedHandle};
use std::path::Path;
use std::time::Instant;

use windows_sys::Win32::Foundation::{
    SetHandleInformation, BOOL, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows_sys::Win32::Security::{PSID, SECURITY_CAPABILITIES};
use windows_sys::Win32::System::Diagnostics::Debug::{
    SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOGPFAULTERRORBOX, SEM_NOOPENFILEERRORBOX,
};
use windows_sys::Win32::System::JobObjects::{
    CreateJobObjectW, JobObjectAssociateCompletionPortInformation, JobObjectBasicAccountingInformation,
    JobObjectBasicUIRestrictions, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_ASSOCIATE_COMPLETION_PORT,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_BASIC_UI_RESTRICTIONS, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess, InitializeProcThreadAttributeList, ResumeThread,
    UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
    DETACHED_PROCESS, EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};
use windows_sys::Win32::System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED};

use crate::status::is_crash_code;
use crate::{Error, Limits, Outcome, Result, RunResult, RunSpec, Stdio};
use util::{owned, wide, Sid, SidFree};

// Win32 constants, defined locally to keep their integer types simple.
const PROC_THREAD_ATTRIBUTE_HANDLE_LIST: usize = 0x0002_0002;
const PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES: usize = 0x0002_0009;
const PROC_THREAD_ATTRIBUTE_JOB_LIST: usize = 0x0002_000D;

const JOB_OBJECT_LIMIT_JOB_TIME: u32 = 0x0000_0004;
const JOB_OBJECT_LIMIT_ACTIVE_PROCESS: u32 = 0x0000_0008;
const JOB_OBJECT_LIMIT_AFFINITY: u32 = 0x0000_0010;
const JOB_OBJECT_LIMIT_PROCESS_MEMORY: u32 = 0x0000_0100;
const JOB_OBJECT_LIMIT_JOB_MEMORY: u32 = 0x0000_0200;
const JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION: u32 = 0x0000_0400;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
/// Desktop, display settings, exit windows, global atoms, USER handles,
/// clipboard read/write, system parameters.
const JOB_OBJECT_UILIMIT_ALL: u32 = 0xFF;

const JOB_OBJECT_MSG_END_OF_JOB_TIME: u32 = 1;
const JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT: u32 = 3;
const JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT: u32 = 9;
const JOB_OBJECT_MSG_JOB_MEMORY_LIMIT: u32 = 10;

const ERROR_NOT_ENOUGH_MEMORY: i32 = 8;
const ERROR_COMMITMENT_LIMIT: i32 = 1455;
const ERROR_NOT_ENOUGH_QUOTA: i32 = 1816;
const STATUS_NO_MEMORY: u32 = 0xC000_0017;
const STATUS_COMMITMENT_LIMIT: u32 = 0xC000_012D;
const HRESULT_ALREADY_EXISTS: i32 = 0x8007_00B7_u32 as i32;
/// Exit code we use when we terminate the job ourselves.
const KILLED_EXIT_CODE: u32 = 0xDEAD;

/// SID shared by every AppContainer process ("ALL APPLICATION PACKAGES").
const ALL_APP_PACKAGES: &str = "S-1-15-2-1";

/// Process wide setup; call once at startup. Children inherit the error
/// mode, so a crashing program never pops up a Windows Error Reporting dialog.
pub fn init_process() {
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);
    }
    cpu::tsc_hz();
}

/// An AppContainer identity. Every judging worker owns one, so programs run
/// by different workers cannot see each other's working directories.
pub struct Container {
    name: String,
    sid: Option<Sid>,
}

impl Container {
    /// Create (or reuse) the AppContainer profile `name` for running
    /// untrusted programs. The container's profile folder is locked, so the
    /// only writable location is what [`Container::grant_dir`] grants.
    pub fn open(name: &str) -> Result<Container> {
        let c = Self::open_profile(name)?;
        lock_profile_folder(name, c.sid.as_ref().unwrap())?;
        Ok(c)
    }

    /// Like [`Container::open`], but keeps the profile's TEMP folder writable
    /// (and empties it). For compilers: on Windows GCC can only use TEMP for
    /// intermediate files, and CreateProcess forces TEMP into the profile.
    /// Never run contestant programs in such a container.
    pub fn open_scratch(name: &str) -> Result<Container> {
        let c = Self::open_profile(name)?;
        if let Some(ac) = profile_folder(name) {
            let sid = c.sid.as_ref().unwrap();
            if ac.exists() && !acl::has_grant(&ac, sid, acl::ACCESS_FULL)? {
                acl::grant(&ac, sid, acl::ACCESS_FULL, true)?;
            }
        }
        c.wipe_profile_temp();
        Ok(c)
    }

    /// Delete everything in the container profile's TEMP folder.
    pub fn wipe_profile_temp(&self) {
        if let Some(ac) = profile_folder(&self.name) {
            wipe_dir(&ac.join("Temp"));
        }
    }

    fn open_profile(name: &str) -> Result<Container> {
        let wname = wide(name);
        let mut sid: PSID = std::ptr::null_mut();
        let hr = unsafe {
            CreateAppContainerProfile(
                wname.as_ptr(),
                wname.as_ptr(),
                wide("OIJudger sandbox").as_ptr(),
                std::ptr::null(),
                0,
                &mut sid,
            )
        };
        if hr == HRESULT_ALREADY_EXISTS {
            let hr = unsafe { DeriveAppContainerSidFromAppContainerName(wname.as_ptr(), &mut sid) };
            if hr < 0 {
                return Err(Error::Other(format!("DeriveAppContainerSid({name}) failed: 0x{hr:08X}")));
            }
        } else if hr < 0 {
            return Err(Error::Other(format!("CreateAppContainerProfile({name}) failed: 0x{hr:08X}")));
        }
        let sid = unsafe { Sid::from_raw(sid, SidFree::FreeSid) };
        Ok(Container { name: name.to_string(), sid: Some(sid) })
    }

    /// A container that does **not** isolate (job limits only). Only for
    /// diagnostics; never used for untrusted code by default.
    pub fn unisolated() -> Container {
        Container { name: String::from("<none>"), sid: None }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_isolated(&self) -> bool {
        self.sid.is_some()
    }

    /// Remove an AppContainer profile created by [`Container::open`].
    pub fn delete_profile(name: &str) {
        unsafe {
            DeleteAppContainerProfile(wide(name).as_ptr());
        }
    }

    /// Give this container full access to `dir` and everything created in it.
    pub fn grant_dir(&self, dir: &Path) -> Result<()> {
        match &self.sid {
            Some(sid) => acl::grant(dir, sid, acl::ACCESS_FULL, true),
            None => Ok(()),
        }
    }

    /// Give this container read-only access to a single file or directory tree.
    pub fn grant_read(&self, path: &Path) -> Result<()> {
        match &self.sid {
            Some(sid) => acl::grant(path, sid, acl::ACCESS_READ_EXECUTE, true),
            None => Ok(()),
        }
    }

    pub fn revoke(&self, path: &Path) -> Result<()> {
        match &self.sid {
            Some(sid) => acl::revoke(path, sid),
            None => Ok(()),
        }
    }

    /// Start a process; use [`Child::wait`] to collect the result.
    pub fn spawn(&self, spec: RunSpec) -> Result<Child> {
        spawn(self.sid.as_ref(), spec)
    }

    /// Start a process and wait for it.
    pub fn run(&self, spec: RunSpec) -> Result<RunResult> {
        Ok(self.spawn(spec)?.wait())
    }
}

fn profile_folder(name: &str) -> Option<std::path::PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(Path::new(&local).join("Packages").join(name.to_lowercase()).join("AC"))
}

fn wipe_dir(dir: &Path) {
    if let Ok(inner) = std::fs::read_dir(dir) {
        for f in inner.flatten() {
            let _ = std::fs::remove_dir_all(f.path()).or_else(|_| std::fs::remove_file(f.path()));
        }
    }
}

/// Windows gives every AppContainer a private, writable profile folder
/// (`%LOCALAPPDATA%\Packages\<name>\AC`, which also becomes the child's
/// TEMP). It survives between runs, so one contestant's program could leave
/// data for the next one. Remove the container's access to it.
fn lock_profile_folder(name: &str, sid: &Sid) -> Result<()> {
    let Some(ac) = profile_folder(name) else { return Ok(()) };
    if !ac.exists() {
        return Ok(());
    }
    wipe_dir(&ac.join("Temp"));
    if acl::has_grant(&ac, sid, 0x2)? {
        acl::revoke(&ac, sid)?;
    }
    Ok(())
}

/// Compiler installations used inside the sandbox must be readable by
/// AppContainer processes.
pub struct Toolchain;

impl Toolchain {
    /// Make `dir` (recursively) readable and executable by every AppContainer
    /// process. Returns `true` if the ACL had to be changed. Directories under
    /// `C:\Program Files` usually already allow this.
    pub fn grant_read(dir: &Path) -> Result<bool> {
        let sid = Sid::parse(ALL_APP_PACKAGES)?;
        if acl::has_grant(dir, &sid, acl::ACCESS_READ_EXECUTE)? {
            return Ok(false);
        }
        acl::grant(dir, &sid, acl::ACCESS_READ_EXECUTE, true)?;
        Ok(true)
    }

    pub fn is_readable(dir: &Path) -> Result<bool> {
        let sid = Sid::parse(ALL_APP_PACKAGES)?;
        acl::has_grant(dir, &sid, acl::ACCESS_READ_EXECUTE)
    }
}

/// A running sandboxed process.
pub struct Child {
    state: ChildState,
}

enum ChildState {
    Running(Running),
    /// The process could not even start for a reason that is a verdict
    /// (e.g. a static array larger than the memory limit).
    Finished(RunResult),
}

struct Running {
    process: OwnedHandle,
    job: OwnedHandle,
    port: OwnedHandle,
    start: Instant,
    limits: Limits,
}

unsafe impl Send for Child {}

fn null_file() -> Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("NUL")
        .map_err(|e| Error::Io { context: "open NUL".into(), source: e })
}

fn stdio_file(s: Stdio) -> Result<File> {
    match s {
        Stdio::Null => null_file(),
        Stdio::File(f) => Ok(f),
    }
}

fn environment_block(spec: &RunSpec) -> Vec<u16> {
    let mut vars: Vec<(OsString, OsString)> = Vec::new();
    // CreateProcess rewrites LOCALAPPDATA / TEMP / TMP for AppContainer
    // processes and fails with ERROR_ENVVAR_NOT_FOUND when they are absent.
    for k in [
        "SystemRoot",
        "windir",
        "SystemDrive",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
    ] {
        if let Some(v) = std::env::var_os(k) {
            vars.push((k.into(), v));
        }
    }
    let sysroot = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let mut path = sysroot.clone();
    path.push("\\System32");
    vars.push(("PATH".into(), path));
    vars.push(("TEMP".into(), spec.cwd.clone().into_os_string()));
    vars.push(("TMP".into(), spec.cwd.clone().into_os_string()));
    for (k, v) in &spec.env {
        let key = k.to_string_lossy().to_ascii_uppercase();
        vars.retain(|(ek, _)| ek.to_string_lossy().to_ascii_uppercase() != key);
        vars.push((k.clone(), v.clone()));
    }
    vars.sort_by_key(|(k, _)| k.to_string_lossy().to_ascii_uppercase());
    let mut block = Vec::new();
    for (k, v) in vars {
        block.extend(k.encode_wide());
        block.push(b'=' as u16);
        block.extend(v.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

fn command_line(spec: &RunSpec) -> Vec<u16> {
    let mut cmd = Vec::new();
    util::quote_arg(spec.program.as_os_str(), &mut cmd);
    for a in &spec.args {
        cmd.push(b' ' as u16);
        util::quote_arg(a, &mut cmd);
    }
    cmd.push(0);
    cmd
}

fn create_job(limits: &Limits) -> Result<(OwnedHandle, OwnedHandle)> {
    let job = owned(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) }, "CreateJobObject")?;
    let port = owned(
        unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 1) },
        "CreateIoCompletionPort",
    )?;
    unsafe {
        let mut ext: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        let mut flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_JOB_TIME;
        ext.BasicLimitInformation.ActiveProcessLimit = limits.max_processes.max(1);
        // Kernel backstop for CPU time (user mode only); the precise check
        // on user + kernel time happens in the wait loop.
        ext.BasicLimitInformation.PerJobUserTimeLimit = ((limits.cpu_ms + 500) * 10_000) as i64;
        if limits.memory_bytes > 0 {
            if limits.memory_per_job {
                flags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
                ext.JobMemoryLimit = limits.memory_bytes as usize;
            } else {
                flags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
                ext.ProcessMemoryLimit = limits.memory_bytes as usize;
            }
        }
        if limits.affinity != 0 {
            flags |= JOB_OBJECT_LIMIT_AFFINITY;
            ext.BasicLimitInformation.Affinity = limits.affinity;
        }
        ext.BasicLimitInformation.LimitFlags = flags;
        set_job_info(&job, JobObjectExtendedLimitInformation, &ext, "job limits")?;

        let ui = JOBOBJECT_BASIC_UI_RESTRICTIONS { UIRestrictionsClass: JOB_OBJECT_UILIMIT_ALL };
        set_job_info(&job, JobObjectBasicUIRestrictions, &ui, "job UI restrictions")?;

        let assoc = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
            CompletionKey: std::ptr::null_mut(),
            CompletionPort: port.as_raw_handle() as HANDLE,
        };
        set_job_info(&job, JobObjectAssociateCompletionPortInformation, &assoc, "job completion port")?;
    }
    Ok((job, port))
}

unsafe fn set_job_info<T>(job: &OwnedHandle, class: i32, value: &T, what: &str) -> Result<()> {
    let ok: BOOL = SetInformationJobObject(
        job.as_raw_handle() as HANDLE,
        class as _,
        value as *const T as *const _,
        std::mem::size_of::<T>() as u32,
    );
    if ok == 0 {
        return Err(Error::last_os(format!("SetInformationJobObject({what})")));
    }
    Ok(())
}

fn spawn(sid: Option<&Sid>, mut spec: RunSpec) -> Result<Child> {
    let stdin = stdio_file(std::mem::replace(&mut spec.stdin, Stdio::Null))?;
    let stdout = stdio_file(std::mem::replace(&mut spec.stdout, Stdio::Null))?;
    let stderr = stdio_file(std::mem::replace(&mut spec.stderr, Stdio::Null))?;

    let (job, port) = create_job(&spec.limits)?;

    // Only these handles are inherited (PROC_THREAD_ATTRIBUTE_HANDLE_LIST),
    // so concurrent spawns on other threads cannot leak handles into us.
    let std_handles =
        [stdin.as_raw_handle() as HANDLE, stdout.as_raw_handle() as HANDLE, stderr.as_raw_handle() as HANDLE];
    let mut inherit: Vec<HANDLE> = Vec::with_capacity(3);
    for h in std_handles {
        if !inherit.contains(&h) {
            inherit.push(h);
        }
        unsafe {
            SetHandleInformation(h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT);
        }
    }
    let job_list: [HANDLE; 1] = [job.as_raw_handle() as HANDLE];
    let mut caps: SECURITY_CAPABILITIES = unsafe { std::mem::zeroed() };
    if let Some(sid) = sid {
        caps.AppContainerSid = sid.as_ptr();
    }

    let attr_count = if sid.is_some() { 3 } else { 2 };
    let mut attr_size: usize = 0;
    unsafe {
        InitializeProcThreadAttributeList(std::ptr::null_mut(), attr_count, 0, &mut attr_size);
    }
    let mut attr_buf = vec![0u64; attr_size.div_ceil(8)];
    let attrs = attr_buf.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
    unsafe {
        if InitializeProcThreadAttributeList(attrs, attr_count, 0, &mut attr_size) == 0 {
            return Err(Error::last_os("InitializeProcThreadAttributeList"));
        }
    }
    struct AttrGuard(LPPROC_THREAD_ATTRIBUTE_LIST);
    impl Drop for AttrGuard {
        fn drop(&mut self) {
            unsafe { DeleteProcThreadAttributeList(self.0) }
        }
    }
    let _guard = AttrGuard(attrs);

    unsafe {
        let upd = |attr: usize, ptr: *const core::ffi::c_void, size: usize, what: &str| -> Result<()> {
            if UpdateProcThreadAttribute(attrs, 0, attr, ptr, size, std::ptr::null_mut(), std::ptr::null()) == 0 {
                return Err(Error::last_os(format!("UpdateProcThreadAttribute({what})")));
            }
            Ok(())
        };
        upd(
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            inherit.as_ptr() as *const _,
            inherit.len() * std::mem::size_of::<HANDLE>(),
            "handle list",
        )?;
        upd(PROC_THREAD_ATTRIBUTE_JOB_LIST, job_list.as_ptr() as *const _, std::mem::size_of::<HANDLE>(), "job list")?;
        if sid.is_some() {
            upd(
                PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
                &caps as *const SECURITY_CAPABILITIES as *const _,
                std::mem::size_of::<SECURITY_CAPABILITIES>(),
                "security capabilities",
            )?;
        }
    }

    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = std_handles[0];
    si.StartupInfo.hStdOutput = std_handles[1];
    si.StartupInfo.hStdError = std_handles[2];
    si.lpAttributeList = attrs;

    let app = wide(spec.program.as_os_str());
    let mut cmd = command_line(&spec);
    let env = environment_block(&spec);
    let cwd = wide(spec.cwd.as_os_str());
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // Single-process runs get no console at all: CREATE_NO_WINDOW would still
    // start a hidden conhost.exe per run (~15 ms); std streams are redirected
    // anyway. Multi-process runs (compilers) keep a hidden console so their
    // children inherit it instead of each opening a visible one.
    let console = if spec.limits.max_processes > 1 { CREATE_NO_WINDOW } else { DETACHED_PROCESS };
    let ok = unsafe {
        CreateProcessW(
            app.as_ptr(),
            cmd.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | console | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED,
            env.as_ptr() as *const _,
            cwd.as_ptr(),
            &si.StartupInfo,
            &mut pi,
        )
    };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        let code = err.raw_os_error().unwrap_or(0);
        if spec.limits.memory_bytes > 0
            && matches!(code, ERROR_NOT_ENOUGH_MEMORY | ERROR_COMMITMENT_LIMIT | ERROR_NOT_ENOUGH_QUOTA)
        {
            return Ok(Child {
                state: ChildState::Finished(RunResult {
                    outcome: Outcome::MemoryLimit,
                    exit_code: STATUS_NO_MEMORY,
                    cpu_ms: 0,
                    wall_ms: 0,
                    peak_memory_bytes: spec.limits.memory_bytes,
                }),
            });
        }
        return Err(Error::Io { context: format!("CreateProcess({})", spec.program.display()), source: err });
    }
    let process = owned(pi.hProcess, "process handle")?;
    let thread = owned(pi.hThread, "thread handle")?;
    let start = Instant::now();
    unsafe {
        ResumeThread(thread.as_raw_handle() as HANDLE);
    }
    drop(thread);
    // Our copies of the child's std handles are no longer needed; for pipes
    // this is what lets the other side observe EOF.
    drop((stdin, stdout, stderr));

    Ok(Child { state: ChildState::Running(Running { process, job, port, start, limits: spec.limits }) })
}

#[derive(Default)]
struct Flags {
    memory: bool,
    process_limit: bool,
    job_time: bool,
}

impl Running {
    fn h(&self, h: &OwnedHandle) -> HANDLE {
        h.as_raw_handle() as HANDLE
    }

    /// Precise CPU time of the main process from its cycle count; `None`
    /// when unavailable (then job accounting is used).
    fn precise_cpu_ms(&self) -> Option<u64> {
        use windows_sys::Win32::System::WindowsProgramming::QueryProcessCycleTime;
        let hz = cpu::tsc_hz();
        if hz <= 0.0 || self.limits.max_processes > 1 {
            return None;
        }
        let mut cycles: u64 = 0;
        if unsafe { QueryProcessCycleTime(self.h(&self.process), &mut cycles) } == 0 {
            return None;
        }
        Some((cycles as f64 * 1000.0 / hz).round() as u64)
    }

    /// (CPU ms, peak memory bytes). CPU time is the larger of the precise
    /// cycle-based value and job accounting, so tick granularity can never
    /// hide time that was really used.
    fn accounting(&self) -> (u64, u64) {
        let (job_cpu, peak) = self.job_accounting();
        match self.precise_cpu_ms() {
            // Job accounting lags by up to one 15.6 ms tick; trust the precise
            // value unless the job reports clearly more (other processes).
            Some(p) if job_cpu <= p + 32 => (p, peak),
            _ => (job_cpu, peak),
        }
    }

    fn job_accounting(&self) -> (u64, u64) {
        unsafe {
            let mut acc: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            QueryInformationJobObject(
                self.h(&self.job),
                JobObjectBasicAccountingInformation,
                &mut acc as *mut _ as *mut _,
                std::mem::size_of_val(&acc) as u32,
                std::ptr::null_mut(),
            );
            let cpu = (acc.TotalUserTime as u64 + acc.TotalKernelTime as u64) / 10_000;
            let mut ext: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            QueryInformationJobObject(
                self.h(&self.job),
                JobObjectExtendedLimitInformation,
                &mut ext as *mut _ as *mut _,
                std::mem::size_of_val(&ext) as u32,
                std::ptr::null_mut(),
            );
            let peak = if self.limits.memory_per_job { ext.PeakJobMemoryUsed } else { ext.PeakProcessMemoryUsed };
            (cpu, peak as u64)
        }
    }

    fn drain_messages(&self, flags: &mut Flags) {
        loop {
            let mut msg: u32 = 0;
            let mut key: usize = 0;
            let mut ov: *mut OVERLAPPED = std::ptr::null_mut();
            let ok = unsafe { GetQueuedCompletionStatus(self.h(&self.port), &mut msg, &mut key, &mut ov, 0) };
            if ok == 0 {
                break;
            }
            match msg {
                JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT | JOB_OBJECT_MSG_JOB_MEMORY_LIMIT => flags.memory = true,
                JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT => flags.process_limit = true,
                JOB_OBJECT_MSG_END_OF_JOB_TIME => flags.job_time = true,
                _ => {}
            }
        }
    }

    fn output_size(&self) -> u64 {
        self.limits.watch_files.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum()
    }

    fn kill(&self) {
        unsafe {
            TerminateJobObject(self.h(&self.job), KILLED_EXIT_CODE);
            WaitForSingleObject(self.h(&self.process), 5000);
        }
    }

    fn wait(self) -> RunResult {
        let lim = &self.limits;
        let mut flags = Flags::default();
        let mut forced: Option<Outcome> = None;
        loop {
            let (cpu, _) = self.accounting();
            let wall = self.start.elapsed().as_millis() as u64;
            self.drain_messages(&mut flags);
            if flags.memory {
                forced = Some(Outcome::MemoryLimit);
            } else if cpu > lim.cpu_ms || flags.job_time {
                forced = Some(Outcome::TimeLimit);
            } else if wall > lim.wall_ms {
                forced = Some(Outcome::IdleLimit);
            } else if lim.output_bytes > 0 && self.output_size() > lim.output_bytes {
                forced = Some(Outcome::OutputLimit);
            }
            if forced.is_some() {
                self.kill();
                break;
            }
            // CPU time never runs faster than the wall clock, so sleeping for
            // the remaining CPU budget cannot overshoot the limit by much.
            let slice = (lim.cpu_ms - cpu).min(lim.wall_ms - wall).clamp(1, 100) as u32 + 1;
            if unsafe { WaitForSingleObject(self.h(&self.process), slice) } == WAIT_OBJECT_0 {
                break;
            }
        }
        let wall_ms = self.start.elapsed().as_millis() as u64;
        let mut exit_code: u32 = 0;
        unsafe {
            GetExitCodeProcess(self.h(&self.process), &mut exit_code);
            // Kill stragglers (e.g. children of a compiler driver).
            TerminateJobObject(self.h(&self.job), KILLED_EXIT_CODE);
        }
        self.drain_messages(&mut flags);
        let (cpu_ms, peak) = self.accounting();

        let outcome = if let Some(o) = forced {
            o
        } else if flags.memory {
            Outcome::MemoryLimit
        } else if cpu_ms > lim.cpu_ms || flags.job_time {
            Outcome::TimeLimit
        } else if lim.output_bytes > 0 && self.output_size() > lim.output_bytes {
            Outcome::OutputLimit
        } else if exit_code == 0 {
            Outcome::Ok
        } else if flags.process_limit {
            Outcome::ProcessLimit
        } else if lim.memory_bytes > 0
            && (exit_code == STATUS_NO_MEMORY
                || exit_code == STATUS_COMMITMENT_LIMIT
                || (is_crash_code(exit_code) || exit_code == 3) && peak * 100 >= lim.memory_bytes * 97)
        {
            Outcome::MemoryLimit
        } else if is_crash_code(exit_code) {
            Outcome::Crashed
        } else {
            Outcome::NonZeroExit
        };
        let cpu_ms = if outcome == Outcome::TimeLimit { cpu_ms.max(lim.cpu_ms + 1) } else { cpu_ms };
        RunResult { outcome, exit_code, cpu_ms, wall_ms, peak_memory_bytes: peak }
    }
}

impl Child {
    /// Wait for the process to finish, enforcing all limits.
    pub fn wait(self) -> RunResult {
        match self.state {
            ChildState::Finished(r) => r,
            ChildState::Running(r) => r.wait(),
        }
    }

    /// Kill the process tree immediately.
    pub fn kill(&self) {
        if let ChildState::Running(r) = &self.state {
            r.kill();
        }
    }
}

/// Create an anonymous pipe; returns (read end, write end).
pub fn pipe() -> Result<(File, File)> {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::System::Pipes::CreatePipe;
    let mut r: HANDLE = std::ptr::null_mut();
    let mut w: HANDLE = std::ptr::null_mut();
    if unsafe { CreatePipe(&mut r, &mut w, std::ptr::null(), 1 << 16) } == 0 {
        return Err(Error::last_os("CreatePipe"));
    }
    Ok(unsafe { (File::from_raw_handle(r as _), File::from_raw_handle(w as _)) })
}
