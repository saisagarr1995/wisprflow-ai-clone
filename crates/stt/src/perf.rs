//! Hardware-adaptive decode budget (design doc §11). Whisper passes used to
//! take fixed thread counts tuned on an 8-core machine; on a RAM-starved or
//! small-core laptop the concurrent partial + chunk decodes saturated every
//! core, making the whole system sluggish (and inviting thermal shutdown).
//! Instead the budget is derived once at startup from cores + memory, the
//! same idea as Meetily's `HardwareProfile::get_whisper_config` tiers.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Few cores or tight memory: leave real headroom for the OS and the
    /// foreground app. Dictation still works, captions are just calmer.
    Constrained,
    /// Enough hardware to run the tuned defaults.
    Standard,
}

/// Thread/cadence budget shared by every decode path. All passes together
/// must never claim every core: partials + chunk decodes overlap while the
/// user is speaking.
#[derive(Debug, Clone, Copy)]
pub struct PerfProfile {
    pub tier: Tier,
    /// Live-caption re-decode cadence.
    pub partial_interval_ms: u64,
    /// Threads for the small captions engine.
    pub partial_threads: i32,
    /// Threads for streaming finalizer chunk decodes (overlap partials).
    pub chunk_threads: i32,
    /// Threads for the release-time tail decode (runs alone, latency-critical).
    pub tail_threads: i32,
}

impl PerfProfile {
    /// Detected once per process; every decode path reads the same budget.
    pub fn get() -> &'static PerfProfile {
        static PROFILE: OnceLock<PerfProfile> = OnceLock::new();
        PROFILE.get_or_init(|| {
            let cores = std::thread::available_parallelism().map_or(4, |n| n.get() as i32);
            let profile = Self::for_hardware(cores, total_ram_mb());
            tracing::info!(?profile, cores, "decode budget");
            profile
        })
    }

    /// Pure classification so tests cover the policy without real hardware.
    /// `total_ram_mb` = 0 means "unknown" and classifies by cores alone.
    pub fn for_hardware(cores: i32, total_ram_mb: u64) -> PerfProfile {
        let tight_ram = total_ram_mb != 0 && total_ram_mb <= 8 * 1024;
        if cores <= 4 || tight_ram {
            PerfProfile {
                tier: Tier::Constrained,
                partial_interval_ms: 600,
                partial_threads: 2,
                chunk_threads: (cores / 2).clamp(2, 3),
                tail_threads: (cores - 2).max(2),
            }
        } else {
            PerfProfile {
                tier: Tier::Standard,
                partial_interval_ms: 350,
                partial_threads: 3,
                chunk_threads: 4,
                tail_threads: (cores - 1).max(2),
            }
        }
    }
}

/// Physical RAM in MB, 0 if it cannot be determined.
fn total_ram_mb() -> u64 {
    #[cfg(windows)]
    {
        // GlobalMemoryStatusEx via minimal FFI — not worth a sysinfo dep.
        #[repr(C)]
        struct MemoryStatusEx {
            length: u32,
            memory_load: u32,
            total_phys: u64,
            avail_phys: u64,
            total_page_file: u64,
            avail_page_file: u64,
            total_virtual: u64,
            avail_virtual: u64,
            avail_extended_virtual: u64,
        }
        extern "system" {
            fn GlobalMemoryStatusEx(buf: *mut MemoryStatusEx) -> i32;
        }
        let mut status = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page_file: 0,
            avail_page_file: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended_virtual: 0,
        };
        if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
            return status.total_phys / (1024 * 1024);
        }
        0
    }
    #[cfg(not(windows))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_laptop_is_constrained() {
        // The 7.4 GB / 8-core machine this was tuned on.
        let p = PerfProfile::for_hardware(8, 7600);
        assert_eq!(p.tier, Tier::Constrained);
        // Overlapping passes (partials + chunks) must leave cores free.
        assert!(p.partial_threads + p.chunk_threads < 8);
        assert_eq!(p.tail_threads, 6);
    }

    #[test]
    fn few_cores_are_constrained_even_with_ram() {
        let p = PerfProfile::for_hardware(4, 32 * 1024);
        assert_eq!(p.tier, Tier::Constrained);
        assert_eq!(p.chunk_threads, 2);
        assert_eq!(p.tail_threads, 2);
    }

    #[test]
    fn big_machine_keeps_tuned_defaults() {
        let p = PerfProfile::for_hardware(16, 32 * 1024);
        assert_eq!(p.tier, Tier::Standard);
        assert_eq!(p.partial_interval_ms, 350);
        assert_eq!(p.tail_threads, 15);
    }

    #[test]
    fn unknown_ram_classifies_by_cores() {
        assert_eq!(PerfProfile::for_hardware(8, 0).tier, Tier::Standard);
        assert_eq!(PerfProfile::for_hardware(2, 0).tier, Tier::Constrained);
    }

    #[test]
    fn detect_runs_on_this_machine() {
        let p = PerfProfile::get();
        assert!(p.partial_threads >= 2 && p.tail_threads >= 2);
    }
}
