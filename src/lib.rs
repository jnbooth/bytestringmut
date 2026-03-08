//! A pool for creating byte-slices and strings that can be cheaply cloned and shared across threads
//! without allocating memory. Byte-slices are shared as [`Bytes`], and strings are shared as
//! [`ByteString`]s.
//!
//! Internally, a `BytesPool` is a wrapper around a [`ByteStringMut`] buffer from the [`bytes`] crate.
//! It shares data by appending the data to its buffer and then splitting the buffer off with
//! [`ByteStringMut::split`]. This only allocates memory if the buffer needs to resize.

#![no_std]

extern crate alloc;

use core::borrow::{Borrow, BorrowMut};
use core::ops::{Deref, DerefMut};
use core::str::Utf8Error;
use core::{cmp, fmt};

use alloc::string::String;

pub use bytes::Bytes;
pub use bytestring::ByteString;

use bytes::BytesMut;

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteStringMut {
    inner: BytesMut,
}

impl ByteStringMut {
    /// Creates a new `ByteStringMut` with the specified capacity.
    ///
    /// The returned `ByteStringMut` will be able to hold at least `capacity` bytes
    /// without reallocating.
    ///
    /// It is important to note that this function does not specify the length
    /// of the returned `ByteStringMut`, but only the capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut bs = ByteStringMut::with_capacity(64);
    ///
    /// // `bs` contains no data, even though there is capacity
    /// assert_eq!(bs.len(), 0);
    ///
    /// bs.push_str("hello world");
    ///
    /// assert_eq!(bs, "hello world");
    /// ```
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        // SAFETY: An empty array is valid UTF8.
        unsafe { Self::from_utf8_unchecked(BytesMut::with_capacity(capacity)) }
    }

    /// Creates a new `ByteStringMut` with default capacity.
    ///
    /// Resulting object has length 0 and unspecified capacity.
    /// This function does not allocate.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut bs = ByteStringMut::new();
    ///
    /// assert_eq!(0, bs.len());
    ///
    /// bs.reserve(2);
    /// bs.push_str("xy");
    ///
    /// assert_eq!(bs, "xy");
    /// ```
    #[inline]
    pub fn new() -> Self {
        // SAFETY: An empty array is valid UTF8.
        unsafe { Self::from_utf8_unchecked(BytesMut::new()) }
    }

    /// Creates a new `ByteString` from a `BytesMut`.
    ///
    /// # Safety
    /// This function is unsafe because it does not check the bytes passed to it are valid UTF-8.
    /// If this constraint is violated, it may cause memory unsafety issues with future users of
    /// the `ByteStringMut`, as we assume that `ByteStringMut`s are valid UTF-8. However, the most
    /// likely issue is that the data gets corrupted.
    #[inline]
    pub unsafe fn from_utf8_unchecked(src: BytesMut) -> Self {
        Self { inner: src }
    }

    /// Returns the number of bytes contained in this `ByteStringMut`.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let bs = ByteStringMut::from("hello");
    /// assert_eq!(bs.len(), 5);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the `ByteStringMut` has a length of 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let bs = ByteStringMut::with_capacity(64);
    /// assert!(bs.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the number of bytes the `ByteStringMut` can hold without reallocating.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let bs = ByteStringMut::with_capacity(64);
    /// assert_eq!(bs.capacity(), 64);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Converts `self` into an immutable `ByteString`.
    ///
    /// The conversion is zero cost and is used to indicate that the slice
    /// referenced by the handle will no longer be mutated. Once the conversion
    /// is done, the handle can be cloned and shared across threads.
    ///
    /// # Examples
    ///
    /// ```ignore-wasm
    /// use bytestringmut::ByteStringMut;
    /// use std::thread;
    ///
    /// let mut bs = ByteStringMut::with_capacity(64);
    /// bs.push_str("hello world");
    /// let bs1 = bs.freeze();
    /// let bs2 = bs1.clone();
    ///
    /// let th = thread::spawn(move || {
    ///     assert_eq!(bs1, "hello world");
    /// });
    ///
    /// assert_eq!(bs2, "hello world");
    /// th.join().unwrap();
    /// ```
    #[inline]
    pub fn freeze(self) -> ByteString {
        let bytes = self.inner.freeze();
        // SAFETY: `bytes` contains only valid UTF-8.
        unsafe { ByteString::from_bytes_unchecked(bytes) }
    }

    /// Splits the bytestring into two at the given index.
    ///
    /// Afterwards `self` contains elements `[0, at)`, and the returned
    /// `ByteStringMut` contains elements `[at, capacity)`. It's guaranteed that the
    /// memory does not move, that is, the address of `self` does not change,
    /// and the address of the returned slice is `at` bytes after that.
    ///
    /// This is an `O(1)` operation that just increases the reference count
    /// and sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut a = ByteStringMut::from("hello WORLD");
    /// let mut b = a.split_off(6);
    ///
    /// a[0..1].make_ascii_uppercase();
    /// b[0..1].make_ascii_lowercase();
    ///
    /// assert_eq!(a, "Hello ");
    /// assert_eq!(b, "wORLD");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `at > capacity` or `at` does not lie on a `char` UTF-8 boundary.
    #[inline]
    #[must_use = "consider ByteStringMut::truncate if you don't need the other half"]
    pub fn split_off(&mut self, at: usize) -> Self {
        self.assert_char_boundary(at);
        // SAFETY: `self.assert_character_boundary` ensures `at` is a valid character boundary.
        unsafe { Self::from_utf8_unchecked(self.inner.split_off(at)) }
    }

    /// Removes the bytes from the current view, returning them in a new
    /// `ByteStringMut` handle.
    ///
    /// Afterwards, `self` will be empty, but will retain any additional
    /// capacity that it had before the operation. This is identical to
    /// `self.split_to(self.len())`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and
    /// sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut buf = ByteStringMut::with_capacity(1024);
    /// buf.push_str("hello world");
    ///
    /// let other = buf.split();
    ///
    /// assert!(buf.is_empty());
    /// assert_eq!(1013, buf.capacity());
    ///
    /// assert_eq!(other, "hello world");
    /// ```
    #[inline]
    #[must_use = "consider ByteStringMut::clear if you don't need the other half"]
    pub fn split(&mut self) -> Self {
        // SAFETY: `self.inner` is valid UTF-8, so `self.inner.split()` must be as well.
        unsafe { Self::from_utf8_unchecked(self.inner.split()) }
    }

    /// Splits the buffer into two at the given index.
    ///
    /// Afterwards `self` contains elements `[at, len)`, and the returned `ByteStringMut`
    /// contains elements `[0, at)`.
    ///
    /// This is an `O(1)` operation that just increases the reference count and
    /// sets a few indices.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut a = ByteStringMut::from("hello WORLD");
    /// let mut b = a.split_to(6);
    ///
    /// a[0..1].make_ascii_lowercase();
    /// b[0..1].make_ascii_uppercase();
    ///
    /// assert_eq!(a, "wORLD");
    /// assert_eq!(b, "Hello ");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `at > len` or `at` does not lie on a `char` UTF-8 boundary.
    #[inline]
    #[must_use = "consider ByteStringMut::advance if you don't need the other half"]
    pub fn split_to(&mut self, at: usize) -> Self {
        self.assert_char_boundary(at);
        // SAFETY: `self.assert_character_boundary` ensures `at` is a valid character boundary.
        unsafe { Self::from_utf8_unchecked(self.inner.split_to(at)) }
    }

    /// Shortens the buffer, keeping the first `len` bytes and dropping the
    /// rest.
    ///
    /// If `len` is greater than the buffer's current length, this has no
    /// effect.
    ///
    /// Existing underlying capacity is preserved.
    ///
    /// The [`split_off`](`Self::split_off()`) method can emulate `truncate`, but this causes the
    /// excess bytes to be returned instead of dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut buf = ByteStringMut::from("hello world");
    /// buf.truncate(5);
    /// assert_eq!(buf, "hello");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if `len` does not lie on a `char` UTF-8 boundary.
    #[inline]
    pub fn truncate(&mut self, len: usize) {
        if len <= self.len() {
            self.assert_char_boundary(len);
            // SAFETY: `self.assert_character_boundary` ensures `at` is a valid character boundary.
            unsafe { self.inner.set_len(len) };
        }
    }

    /// Clears the buffer, removing all data. Existing capacity is preserved.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut buf = ByteStringMut::from("hello world");
    /// buf.clear();
    /// assert!(buf.is_empty());
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Reserves capacity for at least `additional` more bytes to be inserted
    /// into the given `ByteStringMut`.
    ///
    /// More than `additional` bytes may be reserved in order to avoid frequent
    /// reallocations. A call to `reserve` may result in an allocation.
    ///
    /// Before allocating new buffer space, the function will attempt to reclaim
    /// space in the existing buffer. If the current handle references a view
    /// into a larger original buffer, and all other handles referencing part
    /// of the same original buffer have been dropped, then the current view
    /// can be copied/shifted to the front of the buffer and the handle can take
    /// ownership of the full buffer, provided that the full buffer is large
    /// enough to fit the requested additional capacity.
    ///
    /// This optimization will only happen if shifting the data from the current
    /// view to the front of the buffer is not too expensive in terms of the
    /// (amortized) time required. The precise condition is subject to change;
    /// as of now, the length of the data being shifted needs to be at least as
    /// large as the distance that it's shifted by. If the current view is empty
    /// and the original buffer is large enough to fit the requested additional
    /// capacity, then reallocations will never happen.
    ///
    /// # Examples
    ///
    /// In the following example, a new buffer is allocated.
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut buf = ByteStringMut::from("hello");
    /// buf.reserve(64);
    /// assert!(buf.capacity() >= 69);
    /// ```
    ///
    /// In the following example, the existing buffer is reclaimed.
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut buf = ByteStringMut::with_capacity(128);
    /// buf.push_str(&" ".repeat(64));
    ///
    /// let ptr = buf.as_ptr();
    /// let other = buf.split();
    ///
    /// assert!(buf.is_empty());
    /// assert_eq!(buf.capacity(), 64);
    ///
    /// drop(other);
    /// buf.reserve(128);
    ///
    /// assert_eq!(buf.capacity(), 128);
    /// assert_eq!(buf.as_ptr(), ptr);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the new capacity overflows `usize`.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }

    /// Attempts to cheaply reclaim already allocated capacity for at least `additional` more
    /// bytes to be inserted into the given `ByteStringMut` and returns `true` if it succeeded.
    ///
    /// `try_reclaim` behaves exactly like `reserve`, except that it never allocates new storage
    /// and returns a `bool` indicating whether it was successful in doing so:
    ///
    /// `try_reclaim` returns false under these conditions:
    ///  - The spare capacity left is less than `additional` bytes AND
    ///  - The existing allocation cannot be reclaimed cheaply or it was less than
    ///    `additional` bytes in size
    ///
    /// Reclaiming the allocation cheaply is possible if the `ByteStringMut` has no outstanding
    /// references through other `ByteStringMut`s or `Bytes` which point to the same underlying
    /// storage.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut buf = ByteStringMut::with_capacity(64);
    /// assert_eq!(true, buf.try_reclaim(64));
    /// assert_eq!(64, buf.capacity());
    ///
    /// buf.push_str("abcd");
    /// let mut split = buf.split();
    /// assert_eq!(60, buf.capacity());
    /// assert_eq!(4, split.capacity());
    /// assert_eq!(false, split.try_reclaim(64));
    /// assert_eq!(false, buf.try_reclaim(64));
    /// // The split buffer is filled with "abcd"
    /// assert_eq!(false, split.try_reclaim(4));
    /// // buf is empty and has capacity for 60 bytes
    /// assert_eq!(true, buf.try_reclaim(60));
    ///
    /// drop(buf);
    /// assert_eq!(false, split.try_reclaim(64));
    ///
    /// split.clear();
    /// assert_eq!(4, split.capacity());
    /// assert_eq!(true, split.try_reclaim(64));
    /// assert_eq!(64, split.capacity());
    /// ```
    #[inline]
    #[must_use = "consider ByteStringMut::reserve if you need an infallible reservation"]
    pub fn try_reclaim(&mut self, additional: usize) -> bool {
        self.inner.try_reclaim(additional)
    }

    /// Appends given bytes to this `ByteStringMut`.
    ///
    /// If this `ByteStringMut` object does not have enough capacity, it is resized
    /// first.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut buf = ByteStringMut::with_capacity(0);
    /// buf.push_str("aaabbb");
    /// buf.push_str("cccddd");
    ///
    /// assert_eq!(buf, "aaabbbcccddd");
    /// ```
    #[inline]
    pub fn push_str(&mut self, s: &str) {
        self.inner.extend_from_slice(s.as_bytes());
    }

    /// Appends a single character to this `ByteStringMut`.
    ///
    /// If this `ByteStringMut` object does not have enough capacity, it is resized
    /// first.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut buf = ByteStringMut::with_capacity(0);
    /// buf.push('a');
    /// buf.push('b');
    ///
    /// assert_eq!(buf, "ab");
    /// ```
    #[inline]
    pub fn push(&mut self, ch: char) {
        let len = self.len();
        let ch_len = ch.len_utf8();
        self.inner.reserve(ch_len);
        // SAFETY: Will be initialized in the next step.
        unsafe { self.inner.set_len(len + ch_len) };
        ch.encode_utf8(&mut self.inner[len..]);
    }

    /// Absorbs a `ByteStringMut` that was previously split off.
    ///
    /// If the two `ByteStringMut` objects were previously contiguous and not mutated
    /// in a way that causes re-allocation i.e., if `other` was created by
    /// calling `split_off` on this `ByteStringMut`, then this is an `O(1)` operation
    /// that just decreases a reference count and sets a few indices.
    /// Otherwise this method degenerates to `self.push_str(other.as_ref())`.
    ///
    /// # Examples
    ///
    /// ```
    /// use bytestringmut::ByteStringMut;
    ///
    /// let mut buf = ByteStringMut::with_capacity(64);
    /// buf.push_str("aaabbbcccddd");
    ///
    /// let split = buf.split_off(6);
    /// assert_eq!(buf, "aaabbb");
    /// assert_eq!(split, "cccddd");
    ///
    /// buf.unsplit(split);
    /// assert_eq!(buf, "aaabbbcccddd");
    /// ```
    #[inline]
    pub fn unsplit(&mut self, other: Self) {
        self.inner.unsplit(other.inner);
    }

    #[inline]
    fn as_str(&self) -> &str {
        // SAFETY: `self.inner` contains only valid UTF-8.
        unsafe { str::from_utf8_unchecked(&self.inner) }
    }

    #[inline]
    fn as_str_mut(&mut self) -> &mut str {
        // SAFETY: `self.inner` contains only valid UTF-8.
        unsafe { str::from_utf8_unchecked_mut(&mut self.inner) }
    }

    /// # Panics
    ///
    /// Panics if `at` does not lie on a `char` UTF-8 boundary.
    #[track_caller]
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn assert_char_boundary(&self, at: usize) {
        let _ = self.as_str().split_at(at); // trigger panic
    }
}

