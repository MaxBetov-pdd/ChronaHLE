/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Cross-platform memory management wrappers using the host's system calls.

/// Cross-platform memory allocation using the host's system calls.
/// Returns an address aligned to the guest's 4KB page boundaries.
///
/// - The function returns a raw pointer to allocated memory.
///   The caller is responsible for managing that memory.
/// - The returned pointer must be freed using the corresponding
///   [`free_memory`] call.
#[cfg(windows)]
pub(super) unsafe fn allocate_memory(size: usize) -> std::io::Result<*mut core::ffi::c_void> {
    use windows_sys::Win32::System::Memory::{
        VirtualAlloc, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE,
    };

    let ptr = unsafe {
        VirtualAlloc(
            std::ptr::null(),
            size,
            MEM_RESERVE | MEM_COMMIT,
            PAGE_READWRITE,
        )
    };

    if ptr.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    Ok(ptr)
}

#[cfg(windows)]
pub(super) unsafe fn protect_no_access(
    address: *mut core::ffi::c_void,
    size: usize,
) -> std::io::Result<()> {
    use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_NOACCESS};

    let mut old_protection = 0;
    let res = unsafe { VirtualProtect(address, size, PAGE_NOACCESS, &mut old_protection) };
    if res == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Discard committed guest pages while preserving their virtual addresses.
/// Re-accessing the range yields zero-filled pages, matching a fresh VM allocation.
#[cfg(windows)]
pub(super) unsafe fn discard_pages(
    guest_address: u32,
    guest_memory: *mut [u8; 1 << 32],
    size: u32,
) -> std::io::Result<()> {
    use windows_sys::Win32::System::Memory::{
        VirtualAlloc, VirtualFree, MEM_COMMIT, MEM_DECOMMIT, PAGE_READWRITE,
    };

    let address = unsafe { (*guest_memory).as_mut_ptr().add(guest_address as usize) }.cast();
    if unsafe { VirtualFree(address, size as usize, MEM_DECOMMIT) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let committed = unsafe { VirtualAlloc(address, size as usize, MEM_COMMIT, PAGE_READWRITE) };
    if committed != address {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
pub(super) unsafe fn allocate_memory(size: usize) -> std::io::Result<*mut core::ffi::c_void> {
    use libc::{mmap, sysconf, MAP_ANONYMOUS, MAP_PRIVATE, PROT_READ, PROT_WRITE};

    const PAGE_SIZE: usize = crate::mem::PAGE_SIZE as usize;
    let host_page_size = unsafe { sysconf(libc::_SC_PAGESIZE) as usize };

    assert!(
        host_page_size >= PAGE_SIZE,
        "Hosts with smaller than 4KiB pages are not supported."
    );

    let ptr = unsafe {
        mmap(
            std::ptr::null_mut(),
            size,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        )
    };

    if ptr == libc::MAP_FAILED {
        return Err(std::io::Error::last_os_error());
    }
    Ok(ptr)
}

#[cfg(unix)]
pub(super) unsafe fn protect_no_access(
    address: *mut core::ffi::c_void,
    size: usize,
) -> std::io::Result<()> {
    let res = unsafe { libc::mprotect(address, size, libc::PROT_NONE) };
    if res == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
pub(super) unsafe fn discard_pages(
    guest_address: u32,
    guest_memory: *mut [u8; 1 << 32],
    size: u32,
) -> std::io::Result<()> {
    use std::sync::OnceLock;

    static HOST_PAGE_SIZE: OnceLock<usize> = OnceLock::new();
    let page_size = *HOST_PAGE_SIZE.get_or_init(|| {
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        usize::try_from(page_size).expect("Invalid host page size")
    });

    let address = unsafe { guest_memory.cast::<u8>().add(guest_address as usize) };
    let start = address as usize;
    let end = start.checked_add(size as usize).unwrap();
    let aligned_start = start.next_multiple_of(page_size).min(end);
    let aligned_end = (end / page_size) * page_size;

    unsafe { address.write_bytes(0, aligned_start - start) };
    if aligned_start < aligned_end {
        let res = unsafe {
            libc::madvise(
                aligned_start as *mut core::ffi::c_void,
                aligned_end - aligned_start,
                libc::MADV_DONTNEED,
            )
        };
        if res == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }
    let suffix_start = aligned_end.max(start);
    unsafe { (suffix_start as *mut u8).write_bytes(0, end - suffix_start) };
    Ok(())
}

/// Cross-platform memory free using the host's system calls.
///
/// # Safety
/// - The address and size should match parameters and result of the
///   [`allocate_memory`] call.
#[cfg(windows)]
pub(super) unsafe fn free_memory(
    address: *mut core::ffi::c_void,
    _size: usize,
) -> std::io::Result<()> {
    use windows_sys::Win32::System::Memory::{VirtualFree, MEM_RELEASE};

    let res = unsafe { VirtualFree(address, 0, MEM_RELEASE) };

    if res == 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}

#[cfg(unix)]
pub(super) unsafe fn free_memory(
    address: *mut core::ffi::c_void,
    size: usize,
) -> std::io::Result<()> {
    use libc::munmap;

    let res = unsafe { munmap(address, size) };

    if res == -1 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}
