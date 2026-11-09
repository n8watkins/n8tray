//! Every process in one call: NtQuerySystemInformation(SystemProcessInformation)
//! returns pid, parent, creation time, CPU times, working set and private
//! bytes for all processes at once, which is how Task Manager reads them.

use std::io;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use windows_sys::Wdk::System::SystemInformation::{
    NtQuerySystemInformation, SystemProcessInformation,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

use super::Handle;
use crate::process::ProcInfo;

const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

/// SYSTEM_PROCESS_INFORMATION as the kernel lays it out (the documented
/// struct hides most fields as reserved).
#[repr(C)]
struct SystemProcessInfo {
    next_entry_offset: u32,
    number_of_threads: u32,
    working_set_private_size: i64,
    hard_fault_count: u32,
    number_of_threads_high_watermark: u32,
    cycle_time: u64,
    create_time: i64,
    user_time: i64,
    kernel_time: i64,
    image_name: UnicodeString,
    base_priority: i32,
    unique_process_id: usize,
    inherited_from_unique_process_id: usize,
    handle_count: u32,
    session_id: u32,
    unique_process_key: usize,
    peak_virtual_size: usize,
    virtual_size: usize,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_page_count: usize,
}

/// A FILETIME (100 ns since 1601) as a UTC time.
pub fn filetime_to_utc(filetime: i64) -> DateTime<Utc> {
    const UNIX_EPOCH_AS_FILETIME: i64 = 116_444_736_000_000_000;
    let since_unix = filetime - UNIX_EPOCH_AS_FILETIME;
    if since_unix <= 0 {
        return DateTime::<Utc>::UNIX_EPOCH;
    }
    DateTime::from_timestamp(
        since_unix / 10_000_000,
        ((since_unix % 10_000_000) * 100) as u32,
    )
    .unwrap_or_default()
}

pub fn snapshot() -> io::Result<Vec<ProcInfo>> {
    // u64 elements keep the buffer 8-byte aligned, as the entries need.
    let mut buffer: Vec<u64> = vec![0; 128 * 1024];
    loop {
        let bytes = (buffer.len() * 8) as u32;
        let mut needed = 0u32;
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buffer.as_mut_ptr().cast(),
                bytes,
                &mut needed,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            let wanted = (needed as usize).max(bytes as usize * 2) + 64 * 1024;
            buffer = vec![0; wanted.div_ceil(8)];
            continue;
        }
        if status < 0 {
            return Err(io::Error::other(format!(
                "NtQuerySystemInformation failed: 0x{:08X}",
                status as u32
            )));
        }
        break;
    }

    let base = buffer.as_ptr().cast::<u8>();
    let total = buffer.len() * 8;
    let mut offset = 0usize;
    let mut processes = Vec::with_capacity(512);
    loop {
        if offset + std::mem::size_of::<SystemProcessInfo>() > total {
            break;
        }
        let entry = unsafe { &*base.add(offset).cast::<SystemProcessInfo>() };
        let name = if entry.image_name.buffer.is_null() || entry.image_name.length == 0 {
            if entry.unique_process_id == 0 {
                "System Idle Process".to_string()
            } else {
                "System".to_string()
            }
        } else {
            let units = unsafe {
                std::slice::from_raw_parts(
                    entry.image_name.buffer,
                    entry.image_name.length as usize / 2,
                )
            };
            String::from_utf16_lossy(units)
        };
        processes.push(ProcInfo {
            pid: entry.unique_process_id as u32,
            parent_pid: entry.inherited_from_unique_process_id as u32,
            started_at: filetime_to_utc(entry.create_time),
            name,
            cpu_time_ms: ((entry.user_time + entry.kernel_time).max(0) / 10_000) as u64,
            rss_bytes: entry.working_set_size as u64,
            private_bytes: Some(entry.pagefile_usage as u64),
        });
        if entry.next_entry_offset == 0 {
            break;
        }
        offset += entry.next_entry_offset as usize;
    }
    Ok(processes)
}

pub fn open_query(pid: u32) -> Option<Handle> {
    Handle::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) })
}

pub fn exe_path(pid: u32) -> Option<PathBuf> {
    let process = open_query(pid)?;
    let mut buffer = vec![0u16; 32 * 1024];
    let mut length = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            process.raw(),
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut length,
        )
    };
    (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filetimes() {
        assert_eq!(filetime_to_utc(0), DateTime::<Utc>::UNIX_EPOCH);
        let t = filetime_to_utc(116_444_736_000_000_000 + 12_345_678);
        assert_eq!(t.timestamp(), 1);
        assert_eq!(t.timestamp_subsec_nanos(), 234_567_800);
    }

    #[test]
    fn layout_matches_the_kernel() {
        assert_eq!(
            std::mem::offset_of!(SystemProcessInfo, unique_process_id),
            0x50
        );
        assert_eq!(
            std::mem::offset_of!(SystemProcessInfo, working_set_size),
            0x90
        );
        assert_eq!(
            std::mem::offset_of!(SystemProcessInfo, pagefile_usage),
            0xB8
        );
    }
}
