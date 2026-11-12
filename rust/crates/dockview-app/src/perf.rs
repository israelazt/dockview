//! Process-level performance measurements (CPU and memory).

use std::time::Instant;

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// CPU used over an interval, as a percentage of *one* core.
///
/// `cpu_100ns` is the CPU time consumed (kernel + user) and `wall_secs` the
/// elapsed real time. A fully busy single thread gives 100.
pub fn cpu_core_percent(cpu_100ns: u64, wall_secs: f64) -> f32 {
    if wall_secs <= 0.0 {
        return 0.0;
    }
    let cpu_secs = cpu_100ns as f64 / 10_000_000.0;
    (cpu_secs / wall_secs * 100.0) as f32
}

fn filetime_100ns(ft: FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

/// What the app is costing the machine right now.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProcessUsage {
    /// Percent of one core.
    pub cpu_core_percent: f32,
    /// Percent of the whole CPU (all logical cores).
    pub cpu_total_percent: f32,
    pub memory_mb: f32,
}

/// Samples this process's CPU time between calls.
pub struct ProcessSampler {
    last_cpu: u64,
    last_wall: Instant,
    logical_cores: f32,
}

impl ProcessSampler {
    pub fn new() -> Self {
        Self {
            last_cpu: process_cpu_time_100ns().unwrap_or(0),
            last_wall: Instant::now(),
            logical_cores: std::thread::available_parallelism().map_or(1, |n| n.get()) as f32,
        }
    }

    /// CPU use since the previous sample, plus current memory.
    pub fn sample(&mut self) -> ProcessUsage {
        let now = Instant::now();
        let cpu = process_cpu_time_100ns().unwrap_or(self.last_cpu);
        let wall = now.duration_since(self.last_wall).as_secs_f64();
        let core_percent = cpu_core_percent(cpu.saturating_sub(self.last_cpu), wall);
        self.last_cpu = cpu;
        self.last_wall = now;

        ProcessUsage {
            cpu_core_percent: core_percent,
            cpu_total_percent: core_percent / self.logical_cores,
            memory_mb: working_set_bytes().map_or(0.0, |b| b as f32 / (1024.0 * 1024.0)),
        }
    }
}

impl Default for ProcessSampler {
    fn default() -> Self {
        Self::new()
    }
}

fn process_cpu_time_100ns() -> Option<u64> {
    let (mut creation, mut exit, mut kernel, mut user) = Default::default();
    // SAFETY: all out-pointers refer to live FILETIME values.
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
        .ok()?;
    }
    Some(filetime_100ns(kernel) + filetime_100ns(user))
}

fn working_set_bytes() -> Option<usize> {
    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: `counters` is a live, correctly sized PROCESS_MEMORY_COUNTERS.
    unsafe {
        GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb).ok()?;
    }
    Some(counters.WorkingSetSize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_busy_core_is_one_hundred_percent() {
        // 2 s of CPU time over 2 s of wall time.
        assert!((cpu_core_percent(20_000_000, 2.0) - 100.0).abs() < 1e-3);
        assert!((cpu_core_percent(5_000_000, 2.0) - 25.0).abs() < 1e-3);
    }

    #[test]
    fn zero_or_negative_intervals_do_not_divide_by_zero() {
        assert_eq!(cpu_core_percent(1_000, 0.0), 0.0);
        assert_eq!(cpu_core_percent(1_000, -1.0), 0.0);
    }

    #[test]
    fn filetime_combines_both_halves() {
        let ft = FILETIME {
            dwLowDateTime: 5,
            dwHighDateTime: 1,
        };
        assert_eq!(filetime_100ns(ft), (1u64 << 32) + 5);
    }

    #[test]
    fn the_sampler_reads_real_values_for_this_process() {
        let mut sampler = ProcessSampler::new();
        // Burn a little CPU so the delta is non-zero.
        let mut x = 0u64;
        let start = Instant::now();
        while start.elapsed().as_millis() < 60 {
            x = x.wrapping_add(std::hint::black_box(1));
        }
        std::hint::black_box(x);
        let usage = sampler.sample();
        assert!(usage.memory_mb > 1.0, "memory {}", usage.memory_mb);
        assert!(
            usage.cpu_core_percent > 0.0,
            "cpu {}",
            usage.cpu_core_percent
        );
        assert!(usage.cpu_total_percent <= usage.cpu_core_percent);
    }
}