impl Deref for ByteStringMut {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl DerefMut for ByteStringMut {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_str_mut()
    }
}

impl AsRef<str> for ByteStringMut {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsMut<str> for ByteStringMut {
    #[inline]
    fn as_mut(&mut self) -> &mut str {
        self.as_str_mut()
    }
}

impl AsRef<[u8]> for ByteStringMut {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.inner.as_ref()
    }
}

impl From<&str> for ByteStringMut {
    fn from(src: &str) -> Self {
        // SAFETY: `src` is valid UTF-8.
        unsafe { Self::from_utf8_unchecked(src.as_bytes().into()) }
    }
}

impl TryFrom<&[u8]> for ByteStringMut {
    type Error = Utf8Error;

    fn try_from(src: &[u8]) -> Result<Self, Self::Error> {
        let src = str::from_utf8(src)?.as_bytes();
        // SAFETY: `src` is valid UTF-8.
        Ok(unsafe { Self::from_utf8_unchecked(src.into()) })
    }
}

impl From<ByteStringMut> for ByteString {
    fn from(src: ByteStringMut) -> Self {
        src.freeze()
    }
}

impl From<ByteStringMut> for String {
    fn from(src: ByteStringMut) -> Self {
        // SAFETY: `src.inner` contains only valid UTF-8.
        unsafe { String::from_utf8_unchecked(src.inner.into()) }
    }
}

