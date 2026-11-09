//! Free and total physical memory, the figures the pressure level is computed from.

/// Physical memory now, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemMemory {
    pub total_bytes: u64,
    /// Free memory as the OS counts it: available physical memory on
    /// Windows (what Task Manager calls Available), MemAvailable on Linux.
    pub available_bytes: u64,
}

pub fn system_memory() -> Option<SystemMemory> {
    #[cfg(windows)]
    {
        crate::win::system::memory().map(|(total_bytes, available_bytes)| SystemMemory {
            total_bytes,
            available_bytes,
        })
    }
    #[cfg(not(windows))]
    {
        let mut system = sysinfo::System::new().ok()?;
        system.refresh_memory();
        (system.total_memory() > 0).then(|| SystemMemory {
            total_bytes: system.total_memory(),
            available_bytes: system.available_memory(),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn memory_is_known() {
        let memory = super::system_memory().expect("the OS reports memory");
        assert!(memory.total_bytes > 0);
        assert!(memory.available_bytes <= memory.total_bytes);
    }
}
