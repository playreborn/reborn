//! The callback queue behind `Steam_BGetCallback` and
//! `Steam_FreeLastCallback`: one queue per handle the game polls with,
//! filled later by `rebornd` (ADR 0047: "a reader thread in the shim puts
//! it into a local queue"). Nothing fills it yet: no callback content has
//! a confirmed source in the shim, so only the tests enqueue.
//!
//! Provenance (`docs/shim/meanings.md`, "Callback delivery …", loop
//! iteration 9b, result of experiment 19, and `docs/shim/entry-points.md`,
//! steps 4b and 4d):
//!
//! - Confirmed: the 24-byte header the game gets at `rdx` holds the
//!   callback number in bytes 4..8, the address of its data in bytes
//!   8..16 and the size of that data in bytes 16..20; a poll that hands a
//!   callback over returns 1, and the game frees it with exactly one
//!   `Steam_FreeLastCallback` on the same `rcx` (and thread) before it
//!   polls again.
//! - Recorded, used as recorded: a free with nothing held. Linsips
//!   (`observe-55108`) freed once more than it got headers, after a poll
//!   that returned 0, and that free returned 0 (`meanings.md`, loop
//!   iteration 9b, Linsips note). The shim then changes nothing and
//!   returns 0; experiment 19's "no free without a header" (D) held for
//!   its five games, not for every game.
//! - Candidate, used as recorded: bytes 0..4 were 1 in every recorded
//!   header, also for polls with `rcx` 2. Whether that is the user handle,
//!   a constant 1 or the user on that pipe is not settled; the shim writes
//!   its user handle, which is 1 (`crate::objects::USER`), so the bytes
//!   are what every recording shows.
//! - Candidate, used as recorded: bytes 20..24 were 0 in every header
//!   (padding or the upper half of the size; sizes stay small).
//! - Candidate: whether `rcx` is the pipe or the user handle. The queue
//!   is keyed by the value as the game passes it, which works for either
//!   reading.
//!
//! The held slot is per handle (`rcx`), not per thread. Experiment 19
//! recorded the rule per `rcx` and thread (one free on the same `rcx` and
//! thread before that thread's next poll), and in the recordings each
//! handle was polled from one thread. Two cases follow from keying by
//! handle only and were never recorded: a poll from another thread on the
//! same handle while a callback is held ends the process (`StillHeld`),
//! and a free from another thread on the same handle is accepted.
//!
//! The data is kept in 16-byte aligned memory. The alignment the official
//! client uses is not recorded; 16 is chosen so that any field a game
//! reads is at least as aligned as in a naturally laid out struct.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, MutexGuard};

/// Size of the header the game gets (`meanings.md`, experiment 19, B).
pub(crate) const HEADER_SIZE: usize = 24;

/// 16 bytes, 16-byte aligned: the unit the data is stored in.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct Block([u8; 16]);

/// One callback: its number and its data.
struct Callback {
    number: i32,
    /// The data, padded with zeros to whole blocks. A `Vec`, not a `Box`:
    /// moving a `Box` asserts unique access to its memory, which would
    /// invalidate the address already handed to the game; moving a `Vec`
    /// does not touch its buffer.
    blocks: Vec<Block>,
    /// The size of the data without padding, as the header gives it.
    size: u32,
}

impl Callback {
    fn address(&self) -> *const u8 {
        self.blocks.as_ptr().cast()
    }

    /// The header for this callback (`meanings.md`, experiment 19).
    fn header(&self) -> [u8; HEADER_SIZE] {
        let mut header = [0u8; HEADER_SIZE];
        // Bytes 0..4: 1 in every recording (candidate, see module docs).
        header[0..4].copy_from_slice(&crate::objects::USER.to_le_bytes());
        // Bytes 4..8: the callback number (confirmed, B, C, E).
        header[4..8].copy_from_slice(&self.number.to_le_bytes());
        // Bytes 8..16: the address of the data (confirmed, B).
        header[8..16].copy_from_slice(&(self.address() as usize as u64).to_le_bytes());
        // Bytes 16..20: the size of the data (confirmed, B, C).
        header[16..20].copy_from_slice(&self.size.to_le_bytes());
        // Bytes 20..24 stay 0, as in every recording (candidate).
        header
    }
}