impl Borrow<str> for ByteStringMut {
    fn borrow(&self) -> &str {
        self.as_ref()
    }
}

impl BorrowMut<str> for ByteStringMut {
    fn borrow_mut(&mut self) -> &mut str {
        self.as_mut()
    }
}

impl fmt::Debug for ByteStringMut {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl fmt::Display for ByteStringMut {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl fmt::Write for ByteStringMut {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.inner.write_str(s)
    }

    #[inline]
    fn write_fmt(&mut self, args: fmt::Arguments) -> fmt::Result {
        self.inner.write_fmt(args)
    }
}

impl Extend<char> for ByteStringMut {
    fn extend<T: IntoIterator<Item = char>>(&mut self, iter: T) {
        let iterator = iter.into_iter();
        let (lower_bound, _) = iterator.size_hint();
        self.reserve(lower_bound);
        iterator.for_each(move |c| self.push(c));
    }
}

impl<'a> Extend<&'a char> for ByteStringMut {
    fn extend<T: IntoIterator<Item = &'a char>>(&mut self, iter: T) {
        self.extend(iter.into_iter().copied());
    }
}

impl<'a> Extend<&'a str> for ByteStringMut {
    fn extend<T: IntoIterator<Item = &'a str>>(&mut self, iter: T) {
        iter.into_iter().for_each(move |s| self.push_str(s));
    }
}

