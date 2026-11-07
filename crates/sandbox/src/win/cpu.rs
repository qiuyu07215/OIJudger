//! CPU topology: find physical cores and their efficiency class so judging
//! workers can be pinned to distinct performance cores (stable timing on
//! hybrid P/E-core CPUs).

use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};

#[derive(Debug, Clone, Copy)]
pub struct Core {
    pub efficiency_class: u8,
    pub group: u16,
    pub mask: usize,
    /// Core has more than one logical processor (SMT / Hyper-Threading).
    #[allow(dead_code)]
    pub smt: bool,
}

pub fn cores() -> Vec<Core> {
    let mut len: u32 = 0;
    unsafe {
        GetLogicalProcessorInformationEx(RelationProcessorCore, std::ptr::null_mut(), &mut len);
    }
    if len == 0 {
        return Vec::new();
    }
    // u64 storage for alignment.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    let ok = unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
            &mut len,
        )
    };
    if ok == 0 {
        return Vec::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, len as usize) };
    let rd16 = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let rd32 = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let rdus = |o: usize| usize::from_le_bytes(bytes[o..o + std::mem::size_of::<usize>()].try_into().unwrap());

    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 8 <= bytes.len() {
        let size = rd32(off + 4) as usize;
        if size == 0 || off + size > bytes.len() {
            break;
        }
        // PROCESSOR_RELATIONSHIP at +8: Flags u8, EfficiencyClass u8,
        // Reserved[20], GroupCount u16, GroupMask[] (GROUP_AFFINITY, at +24).
        let pr = off + 8;
        let flags = bytes[pr];
        let eff = bytes[pr + 1];
        let group_count = rd16(pr + 22) as usize;
        if group_count >= 1 {
            let ga = pr + 24;
            out.push(Core { efficiency_class: eff, group: rd16(ga + 8), mask: rdus(ga), smt: flags & 1 != 0 });
        }
        off += size;
    }
    out
}

/// One single-bit affinity mask (group 0) per physical performance core.
pub fn performance_core_masks() -> Vec<usize> {
    let cores: Vec<Core> = cores().into_iter().filter(|c| c.group == 0 && c.mask != 0).collect();
    let best = cores.iter().map(|c| c.efficiency_class).max().unwrap_or(0);
    cores
        .iter()
        .filter(|c| c.efficiency_class == best)
        .map(|c| c.mask & c.mask.wrapping_neg()) // lowest logical processor of the core
        .collect()
}

/// Frequency of the invariant time stamp counter, calibrated once against
/// the performance counter. `QueryProcessCycleTime` counts TSC ticks while
/// the process runs, so dividing by this gives CPU time with microsecond
/// resolution (job accounting only advances in 15.6 ms timer ticks).
pub fn tsc_hz() -> f64 {
    use std::sync::OnceLock;
    static HZ: OnceLock<f64> = OnceLock::new();
    *HZ.get_or_init(|| {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use core::arch::x86_64::_rdtsc;
            let mut best = 0.0f64;
            // Take the median-ish of a few short samples to dodge preemption.
            let mut samples = Vec::new();
            for _ in 0..5 {
                let t0 = std::time::Instant::now();
                let c0 = _rdtsc();
                while t0.elapsed() < std::time::Duration::from_millis(10) {
                    std::hint::spin_loop();
                }
                let c1 = _rdtsc();
                let dt = t0.elapsed().as_secs_f64();
                samples.push((c1 - c0) as f64 / dt);
            }
            samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if let Some(m) = samples.get(samples.len() / 2) {
                best = *m;
            }
            best
        }
        #[cfg(not(target_arch = "x86_64"))]
        0.0
    })
}