/// What one handle has: callbacks waiting, and the one handed over and
/// not freed yet (its data must stay where the header points).
#[derive(Default)]
struct Pipe {
    waiting: VecDeque<Callback>,
    held: Option<Callback>,
}

static PIPES: Mutex<BTreeMap<i32, Pipe>> = Mutex::new(BTreeMap::new());

fn pipes() -> MutexGuard<'static, BTreeMap<i32, Pipe>> {
    PIPES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Why a callback was not queued.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum Unsupported {
    /// No data: every recorded callback had 1 to 784 bytes; an empty one
    /// was never seen.
    Empty,
    /// More data than the 4-byte size field holds.
    TooLarge,
}

/// The size field for `len` bytes of data, if the shim supports it.
#[cfg_attr(not(test), allow(dead_code))]
fn size_for(len: usize) -> Result<u32, Unsupported> {
    if len == 0 {
        return Err(Unsupported::Empty);
    }
    u32::try_from(len).map_err(|_| Unsupported::TooLarge)
}

/// Queues a callback for the game polling with `pipe`. Callbacks of one
/// handle are handed over in the order they were queued.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn enqueue(pipe: i32, number: i32, data: &[u8]) -> Result<(), Unsupported> {
    let size = size_for(data.len())?;
    let blocks = data
        .chunks(16)
        .map(|chunk| {
            let mut block = [0u8; 16];
            block[..chunk.len()].copy_from_slice(chunk);
            Block(block)
        })
        .collect();
    pipes()
        .entry(pipe)
        .or_default()
        .waiting
        .push_back(Callback {
            number,
            blocks,
            size,
        });
    Ok(())
}

/// The result of a poll.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Fetch {
    /// Nothing waiting.
    Empty,
    /// The next callback, now held until it is freed; the header to give
    /// the game.
    Delivered([u8; HEADER_SIZE]),
    /// The last callback was not freed yet. Never recorded (experiment 19,
    /// D: one free per header before the next poll), so it has no answer.
    StillHeld,
}

/// Hands over the next callback for `pipe`, if any.
pub(crate) fn fetch(pipe: i32) -> Fetch {
    let mut pipes = pipes();
    let Some(state) = pipes.get_mut(&pipe) else {
        return Fetch::Empty;
    };
    if state.held.is_some() {
        return Fetch::StillHeld;
    }
    match state.waiting.pop_front() {
        Some(callback) => {
            let header = callback.header();
            state.held = Some(callback);
            Fetch::Delivered(header)
        }
        None => Fetch::Empty,
    }
}

/// The result of freeing.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Free {
    /// The held callback's data is released.
    Freed,
    /// Nothing was held; nothing changes. Recorded in Linsips
    /// (`observe-55108`: 10 headers, 11 frees, the extra one after a poll
    /// that returned 0; `meanings.md`, loop iteration 9b, Linsips note).
    NothingHeld,
}

