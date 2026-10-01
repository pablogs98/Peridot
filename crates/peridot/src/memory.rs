//! Helpers for reading guest linear memory from a context.
//!
//! # Zero copy, and where it stops
//!
//! [`wiggle::GuestMemory`] is either `Unshared(&mut [u8])` or `Shared(&[UnsafeCell<u8>])`. The
//! borrowing accessors work only on the first: handing out a `&[u8]` into memory another thread
//! may be mutating would be unsound, so `as_slice` returns `Ok(None)` for shared memories and
//! `as_cow` falls back to copying. The helpers here return [`Cow`] for exactly that reason —
//! borrowed from guest memory in the common case, owned when the guest uses shared memory.
//!
//! A second limit is not about the API at all: a context that hands the bytes to deferred work
//! (a spawned upload, a buffer flushed later) has to own them, because the borrow ends when the
//! hostcall returns. Only a context that consumes the payload before returning can stay
//! zero-copy. Call [`Cow::into_owned`] deliberately in the former case.

use std::borrow::Cow;
use wasmtime_wasi::p1::types::CiovecArray;
use wiggle::{GuestError, GuestMemory, GuestPtr};

/// The first non-empty buffer of a vectored write, as a guest pointer.
///
/// Servicing only the first buffer is legal: a short write is a valid `write(2)` result and the
/// guest will re-issue the remainder.
pub fn first_non_empty_ciovec(
    memory: &GuestMemory<'_>,
    ciovs: CiovecArray,
) -> Result<GuestPtr<[u8]>, GuestError> {
    for iov in ciovs.iter() {
        let iov = memory.read(iov?)?;
        if iov.buf_len == 0 {
            continue;
        }
        return Ok(iov.buf.as_array(iov.buf_len));
    }
    Ok(GuestPtr::new((0, 0)))
}

/// The payload of a vectored write: borrowed from guest memory, or copied if the guest memory
/// is shared.
pub fn payload<'a>(
    memory: &'a GuestMemory<'_>,
    ciovs: CiovecArray,
) -> Result<Cow<'a, [u8]>, GuestError> {
    let ptr = first_non_empty_ciovec(memory, ciovs)?;
    memory.as_cow(ptr)
}

/// A path argument: borrowed from guest memory, or copied if the guest memory is shared.
pub fn read_path<'a>(
    memory: &'a GuestMemory<'_>,
    ptr: GuestPtr<str>,
) -> Result<Cow<'a, str>, GuestError> {
    memory.as_cow_str(ptr)
}
