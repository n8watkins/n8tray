//! Per-process GPU use from the performance counters Task Manager reads:
//! `GPU Engine(*)\Utilization Percentage` and `GPU Process Memory(*)\Dedicated
//! Usage` and `Shared Usage`. Instance names start with `pid_<pid>_`.

use std::collections::HashMap;

use windows_sys::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW,
};

use super::wide;

const PDH_FMT_NOCAP100: u32 = 0x0000_8000;

pub struct Query {
    query: PDH_HQUERY,
    engine: PDH_HCOUNTER,
    dedicated: PDH_HCOUNTER,
    shared: PDH_HCOUNTER,
}

// PDH handles may be used from any one thread at a time.
unsafe impl Send for Query {}

impl Query {
    pub fn open() -> Option<Query> {
        let mut query: PDH_HQUERY = std::ptr::null_mut();
        if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } != 0 {
            return None;
        }
        let add = |path: &str| {
            let path = wide(path);
            let mut counter: PDH_HCOUNTER = std::ptr::null_mut();
            (unsafe { PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) } == 0)
                .then_some(counter)
        };
        let counters = (
            add("\\GPU Engine(*)\\Utilization Percentage"),
            add("\\GPU Process Memory(*)\\Dedicated Usage"),
            add("\\GPU Process Memory(*)\\Shared Usage"),
        );
        let (Some(engine), Some(dedicated), Some(shared)) = counters else {
            unsafe { PdhCloseQuery(query) };
            return None;
        };
        // Utilization is a rate: the first collection only primes it.
        unsafe { PdhCollectQueryData(query) };
        Some(Query {
            query,
            engine,
            dedicated,
            shared,
        })
    }

    /// (memory in bytes, utilization percent) per pid with any GPU use.
    pub fn sample(&mut self) -> HashMap<u32, (f64, f64)> {
        let mut result: HashMap<u32, (f64, f64)> = HashMap::new();
        if unsafe { PdhCollectQueryData(self.query) } != 0 {
            return result;
        }
        for counter in [self.dedicated, self.shared] {
            for (name, value) in values(counter) {
                if let Some(pid) = pid_of(&name) {
                    result.entry(pid).or_default().0 += value;
                }
            }
        }
        // Task Manager's figure: the busiest engine type, summed over its engines.
        let mut per_type: HashMap<(u32, String), f64> = HashMap::new();
        for (name, value) in values(self.engine) {
            if let Some(pid) = pid_of(&name) {
                let kind = name.rsplit("engtype_").next().unwrap_or("").to_string();
                *per_type.entry((pid, kind)).or_default() += value;
            }
        }
        for ((pid, _), value) in per_type {
            let entry = result.entry(pid).or_default();
            entry.1 = entry.1.max(value.min(100.0));
        }
        result
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.query) };
    }
}

fn pid_of(instance: &str) -> Option<u32> {
    instance
        .strip_prefix("pid_")?
        .split('_')
        .next()?
        .parse()
        .ok()
}

fn values(counter: PDH_HCOUNTER) -> Vec<(String, f64)> {
    let format = PDH_FMT_DOUBLE | PDH_FMT_NOCAP100;
    let mut size = 0u32;
    let mut count = 0u32;
    let status = unsafe {
        PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, std::ptr::null_mut())
    };
    if status != PDH_MORE_DATA || size == 0 {
        return Vec::new();
    }
    // u64 elements keep the item array aligned.
    let mut buffer: Vec<u64> = vec![0; (size as usize).div_ceil(8)];
    let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    if unsafe { PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, items) } != 0 {
        return Vec::new();
    }
    let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
    items
        .iter()
        .filter(|item| item.FmtValue.CStatus == 0 && !item.szName.is_null())
        .map(|item| {
            let mut len = 0;
            while unsafe { *item.szName.add(len) } != 0 {
                len += 1;
            }
            let name =
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(item.szName, len) });
            (name, unsafe { item.FmtValue.Anonymous.doubleValue })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn instance_names() {
        assert_eq!(
            super::pid_of("pid_1234_luid_0x0_0xD1C2_phys_0_eng_0_engtype_3D"),
            Some(1234)
        );
        assert_eq!(super::pid_of("_Total"), None);
    }
}