/// Releases the callback last handed over for `pipe`.
pub(crate) fn free_last(pipe: i32) -> Free {
    match pipes().get_mut(&pipe).and_then(|state| state.held.take()) {
        Some(_) => Free::Freed,
        None => Free::NothingHeld,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The queue is global and tests run in parallel: each test uses its
    // own handles.

    fn number_of(header: &[u8; HEADER_SIZE]) -> i32 {
        i32::from_le_bytes([header[4], header[5], header[6], header[7]])
    }

    fn address_of(header: &[u8; HEADER_SIZE]) -> *const u8 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&header[8..16]);
        u64::from_le_bytes(bytes) as usize as *const u8
    }

    fn size_of(header: &[u8; HEADER_SIZE]) -> u32 {
        u32::from_le_bytes([header[16], header[17], header[18], header[19]])
    }

    fn delivered(pipe: i32) -> [u8; HEADER_SIZE] {
        match fetch(pipe) {
            Fetch::Delivered(header) => header,
            other => panic!("expected a callback on {pipe}, got {other:?}"),
        }
    }

    #[test]
    fn empty_queue_gives_nothing() {
        assert_eq!(fetch(100), Fetch::Empty, "never used");
        enqueue(101, 1101, &[1]).unwrap();
        delivered(101);
        assert_eq!(free_last(101), Free::Freed);
        assert_eq!(fetch(101), Fetch::Empty, "used and drained");
    }

    #[test]
    fn header_bytes_follow_experiment_19() {
        let data: Vec<u8> = (0..24).collect();
        enqueue(110, 0x44d, &data).unwrap();
        let header = delivered(110);
        assert_eq!(header[0..4], [1, 0, 0, 0], "bytes 0..4 as recorded");
        assert_eq!(header[4..8], [0x4d, 0x04, 0, 0], "number, little endian");
        assert_eq!(size_of(&header), 24);
        assert_eq!(header[20..24], [0, 0, 0, 0], "bytes 20..24 zero");
        let address = address_of(&header);
        assert!(!address.is_null());
        assert_eq!(address as usize % 16, 0, "16-byte aligned");
        // SAFETY: the callback is held, so its 24 bytes live at `address`.
        let seen = unsafe { std::slice::from_raw_parts(address, 24) };
        assert_eq!(seen, &data[..]);
        assert_eq!(free_last(110), Free::Freed);
    }

    #[test]
    fn callbacks_come_in_queue_order() {
        for number in [304, 336, 703] {
            enqueue(120, number, &[number as u8; 3]).unwrap();
        }
        for number in [304, 336, 703] {
            assert_eq!(number_of(&delivered(120)), number);
            assert_eq!(free_last(120), Free::Freed);
        }
        assert_eq!(fetch(120), Fetch::Empty);
    }

    #[test]
    fn free_releases_only_the_held_callback() {
        assert_eq!(free_last(130), Free::NothingHeld, "nothing fetched");
        enqueue(130, 1, &[1]).unwrap();
        enqueue(130, 2, &[2]).unwrap();
        assert_eq!(free_last(130), Free::NothingHeld, "queued is not held");
        assert_eq!(number_of(&delivered(130)), 1);
        assert_eq!(fetch(130), Fetch::StillHeld, "no second poll before free");
        assert_eq!(free_last(130), Free::Freed);
        assert_eq!(free_last(130), Free::NothingHeld, "freed once only");
        assert_eq!(number_of(&delivered(130)), 2, "the second one is kept");
        assert_eq!(free_last(130), Free::Freed);
    }

    #[test]
    fn two_handles_are_independent() {
        enqueue(140, 1080000, &[7; 628]).unwrap();
        enqueue(141, 1102, &[8; 16]).unwrap();
        let first = delivered(140);
        // The other handle can be polled while 140 holds a callback.
        let second = delivered(141);
        assert_eq!((number_of(&first), size_of(&first)), (1080000, 628));
        assert_eq!((number_of(&second), size_of(&second)), (1102, 16));
        assert_eq!(free_last(141), Free::Freed);
        assert_eq!(fetch(141), Fetch::Empty);
        assert_eq!(fetch(140), Fetch::StillHeld, "141's free left 140 alone");
        assert_eq!(free_last(140), Free::Freed);
    }

    #[test]
    fn data_stays_valid_until_freed() {
        let data: Vec<u8> = (0..=255).cycle().take(784).collect();
        enqueue(150, 1040044, &data).unwrap();
        let header = delivered(150);
        // Other activity in between must not move or drop the data.
        for number in 0..64 {
            enqueue(150, number, &[0xaa; 100]).unwrap();
            enqueue(151, number, &[0xbb; 100]).unwrap();
        }
        delivered(151);
        free_last(151);
        // SAFETY: the callback on 150 is still held, so its 784 bytes
        // live at the header's address.
        let seen = unsafe { std::slice::from_raw_parts(address_of(&header), 784) };
        assert_eq!(seen, &data[..]);
        assert_eq!(free_last(150), Free::Freed);
    }

    #[test]
    fn unsupported_data_is_refused() {
        assert_eq!(enqueue(160, 1, &[]), Err(Unsupported::Empty));
        assert_eq!(fetch(160), Fetch::Empty);
        assert_eq!(size_for(784), Ok(784));
        assert_eq!(size_for(u32::MAX as usize), Ok(u32::MAX));
        // More than the 4-byte size field holds (checked without
        // allocating that much).
        if let Some(len) = (u32::MAX as usize).checked_add(1) {
            assert_eq!(size_for(len), Err(Unsupported::TooLarge));
        }
    }
}
