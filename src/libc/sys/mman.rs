/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use crate::abi::DotDotDot;
use crate::dyld::FunctionExports;
use crate::environment::Environment;
use crate::export_c_func;
use crate::libc::errno::{set_errno, EINVAL, ENOTSUP};
use crate::libc::posix_io;
use crate::libc::posix_io::{off_t, FileDescriptor, SEEK_SET};
use crate::mem::{ConstPtr, GuestUSize, MutVoidPtr, Ptr, PAGE_SIZE_ALIGN_MASK};
use std::collections::HashMap;

#[allow(dead_code)]
const MAP_FILE: i32 = 0x0000;
const MAP_FIXED: i32 = 0x0010;
const MAP_ANON: i32 = 0x1000;

#[derive(Default)]
pub struct State {
    /// Page-aligned ranges currently owned by `mmap`.
    mmap_allocations: HashMap<MutVoidPtr, GuestUSize>,
}

impl State {
    fn untrack_range(&mut self, addr: MutVoidPtr, len: GuestUSize) {
        let remove_start = u64::from(addr.to_bits());
        let remove_end = remove_start + u64::from(len);
        let allocations: Vec<_> = self
            .mmap_allocations
            .iter()
            .map(|(&base, &size)| (base, size))
            .collect();

        for (base, size) in allocations {
            let allocation_start = u64::from(base.to_bits());
            let allocation_end = allocation_start + u64::from(size);
            if allocation_start >= remove_end || remove_start >= allocation_end {
                continue;
            }

            self.mmap_allocations.remove(&base).unwrap();
            if allocation_start < remove_start {
                self.mmap_allocations.insert(
                    base,
                    GuestUSize::try_from(remove_start - allocation_start).unwrap(),
                );
            }
            if remove_end < allocation_end {
                self.mmap_allocations.insert(
                    Ptr::from_bits(GuestUSize::try_from(remove_end).unwrap()),
                    GuestUSize::try_from(allocation_end - remove_end).unwrap(),
                );
            }
        }
    }
}

fn page_aligned_len(len: GuestUSize) -> Option<GuestUSize> {
    len.checked_add(PAGE_SIZE_ALIGN_MASK)
        .map(|len| len & !PAGE_SIZE_ALIGN_MASK)
        .filter(|&len| len != 0)
}

/// For files, our implementation of mmap is really simple:
/// it's just load entirety of file in memory!
fn mmap(
    env: &mut Environment,
    addr: MutVoidPtr,
    len: GuestUSize,
    prot: i32,
    flags: i32,
    fd: FileDescriptor,
    offset: off_t,
) -> MutVoidPtr {
    // TODO: handle errno properly
    set_errno(env, 0);

    log_dbg!(
        "mmap({:?}, {}, {}, {}, {}, {})",
        addr,
        len,
        prot,
        flags,
        fd,
        offset
    );

    let Some(allocation_len) = page_aligned_len(len) else {
        set_errno(env, EINVAL);
        return Ptr::from_bits(GuestUSize::MAX);
    };
    if flags & MAP_FIXED != 0 && addr.to_bits() & PAGE_SIZE_ALIGN_MASK != 0 {
        set_errno(env, EINVAL);
        return Ptr::from_bits(GuestUSize::MAX);
    }

    assert_eq!(offset, 0);
    let ptr = if addr.is_null() {
        env.mem.vm_alloc(None, allocation_len).unwrap()
    } else if flags & MAP_FIXED != 0 {
        env.libc_state.mman.untrack_range(addr, allocation_len);
        env.mem.vm_free(addr, allocation_len);
        env.mem
            .vm_alloc(Some(addr.to_bits()), allocation_len)
            .unwrap()
    } else {
        match env.mem.vm_alloc(Some(addr.to_bits()), allocation_len) {
            Err(err) => {
                let ptr = env.mem.vm_alloc(None, allocation_len).unwrap();
                log!(
                    "Warning: mmap could not allocate at hint {addr:?} ({err:?}), allocated at {ptr:?}",
                );
                ptr
            }
            Ok(ptr) => ptr,
        }
    };

    assert!(ptr.to_bits() & PAGE_SIZE_ALIGN_MASK == 0);

    if (flags & MAP_ANON) != 0 {
        assert_eq!(fd, -1);
    } else {
        let new_offset = posix_io::lseek(env, fd, offset, SEEK_SET);
        assert_eq!(new_offset, offset);

        let read = posix_io::read(env, fd, ptr, len);
        assert_eq!(read as u32, len);
    };

    assert!(!env.libc_state.mman.mmap_allocations.contains_key(&ptr));
    env.libc_state
        .mman
        .mmap_allocations
        .insert(ptr, allocation_len);

    ptr
}