impl FromIterator<char> for ByteStringMut {
    fn from_iter<T: IntoIterator<Item = char>>(iter: T) -> Self {
        let mut buf = Self::new();
        buf.extend(iter);
        buf
    }
}

impl<'a> FromIterator<&'a char> for ByteStringMut {
    fn from_iter<T: IntoIterator<Item = &'a char>>(iter: T) -> Self {
        iter.into_iter().copied().collect()
    }
}

impl<'a> FromIterator<&'a str> for ByteStringMut {
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        let mut buf = Self::new();
        buf.extend(iter);
        buf
    }
}

macro_rules! impl_eq_ord {
    ($t:ty) => {
        impl PartialEq<$t> for ByteStringMut {
            fn eq(&self, other: &$t) -> bool {
                self.inner == *other
            }
        }

        impl PartialOrd<$t> for ByteStringMut {
            fn partial_cmp(&self, other: &$t) -> Option<cmp::Ordering> {
                self.inner.partial_cmp(other)
            }
        }

        impl PartialEq<ByteStringMut> for $t {
            fn eq(&self, other: &ByteStringMut) -> bool {
                *self == other.inner
            }
        }

        impl PartialOrd<ByteStringMut> for $t {
            fn partial_cmp(&self, other: &ByteStringMut) -> Option<cmp::Ordering> {
                self.partial_cmp(&other.inner)
            }
        }
    };
}

impl_eq_ord!(str);
impl_eq_ord!(String);
impl_eq_ord!(BytesMut);

impl PartialEq<Bytes> for ByteStringMut {
    fn eq(&self, other: &Bytes) -> bool {
        self.inner == *other
    }
}

impl PartialEq<ByteStringMut> for Bytes {
    fn eq(&self, other: &ByteStringMut) -> bool {
        *self == other.inner
    }
}

impl<'a, T: ?Sized> PartialEq<&'a T> for ByteStringMut
where
    ByteStringMut: PartialEq<T>,
{
    fn eq(&self, other: &&'a T) -> bool {
        *self == **other
    }
}

impl<'a, T: ?Sized> PartialOrd<&'a T> for ByteStringMut
where
    ByteStringMut: PartialOrd<T>,
{
    fn partial_cmp(&self, other: &&'a T) -> Option<cmp::Ordering> {
        self.partial_cmp(*other)
    }
}
