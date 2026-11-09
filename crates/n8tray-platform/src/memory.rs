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

/// Hands this process's resident pages it is not using back to the OS
/// (Windows: EmptyWorkingSet). A resident tray calls it when it goes idle,
/// such as after its window was destroyed; pages it needs again come back on
/// use. Returns whether anything was done (false where the OS has no such call).
pub fn trim_own_working_set() -> bool {
    #[cfg(windows)]
    {
        crate::win::system::trim_own_working_set()
    }
    #[cfg(not(windows))]
    {
        false
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

    #[test]
    fn trimming_is_safe() {
        assert_eq!(super::trim_own_working_set(), cfg!(windows));
    }
}
