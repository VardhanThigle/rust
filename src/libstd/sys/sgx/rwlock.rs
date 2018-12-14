// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use num::NonZeroUsize;

use super::waitqueue::{WaitVariable, WaitQueue, SpinMutex, SpinMutexGuard, NotifiedTcs, try_lock_or_false};
use mem;

pub struct RWLock {
    readers: SpinMutex<WaitVariable<Option<NonZeroUsize>>>,
    writer: SpinMutex<WaitVariable<bool>>,
}


// Below is to check. at compile time, that RWLock has size of 128 bytes.
// This is an assumption in libwundind for rust-sgx target.
const SIZE_OF_RWLock: usize = 128;
#[allow(unreachable_code)]
unsafe fn _rw_lock_size_assert() {
    mem::transmute::<RWLock, [u8; SIZE_OF_RWLock]>(panic!());
}

//unsafe impl Send for RWLock {}
//unsafe impl Sync for RWLock {} // FIXME

impl RWLock {
    pub const fn new() -> RWLock {
        RWLock {
            readers: SpinMutex::new(WaitVariable::new(None)),
            writer: SpinMutex::new(WaitVariable::new(false))
        }
    }

    #[inline]
    pub unsafe fn read(&self) {
        let mut rguard = self.readers.lock();
        let wguard = self.writer.lock();
        if *wguard.lock_var() || !wguard.queue_empty() {
            // Another thread has or is waiting for the write lock, wait
            drop(wguard);
            WaitQueue::wait(rguard);
            // Another thread has passed the lock to us
        } else {
            // No waiting writers, acquire the read lock
            *rguard.lock_var_mut() =
                NonZeroUsize::new(rguard.lock_var().map_or(0, |n| n.get()) + 1);
        }
    }

    #[inline]
    pub unsafe fn try_read(&self) -> bool {
        let mut rguard = try_lock_or_false!(self.readers);
        let wguard = try_lock_or_false!(self.writer);
        if *wguard.lock_var() || !wguard.queue_empty() {
            // Another thread has or is waiting for the write lock
            false
        } else {
            // No waiting writers, acquire the read lock
            *rguard.lock_var_mut() =
                NonZeroUsize::new(rguard.lock_var().map_or(0, |n| n.get()) + 1);
            true
        }
    }

    #[inline]
    pub unsafe fn write(&self) {
        let rguard = self.readers.lock();
        let mut wguard = self.writer.lock();
        if *wguard.lock_var() || rguard.lock_var().is_some() {
            // Another thread has the lock, wait
            drop(rguard);
            WaitQueue::wait(wguard);
            // Another thread has passed the lock to us
        } else {
            // We are just now obtaining the lock
            *wguard.lock_var_mut() = true;
        }
    }

    #[inline]
    pub unsafe fn try_write(&self) -> bool {
        let rguard = try_lock_or_false!(self.readers);
        let mut wguard = try_lock_or_false!(self.writer);
        if *wguard.lock_var() || rguard.lock_var().is_some() {
            // Another thread has the lock
            false
        } else {
            // We are just now obtaining the lock
            *wguard.lock_var_mut() = true;
            true
        }
    }

    #[inline]
    unsafe fn __read_unlock(&self,
                            mut rguard: SpinMutexGuard<WaitVariable<Option<NonZeroUsize>>>,
                            mut wguard: SpinMutexGuard<WaitVariable<bool>>) {
        *rguard.lock_var_mut() = NonZeroUsize::new(rguard.lock_var().unwrap().get() - 1);
        if rguard.lock_var().is_some() {
            // There are other active readers
        } else {
            if let Ok(mut wguard) = WaitQueue::notify_one(wguard) {
                // A writer was waiting, pass the lock
                *wguard.lock_var_mut() = true;
            } else {
                // No writers were waiting, the lock is released
                assert!(rguard.queue_empty());
            }
        }
    }

    #[inline]
    pub unsafe fn read_unlock(&self) {
        let mut rguard = self.readers.lock();
        let wguard = self.writer.lock();
        self.__read_unlock(rguard, wguard);
    }

    #[inline]
    unsafe fn __write_unlock(&self,
                            mut rguard: SpinMutexGuard<WaitVariable<Option<NonZeroUsize>>>,
                            mut wguard: SpinMutexGuard<WaitVariable<bool>>) {
        if let Err(mut wguard) = WaitQueue::notify_one(wguard) {
            // No writers waiting, release the write lock
            *wguard.lock_var_mut() = false;
            if let Ok(mut rguard) = WaitQueue::notify_all(rguard) {
                // One or more readers were waiting, pass the lock to them
                if let NotifiedTcs::All { count } = rguard.notified_tcs() {
                    *rguard.lock_var_mut() = Some(count)
                } else {
                    unreachable!() // called notify_all
                }
            } else {
                // No readers waiting, the lock is released
            }
        } else {
            // There was a thread waiting for write, just pass the lock
        }
    }


    #[inline]
    pub unsafe fn write_unlock(&self) {
        let rguard = self.readers.lock();
        let wguard = self.writer.lock();
        self.__write_unlock(rguard, wguard);
    }

    #[inline]
    pub unsafe fn unlock(&self) {
        let rguard = self.readers.lock();
        let wguard = self.writer.lock();
        if *wguard.lock_var() == true {
            self.__write_unlock(rguard, wguard);
        } else {
            self.__read_unlock(rguard, wguard);
        }
    }

    #[inline]
    pub unsafe fn destroy(&self) {}
}

const EINVAL:i32 = 22;


// TODO-[unwind support] move these to another file maybe? Link issue due to different CUGs.

#[no_mangle]
pub unsafe extern "C" fn __rust_rwlock_rdlock(p : *mut RWLock) -> i32 {
    if p.is_null() {
        return EINVAL;
    }
    (*p).read();
    return 0;
}



#[no_mangle]
pub unsafe extern "C"  fn __rust_rwlock_wrlock(p : *mut RWLock) -> i32 {
    (*p).write();
    return 0;
}
#[no_mangle]
pub unsafe extern "C"  fn __rust_rwlock_unlock(p : *mut RWLock) -> i32 {
    if p.is_null() {
        return EINVAL;
    }
    (*p).unlock();
    return 0;
}

#[no_mangle]
pub unsafe extern "C"  fn __rust_print_msg(m : *mut u8, s : i32) -> i32 {
    let mut i : i32 = 0;
    for i in 0..s {
        let c = *m.offset(i as isize) as char;
        if c == '\0' {
            break;
        }
        print!("{}", c)
    }
    return i;
}

#[no_mangle]
pub unsafe extern "C"  fn __rust_abort() {
    unsafe { ::sys::abort_internal() };
}

#[no_mangle]
pub unsafe extern "C" fn __rust_encl_address(offset : u64) -> *mut u8 {
    unsafe { ::sys::sgx::abi::mem::rel_ptr_mut::<u8>(offset)}

}
