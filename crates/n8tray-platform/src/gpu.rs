//! Per-process GPU memory and use, where the OS has counters for it
//! (Windows; macOS and Linux have no general per-process source).

use std::collections::HashMap;

/// One process's GPU use.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuUse {
    /// Dedicated plus shared GPU memory, in MiB.
    pub mem_mb: f64,
    /// The busiest engine type's utilization, 0 to 100.
    pub percent: f64,
}

/// A GPU sampler; `None` where the OS has no counters.
pub struct GpuSampler {
    #[cfg(windows)]
    query: crate::win::gpu::Query,
}

impl GpuSampler {
    pub fn new() -> Option<GpuSampler> {
        #[cfg(windows)]
        {
            crate::win::gpu::Query::open().map(|query| GpuSampler { query })
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// GPU use of every process that has any. Utilization is averaged since
    /// the previous call.
    pub fn sample(&mut self) -> HashMap<u32, GpuUse> {
        #[cfg(windows)]
        {
            self.query
                .sample()
                .into_iter()
                .map(|(pid, (bytes, percent))| {
                    (
                        pid,
                        GpuUse {
                            mem_mb: bytes / (1024.0 * 1024.0),
                            percent,
                        },
                    )
                })
                .collect()
        }
        #[cfg(not(windows))]
        {
            HashMap::new()
        }
    }
}