fn munmap(env: &mut Environment, addr: MutVoidPtr, len: GuestUSize) -> i32 {
    // TODO: handle errno properly
    set_errno(env, 0);

    log_dbg!("munmap({:?}, {})", addr, len);

    let Some(allocation_len) = page_aligned_len(len) else {
        set_errno(env, EINVAL);
        log!("Warning: munmap({:?}, {}) failed, returning -1", addr, len);
        return -1;
    };
    if addr.to_bits() & PAGE_SIZE_ALIGN_MASK != 0 {
        set_errno(env, EINVAL);
        return -1;
    }

    env.libc_state.mman.untrack_range(addr, allocation_len);
    env.mem.vm_free(addr, allocation_len);
    0 // success
}

fn madvise(env: &mut Environment, addr: MutVoidPtr, len: GuestUSize, advice: i32) -> i32 {
    log!("TODO: madvise({:?}, {}, {}) -> -1", addr, len, advice);
    set_errno(env, ENOTSUP);
    -1
}

fn shm_open(env: &mut Environment, name: ConstPtr<u8>, oflag: i32, _dots: DotDotDot) -> i32 {
    log!(
        "TODO: shm_open({:?} '{:?}', {}, ...) -> -1",
        name,
        env.mem.cstr_at_utf8(name),
        oflag
    );
    set_errno(env, EINVAL);
    -1
}

fn mprotect(_env: &mut Environment, addr: MutVoidPtr, len: GuestUSize, prot: i32) -> i32 {
    log_dbg!("mprotect({:?}, {}, {}) -> 0", addr, len, prot);
    0
}

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(mmap(_, _, _, _, _, _)),
    export_c_func!(munmap(_, _)),
    export_c_func!(madvise(_, _, _)),
    export_c_func!(shm_open(_, _, _)),
    export_c_func!(mprotect(_, _, _)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrack_range_splits_an_existing_mapping() {
        let mut state = State::default();
        state
            .mmap_allocations
            .insert(Ptr::from_bits(0x1000), 0x5000);

        state.untrack_range(Ptr::from_bits(0x2000), 0x2000);

        assert_eq!(state.mmap_allocations.len(), 2);
        assert_eq!(state.mmap_allocations[&Ptr::from_bits(0x1000)], 0x1000);
        assert_eq!(state.mmap_allocations[&Ptr::from_bits(0x4000)], 0x2000);
    }

    #[test]
    fn untrack_range_handles_multiple_mappings() {
        let mut state = State::default();
        state
            .mmap_allocations
            .insert(Ptr::from_bits(0x1000), 0x2000);
        state
            .mmap_allocations
            .insert(Ptr::from_bits(0x4000), 0x3000);

        state.untrack_range(Ptr::from_bits(0x2000), 0x4000);

        assert_eq!(state.mmap_allocations.len(), 2);
        assert_eq!(state.mmap_allocations[&Ptr::from_bits(0x1000)], 0x1000);
        assert_eq!(state.mmap_allocations[&Ptr::from_bits(0x6000)], 0x1000);
    }
}
