//! A set-like data structure that is the same size as a `HashSet<T>` but has faster best-case
//! membership checks, because `x86_64` only supports 48 bits of address space. So we can embed a
//! bloom filter in the 32 free bits between its capacity and length.

#![warn(clippy::pedantic, clippy::nursery, clippy::restriction)]
#![deny(clippy::missing_inline_in_public_items)]

use std::hash::{Hash, Hasher};
use std::mem::ManuallyDrop;
use std::ptr::NonNull;
use std::slice;

pub struct BloomSet<T> {
    ptr: NonNull<T>,
    length: usize,
    capacity: usize,
}

struct BloomHasher {
    state: u8,
}

impl Hasher for BloomHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.state ^= b;
        }
    }

    #[inline]
    fn finish(&self) -> u64 {
        u64::from(self.state ^ (self.state >> 4))
    }
}

impl<T> Default for BloomSet<T> {
    #[inline]
    fn default() -> Self {
        Self {
            ptr: NonNull::dangling(),
            length: 0,
            capacity: 0,
        }
    }
}

impl<T> BloomSet<T> {
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        let mut vec = ManuallyDrop::new(Vec::with_capacity(cap));
        let ptr = unsafe { NonNull::new_unchecked(vec.as_mut_ptr()) };
        Self {
            ptr,
            length: 0,
            capacity: cap,
        }
    }

    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() != 0
    }

    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.length & 0x0000_FFFF_FFFF_FFFF
    }

    #[inline]
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity & 0x0000_FFFF_FFFF_FFFF
    }

    #[inline]
    #[must_use]
    const fn bloom_contains(&self, bloom: u32) -> bool {
        if bloom > u16::MAX as u32 {
            let bits = self.length.to_le_bytes();
            let bloom_chunk = u16::from_le_bytes([bits[6], bits[7]]);
            (bloom_chunk & (bloom >> 16) as u16) > 0
        } else {
            let bits = self.capacity.to_le_bytes();
            let bloom_chunk = u16::from_le_bytes([bits[6], bits[7]]);
            (bloom_chunk & bloom as u16) > 0
        }
    }

    #[inline]
    #[must_use]
    pub const fn as_mut_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.as_mut_ptr(), self.len()) }
    }

    #[inline(never)]
    fn insert_resizing(&mut self, item: T) {
        let mut vec = unsafe {
            // Use ManuallyDrop to ensure that the Vec is never dropped
            ManuallyDrop::new(Vec::from_raw_parts(
                self.as_mut_ptr(),
                self.len(),
                self.capacity(),
            ))
        };
        vec.push(item);
        unsafe { self.ptr = NonNull::new_unchecked(vec.as_mut_ptr()) };
        self.capacity =
            (vec.capacity() & 0x0000_FFFF_FFFF_FFFF) | (self.capacity & 0xFFFF_0000_0000_0000);
    }

    pub fn clear(&mut self) {
        let mut vec = unsafe {
            // Use ManuallyDrop to ensure that the Vec is never dropped
            ManuallyDrop::new(Vec::from_raw_parts(
                self.as_mut_ptr(),
                self.len(),
                self.capacity(),
            ))
        };
        vec.clear();
        self.capacity &= 0x0000_FFFF_FFFF_FFFF;
        self.length = 0;
    }
}

impl<T: Hash + PartialEq> BloomSet<T> {
    #[inline]
    pub fn insert(&mut self, item: T) {
        let mut hasher = BloomHasher { state: 0 };
        item.hash(&mut hasher);
        let hash = hasher.finish();
        let bloom_bit = 1 << (hash % 32);

        let maybe_in_set = if bloom_bit > u16::MAX as u32 {
            let mut bits = self.length.to_le_bytes();
            let bloom_chunk = u16::from_le_bytes([bits[6], bits[7]]);
            if (bloom_chunk & (bloom_bit >> 16) as u16) > 0 {
                true
            } else {
                let new_bloom_chunk = bloom_chunk | (bloom_bit >> 16) as u16;
                bits[6] = new_bloom_chunk.to_le_bytes()[0];
                bits[7] = new_bloom_chunk.to_le_bytes()[1];
                self.length = usize::from_le_bytes(bits);
                false
            }
        } else {
            let mut bits = self.capacity.to_le_bytes();
            let bloom_chunk = u16::from_le_bytes([bits[6], bits[7]]);
            if (bloom_chunk & bloom_bit as u16) > 0 {
                true
            } else {
                let new_bloom_chunk = bloom_chunk | bloom_bit as u16;
                bits[6] = new_bloom_chunk.to_le_bytes()[0];
                bits[7] = new_bloom_chunk.to_le_bytes()[1];
                self.capacity = usize::from_le_bytes(bits);
                false
            }
        };

        let in_set = if maybe_in_set {
            self.as_slice().iter().any(|it| *it == item)
        } else {
            false
        };
        if !in_set {
            if self.len() == self.capacity() {
                self.insert_resizing(item);
            } else {
                unsafe {
                    use std::convert::TryInto;
                    *self.ptr.as_ptr().offset(self.len().try_into().unwrap()) = item;
                }
            }
            self.length += 1;
        }

        /*
        let maybe_in_set = self.bloom_contains(bloom_bit);
        let in_set = if maybe_in_set {
            self.as_slice().iter().any(|it| *it == item)
        } else {
            false
        };
        if !in_set {
            self.insert_unique(item, bloom_bit);
        }
        */
    }

    #[inline]
    pub fn contains<B: std::borrow::Borrow<T>>(&self, item: B) -> bool {
        let item = item.borrow();
        let mut hasher = BloomHasher { state: 0 };
        item.hash(&mut hasher);
        let hash = hasher.finish();
        let bloom_bit = 1 << (hash % 32);

        let maybe_in_set = self.bloom_contains(bloom_bit);
        if maybe_in_set {
            self.as_slice().iter().any(|it| it == item)
        } else {
            false
        }
    }
}

impl<T> Drop for BloomSet<T> {
    #[inline]
    fn drop(&mut self) {
        unsafe { Vec::from_raw_parts(self.as_mut_ptr(), self.len(), self.capacity()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_right() {
        use core::mem::size_of;
        assert_eq!(size_of::<BloomSet<u8>>(), size_of::<Vec<u8>>());
    }

    #[test]
    fn insert() {
        let mut set = BloomSet::default();
        assert_eq!(set.len(), 0);
        set.insert(2u8);
        assert_eq!(set.len(), 1);
        set.insert(4u8);
        assert_eq!(set.len(), 2);
        assert_eq!(set.as_slice()[0], 2);
        assert_eq!(set.as_slice()[1], 4);
        assert_eq!(set.as_slice().len(), 2);

        set.insert(2);
        assert_eq!(set.len(), 2);

        set.insert(31);
        assert_eq!(set.len(), 3);
        set.insert(31);
        assert_eq!(set.len(), 3);
    }
}
