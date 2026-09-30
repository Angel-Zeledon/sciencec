//! `Map[K, V]`: a hash table that remembers the order it was filled in. §8's
//! method set in full, over the element-descriptor convention of the crate
//! documentation, §6.
//!
//! # The table
//!
//! **Two arrays, not one: the entries in insertion order, and an index over
//! them.** `collections-and-chains.md` §5.2's AMENDMENT 9 says `Map` and `Set`
//! iterate in insertion order and that *"it costs one index array"*, and this
//! is that array read literally — Python's compact dict, which is also the
//! behaviour the amendment cites as what its audience already believes.
//!
//! - **`entries`** holds each key and value as one `Entry[K, V]` record, in
//!   the order the keys were first inserted. Position `p` is live exactly
//!   when `live[p]` is non-zero; a removed entry leaves a hole rather than
//!   shifting everything after it, because a removal that moved `n` entries
//!   would make `remove` linear.
//! - **`slots`** is the open-addressing index: linear probing over a
//!   power-of-two capacity, one word per slot, each word a state in its low
//!   two bits ([`SCIENCE_MAP_SLOT_EMPTY`], [`SCIENCE_MAP_SLOT_OCCUPIED`],
//!   [`SCIENCE_MAP_SLOT_TOMBSTONE`] — the same three bytes the table has
//!   always had) and, when occupied, the entry's position above them.
//!
//! Iteration walks `entries` from position zero and skips holes, so its order
//! is a function of the calls the program made and **of nothing else** — not
//! of the hash, not of the capacity, not of how many rehashes happened on the
//! way. `stdlib-core.md` §3.5 draws the consequence the amendment did not:
//! since the order is not a function of the hash, `science-rt` may change its
//! hasher without changing any program's output. That is a property of this
//! layout, and an "iterate in slot order for speed" change would silently
//! break it; the invariant is pinned by the insertion-order tests in
//! `tests/map.rs` rather than by this sentence.
//!
//! **The rules the order follows**, decided here because the amendment does
//! not spell them out and a table has to pick:
//!
//! - A new key goes last.
//! - Re-inserting a key that is present replaces its value **in place**: the
//!   key keeps its position. `stdlib-core.md` §3.6 writes this into `insert`'s
//!   doc comment — *"re-inserting an existing key does not move it"* — so it
//!   is transcribed, not chosen.
//! - A removed key that is inserted again is a new key, and goes last.
//!   Python's dict does the same, and it is the only answer that does not
//!   require remembering keys that are no longer in the map.
//!
//! # The entry's layout is computed here, and that is the one new obligation
//!
//! The previous table kept keys and values in two parallel arrays precisely so
//! that the runtime never had to know the padding between a `K` and a `V`.
//! That argument does not survive AMENDMENT 9: `for entry in counts:` binds a
//! `&Entry[K, V]` (`collections-and-chains.md` §5.4: *"`Map` yields `Entry of
//! (K, V)`"*, and §4.3: `iterate()` yields a borrow *"uniformly"*), and a
//! borrow has to point at a record that exists. So the runtime lays the pair
//! out as the compiler lays out a two-field record — Decision 17's C layout,
//! the value at the key's size rounded up to the value's alignment, the stride
//! rounded up to the larger alignment — in [`entry_layout`], and
//! `science-codegen-llvm`'s `tests/maps.rs` checks it against the compiler's
//! own layout of `Entry[K, V]` for the key and value types a program can use.
//!
//! **Cost.** Two agreements where there was one. The descriptor already had to
//! agree with the compiler about each type's size and alignment; now the
//! formula that combines them must agree too. It is four lines of arithmetic
//! that §17 fixes, and it is checked rather than trusted.
//!
//! # Load factor, holes and rebuilds
//!
//! A table is rebuilt when one more entry would not fit in `entries`, whose
//! capacity is three quarters of the slot count — so the index stays below
//! three-quarters full, as it always has. Holes count against that capacity
//! exactly as tombstones used to, so a table that is inserted into and removed
//! from forever is rebuilt periodically instead of growing without bound, and
//! the rebuild **compacts**: live entries are moved down to positions
//! `0..len`, in the order they already had, and the index is rebuilt over
//! them. Compaction is a bitwise move, which is invisible in Science (§6.1
//! rule 2), and it preserves the order by construction because it walks the
//! entries in order.

use crate::abi::{ScienceMapInfo, ScienceTypeInfo};
use crate::mem::{capacity_overflow, dangling, science_alloc, science_dealloc};

/// Slot state: never used. The low two bits of a slot word.
pub const SCIENCE_MAP_SLOT_EMPTY: u8 = 0;
/// Slot state: indexes a live entry, whose position is the slot word shifted
/// right by [`SLOT_STATE_BITS`].
pub const SCIENCE_MAP_SLOT_OCCUPIED: u8 = 1;
/// Slot state: indexed an entry that was removed. Lookups probe past it;
/// insertions may claim it.
pub const SCIENCE_MAP_SLOT_TOMBSTONE: u8 = 2;

/// How many low bits of a slot word hold its state.
const SLOT_STATE_BITS: u32 = 2;
const SLOT_STATE_MASK: usize = (1 << SLOT_STATE_BITS) - 1;

/// Science's `Map[K, V]`.
///
/// Layout: six words, `{ slots, entries, live, len, used, cap }`.
///
/// - `slots` is `cap` words: the probe index. Each word's low two bits are a
///   `SCIENCE_MAP_SLOT_*` state; an occupied slot's remaining bits are the
///   position of its entry.
/// - `entries` holds [`entry_capacity`]`(cap)` records laid out by
///   [`entry_layout`], positions `0..used` of which have been written.
/// - `live` is one byte per entry position: non-zero while the entry at that
///   position belongs to the map, zero once it has been removed.
/// - `len` is the number of live entries, and is what §8's `len` returns.
/// - `used` is the number of entry positions written, live or removed. It is
///   what the next insertion's position will be, and the extent of an
///   iteration.
/// - `cap` is zero, or a power of two. A new `Map` allocates nothing.
///
/// None of the three pointers is ever null; all are dangling but aligned when
/// `cap == 0`.
///
/// As with `Array`, the key and value types appear nowhere in this struct: a
/// [`ScienceMapInfo`] describes them on every call, and **it must be the same
/// descriptor every time**.
///
/// Implements no `Drop`: moved by copying six words, destroyed by
/// [`science_map_free`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ScienceMap {
    /// `cap` slot words.
    pub slots: *mut usize,
    /// The entries, in insertion order.
    pub entries: *mut u8,
    /// One liveness byte per entry position.
    pub live: *mut u8,
    /// Live entries.
    pub len: usize,
    /// Entry positions written, live or removed.
    pub used: usize,
    /// Slot count: zero, or a power of two.
    pub cap: usize,
}

/// Where the value sits inside one `Entry[K, V]`, and how far apart two
/// entries are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryLayout {
    /// Byte offset of the value from the start of the entry. The key is at
    /// zero.
    pub value_offset: usize,
    /// Bytes from one entry to the next: the record's size.
    pub stride: usize,
    /// The record's alignment.
    pub align: usize,
}

/// The layout of `Entry[K, V]` for the key and value `info` describes:
/// Decision 17's C layout of a two-field record, in declaration order.
///
/// The key is at offset zero; the value at the key's size rounded up to the
/// value's alignment; the record's alignment is the larger of the two, and its
/// size is rounded up to it. A `Set`'s zero-sized value lands at the key's
/// size and adds nothing, so a set's entry is exactly its element — which is
/// what lets `for x in set:` hand out a pointer to the entry as a `&T`.
pub fn entry_layout(info: &ScienceMapInfo) -> EntryLayout {
    fn round_up(n: usize, align: usize) -> usize {
        // `align` is a power of two: the descriptor guarantees it.
        match n.checked_add(align - 1) {
            Some(n) => n & !(align - 1),
            None => capacity_overflow(),
        }
    }
    let align = info.key.align.max(info.value.align);
    let value_offset = round_up(info.key.size, info.value.align);
    let Some(end) = value_offset.checked_add(info.value.size) else { capacity_overflow() };
    EntryLayout { value_offset, stride: round_up(end, align), align }
}

/// How many entry positions a table of `cap` slots holds: three quarters of
/// them, which is the load factor.
///
/// `cap` is zero or a power of two no smaller than eight, so this is exact.
pub fn entry_capacity(cap: usize) -> usize {
    cap / 4 * 3
}

/// Scramble a caller-supplied hash so that the low bits, which select the slot,
/// depend on all 64.
///
/// This is the splitmix64 finaliser. It exists so that the quality of
/// `hash_fn` is a performance question and never a correctness one: a hash
/// that only varies in its high bits, or one that returns a constant, still
/// produces a correct table.
#[inline]
fn mix(hash: u64) -> u64 {
    let mut z = hash;
    z ^= z >> 30;
    z = z.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    z
}

/// The byte size of `count` entries, or a capacity overflow.
fn entries_bytes(layout: EntryLayout, count: usize) -> usize {
    count.checked_mul(layout.stride).unwrap_or_else(|| capacity_overflow())
}

impl ScienceMap {
    fn empty(info: &ScienceMapInfo) -> Self {
        ScienceMap {
            slots: dangling(std::mem::align_of::<usize>()).cast(),
            entries: dangling(entry_layout(info).align),
            live: dangling(1),
            len: 0,
            used: 0,
            cap: 0,
        }
    }

    /// # Safety
    ///
    /// `index` must be less than `cap`.
    #[inline]
    unsafe fn slot(&self, index: usize) -> usize {
        // SAFETY: `index < cap`, so the offset is inside the slot array.
        unsafe { *self.slots.add(index) }
    }

    /// The start of the entry at `position`.
    ///
    /// # Safety
    ///
    /// `position` must be less than `entry_capacity(cap)`, and `layout` must be
    /// this table's.
    #[inline]
    unsafe fn entry(&self, layout: EntryLayout, position: usize) -> *mut u8 {
        // SAFETY: the position is inside the entry array.
        unsafe { self.entries.add(position * layout.stride) }
    }

    /// # Safety
    ///
    /// As [`ScienceMap::entry`].
    #[inline]
    unsafe fn value_of(&self, layout: EntryLayout, position: usize) -> *mut u8 {
        // SAFETY: the value lies inside its entry.
        unsafe { self.entry(layout, position).add(layout.value_offset) }
    }

    /// # Safety
    ///
    /// `position` must be less than `used`.
    #[inline]
    unsafe fn is_live(&self, position: usize) -> bool {
        // SAFETY: `position < used <= entry_capacity(cap)`.
        unsafe { *self.live.add(position) != 0 }
    }

    /// The slot and entry position holding `key`, if any.
    ///
    /// # Safety
    ///
    /// `info` must describe this table, and `key` must point to a live key.
    unsafe fn find(&self, info: &ScienceMapInfo, key: *const u8) -> Option<(usize, usize)> {
        if self.cap == 0 {
            return None;
        }
        let layout = entry_layout(info);
        let mask = self.cap - 1;
        // SAFETY: the caller guarantees `key` is a live key for this table.
        let mut index = (mix(unsafe { (info.hash_fn)(key) }) as usize) & mask;
        loop {
            // SAFETY: `index` is masked into range.
            let word = unsafe { self.slot(index) };
            match (word & SLOT_STATE_MASK) as u8 {
                SCIENCE_MAP_SLOT_EMPTY => return None,
                SCIENCE_MAP_SLOT_OCCUPIED => {
                    let position = word >> SLOT_STATE_BITS;
                    // SAFETY: an occupied slot names a live entry, whose key
                    // is at its start; `key` is live; both are of the type
                    // `info.key` describes.
                    if unsafe { (info.eq_fn)(self.entry(layout, position), key) } {
                        return Some((index, position));
                    }
                }
                _ => {}
            }
            index = (index + 1) & mask;
            // The loop terminates because the table always keeps at least one
            // empty slot: occupied and tombstoned slots together never exceed
            // `used`, which `reserve_one` keeps within `entry_capacity(cap)`,
            // which is three quarters of `cap`.
        }
    }

    /// Point the first empty slot on `key`'s probe path at `position`. Used
    /// only while rebuilding, when the index has no tombstones and the key is
    /// known to be absent.
    ///
    /// # Safety
    ///
    /// `cap` must be non-zero, and the entry at `position` must hold a live
    /// key of the type `info` describes.
    unsafe fn index_fresh(&mut self, info: &ScienceMapInfo, layout: EntryLayout, position: usize) {
        let mask = self.cap - 1;
        // SAFETY: the caller guarantees a live key at `position`.
        let key = unsafe { self.entry(layout, position) };
        // SAFETY: as above.
        let mut index = (mix(unsafe { (info.hash_fn)(key) }) as usize) & mask;
        // SAFETY: `index` is masked into range.
        while (unsafe { self.slot(index) } & SLOT_STATE_MASK) as u8 != SCIENCE_MAP_SLOT_EMPTY {
            index = (index + 1) & mask;
        }
        // SAFETY: `index` is in range.
        unsafe {
            *self.slots.add(index) =
                (position << SLOT_STATE_BITS) | SCIENCE_MAP_SLOT_OCCUPIED as usize;
        }
    }

    /// Rebuild the table at `new_cap` slots: compact the live entries down to
    /// positions `0..len`, **in the order they already had**, and rebuild the
    /// index over them.
    ///
    /// Relocation is a bitwise move: no destructor and no clone runs, because
    /// moving an owned value is invisible in Science (§6.1 rule 2).
    ///
    /// # Safety
    ///
    /// `info` must describe this table, and `entry_capacity(new_cap)` must be
    /// at least `len + 1`.
    unsafe fn rehash(&mut self, info: &ScienceMapInfo, new_cap: usize) {
        let old = *self;
        let layout = entry_layout(info);
        let Some(slot_bytes) = new_cap.checked_mul(std::mem::size_of::<usize>()) else {
            capacity_overflow()
        };
        let new_entries = entry_capacity(new_cap);

        // SAFETY: the sizes are checked, and the alignments are a word's and
        // the entry's, which `entry_layout` takes from the descriptor.
        unsafe {
            self.slots = science_alloc(slot_bytes, std::mem::align_of::<usize>()).cast();
            std::ptr::write_bytes(self.slots, 0, new_cap);
            self.entries = science_alloc(entries_bytes(layout, new_entries), layout.align);
            self.live = science_alloc(new_entries, 1);
        }
        self.cap = new_cap;
        self.used = 0;

        for position in 0..old.used {
            // SAFETY: `position < old.used`.
            if !unsafe { old.is_live(position) } {
                continue;
            }
            let to = self.used;
            // SAFETY: the old entry is live and moves bitwise to the next
            // position of the new array, which `new_cap` was chosen to have
            // room for; the two arrays are distinct allocations.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    old.entry(layout, position),
                    self.entry(layout, to),
                    layout.stride,
                );
                *self.live.add(to) = 1;
                self.index_fresh(info, layout, to);
            }
            self.used += 1;
        }
        debug_assert_eq!(self.used, self.len);

        // SAFETY: the three old blocks came from `science_alloc` with exactly
        // these sizes and alignments; every live entry has been moved out, and
        // removed ones were already destroyed.
        unsafe { old.release_storage(layout) };
    }

    /// Return the three blocks to the allocator without touching what they
    /// hold.
    ///
    /// # Safety
    ///
    /// The blocks must be this table's own, and `layout` its entry layout.
    unsafe fn release_storage(&self, layout: EntryLayout) {
        let entries = entry_capacity(self.cap);
        // SAFETY: the caller's obligation; each size and alignment is the one
        // the block was allocated with, and a zero-slot table's dangling
        // pointers are ignored by `science_dealloc`.
        unsafe {
            science_dealloc(
                self.slots.cast(),
                self.cap * std::mem::size_of::<usize>(),
                std::mem::align_of::<usize>(),
            );
            science_dealloc(self.entries, entries_bytes(layout, entries), layout.align);
            science_dealloc(self.live, entries, 1);
        }
    }

    /// Guarantee room for one more entry, rebuilding if the table is too full.
    ///
    /// "Too full" counts positions, not live entries: a removed entry's hole
    /// occupies its position until the next rebuild, so a table that is
    /// repeatedly inserted into and removed from is rebuilt rather than
    /// growing. The rebuild sizes for the live entries alone, which is what
    /// reclaims the holes.
    ///
    /// # Safety
    ///
    /// `info` must describe this table.
    unsafe fn reserve_one(&mut self, info: &ScienceMapInfo) {
        if self.used < entry_capacity(self.cap) {
            return;
        }
        let wanted = self
            .len
            .saturating_add(1)
            .saturating_mul(4)
            .saturating_div(3)
            .saturating_add(1);
        let new_cap = wanted
            .checked_next_power_of_two()
            .unwrap_or_else(|| capacity_overflow())
            .max(8);
        debug_assert!(entry_capacity(new_cap) > self.len);
        // SAFETY: `new_cap` is a power of two whose entry capacity exceeds
        // `len`; `info` is the caller's obligation.
        unsafe { self.rehash(info, new_cap) }
    }
}

/// `Map::new()`.
///
/// Allocates nothing.
///
/// # Safety
///
/// `info` must be a non-null, aligned pointer to a valid [`ScienceMapInfo`].
#[no_mangle]
pub unsafe extern "C" fn science_map_new(info: *const ScienceMapInfo) -> ScienceMap {
    // SAFETY: the caller guarantees a valid descriptor.
    ScienceMap::empty(unsafe { &*info })
}

/// Run `drop_fn`, when there is one, over one key or value.
///
/// # Safety
///
/// `at` must point to a live value of the type `info` describes.
#[inline]
unsafe fn drop_in_place(info: &ScienceTypeInfo, at: *mut u8) {
    if let Some(drop_fn) = info.drop_fn {
        // SAFETY: the caller's obligation.
        unsafe { drop_fn(at) }
    }
}

/// Release a `Map`, running the key and value destructors over every entry it
/// still holds.
///
/// **Codegen support:** this is `Map`'s drop glue (§6.1 rule 6). Entries that
/// [`science_map_remove`] took out are not destroyed, since they are no longer
/// the map's: their key was destroyed and their value moved out when they were
/// removed, and their position is a hole.
///
/// The receiver is left as a valid empty map.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`ScienceMap`] that has not
/// already been freed, and `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn science_map_free(map: *mut ScienceMap, info: *const ScienceMapInfo) {
    // SAFETY: the caller guarantees a live map and its descriptor.
    let (map, info) = unsafe { (&mut *map, &*info) };
    let layout = entry_layout(info);

    if info.key.drop_fn.is_some() || info.value.drop_fn.is_some() {
        for position in 0..map.used {
            // SAFETY: `position < used`.
            if !unsafe { map.is_live(position) } {
                continue;
            }
            // SAFETY: a live entry holds a live key and value of the described
            // types.
            unsafe {
                drop_in_place(&info.key, map.entry(layout, position));
                drop_in_place(&info.value, map.value_of(layout, position));
            }
        }
    }

    // SAFETY: the blocks are this map's own.
    unsafe { map.release_storage(layout) };
    *map = ScienceMap::empty(info);
}

/// `Map::len(&self) -> Int`, in entries.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`ScienceMap`].
#[no_mangle]
pub unsafe extern "C" fn science_map_len(map: *const ScienceMap) -> i64 {
    // SAFETY: the caller guarantees a live map.
    unsafe { (*map).len as i64 }
}

/// `Map::insert(&mut self, key: K, value: V) -> V?`.
///
/// Both `key` and `value` are **moved** into the map: after the call the
/// caller's slots are logically uninitialised and must not be dropped.
///
/// The owned-`T?` convention of the crate documentation, §5.3: returns
/// `true` when the key was already present, having moved the displaced value
/// into `out_old`, which the caller now owns; returns `false` when the key is
/// new, in which case `out_old` is **not written** and the answer is `null`.
///
/// A new key is appended, so it comes last in iteration order. A key that was
/// present **keeps its position** (`stdlib-core.md` §3.6: *"re-inserting an
/// existing key does not move it"*).
///
/// On a replacement the map **destroys the key it was already holding** and
/// stores the one it has just been given. The two compare equal by `eq_fn`, so
/// which one survives is observable only through `Drop`; keeping the new one
/// is what lets both parameters be plain `*const u8` sources that the runtime
/// only ever reads, which is the same convention as everywhere else in this
/// crate.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`ScienceMap`]; `info` must
/// be the descriptor it was created with; `key` and `value` must be non-null,
/// aligned, point to initialised values of the described types, and not alias
/// the map's own storage; `out_old` must be non-null, aligned for the value
/// type, and writable for `value.size` bytes.
#[no_mangle]
pub unsafe extern "C" fn science_map_insert(
    map: *mut ScienceMap,
    info: *const ScienceMapInfo,
    key: *const u8,
    value: *const u8,
    out_old: *mut u8,
) -> bool {
    // SAFETY: the caller guarantees a live map and its descriptor.
    let (map, info) = unsafe { (&mut *map, &*info) };
    // SAFETY: `info` describes this map.
    unsafe { map.reserve_one(info) };
    let layout = entry_layout(info);

    let mask = map.cap - 1;
    // SAFETY: the caller guarantees `key` is a live key of the described type.
    let start = (mix(unsafe { (info.hash_fn)(key) }) as usize) & mask;
    let mut index = start;
    let mut first_tombstone: Option<usize> = None;

    loop {
        // SAFETY: `index` is masked into range.
        let word = unsafe { map.slot(index) };
        match (word & SLOT_STATE_MASK) as u8 {
            SCIENCE_MAP_SLOT_EMPTY => {
                // Prefer the earliest tombstone on the probe path, so the chain
                // stays as short as it can. The entry itself always goes at the
                // end: a reused slot says nothing about where the key sorts.
                let slot = first_tombstone.unwrap_or(index);
                let position = map.used;
                // SAFETY: `reserve_one` left room for one more position, which
                // is uninitialised and writable; the sources are the caller's
                // distinct allocations.
                unsafe {
                    std::ptr::copy_nonoverlapping(key, map.entry(layout, position), info.key.size);
                    std::ptr::copy_nonoverlapping(
                        value,
                        map.value_of(layout, position),
                        info.value.size,
                    );
                    *map.live.add(position) = 1;
                    *map.slots.add(slot) =
                        (position << SLOT_STATE_BITS) | SCIENCE_MAP_SLOT_OCCUPIED as usize;
                }
                map.used += 1;
                map.len += 1;
                return false;
            }
            SCIENCE_MAP_SLOT_TOMBSTONE => {
                if first_tombstone.is_none() {
                    first_tombstone = Some(index);
                }
            }
            _ => {
                let position = word >> SLOT_STATE_BITS;
                // SAFETY: an occupied slot names a live entry, whose key is at
                // its start; `key` is live.
                if unsafe { (info.eq_fn)(map.entry(layout, position), key) } {
                    // SAFETY: the entry holds a live value, which moves out to
                    // `out_old`; the new value then moves in. The stored key
                    // is destroyed and replaced by the one just supplied. The
                    // position does not change.
                    unsafe {
                        let stored = map.value_of(layout, position);
                        std::ptr::copy_nonoverlapping(stored, out_old, info.value.size);
                        std::ptr::copy_nonoverlapping(value, stored, info.value.size);
                        let stored_key = map.entry(layout, position);
                        drop_in_place(&info.key, stored_key);
                        std::ptr::copy_nonoverlapping(key, stored_key, info.key.size);
                    }
                    return true;
                }
            }
        }
        index = (index + 1) & mask;
    }
}

/// `Map::get(&self, key: &K) -> (borrowed V)?`.
///
/// The niche convention of the crate documentation, §5.3: the return value
/// **is** the `(borrowed V)?`. The null pointer is `null`; anything else is the
/// borrow itself.
///
/// `key` is borrowed, not moved: the caller still owns it afterwards.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`ScienceMap`]; `info` must
/// be the descriptor it was created with; `key` must be non-null, aligned, and
/// point to a live key of the described type.
#[no_mangle]
pub unsafe extern "C" fn science_map_get(
    map: *const ScienceMap,
    info: *const ScienceMapInfo,
    key: *const u8,
) -> *const u8 {
    // SAFETY: the caller guarantees a live map, its descriptor and a live key.
    let (map, info) = unsafe { (&*map, &*info) };
    // SAFETY: as above.
    match unsafe { map.find(info, key) } {
        // SAFETY: `find` only returns a live entry's position.
        Some((_, position)) => unsafe { map.value_of(entry_layout(info), position) },
        None => std::ptr::null(),
    }
}

/// `Map::contains(&self, key: &K) -> Bool`.
///
/// # Safety
///
/// As [`science_map_get`].
#[no_mangle]
pub unsafe extern "C" fn science_map_contains(
    map: *const ScienceMap,
    info: *const ScienceMapInfo,
    key: *const u8,
) -> bool {
    // SAFETY: the caller's obligations are `science_map_get`'s.
    unsafe { (*map).find(&*info, key).is_some() }
}

/// `Map::remove(&mut self, key: &K) -> V?`.
///
/// The owned-`T?` convention of the crate documentation, §5.3: returns
/// `true` after **moving** the entry's value into `out_value`, which the caller
/// now owns, or `false` when the key is absent, in which case `out_value` is
/// **not written** and the answer is `null`.
///
/// The entry's key belonged to the map, so it is destroyed here. The `key`
/// parameter is only a borrowed probe and is untouched. The entry's position
/// becomes a hole, so every other entry keeps its place in the order.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`ScienceMap`]; `info` must
/// be the descriptor it was created with; `key` must be non-null, aligned, and
/// point to a live key; `out_value` must be non-null, aligned for the value
/// type, and writable for `value.size` bytes.
#[no_mangle]
pub unsafe extern "C" fn science_map_remove(
    map: *mut ScienceMap,
    info: *const ScienceMapInfo,
    key: *const u8,
    out_value: *mut u8,
) -> bool {
    // SAFETY: the caller guarantees a live map and its descriptor.
    let (map, info) = unsafe { (&mut *map, &*info) };
    // SAFETY: as above, plus a live key.
    let Some((slot, position)) = (unsafe { map.find(info, key) }) else {
        return false;
    };
    let layout = entry_layout(info);
    // SAFETY: `find` returns a live entry, so its value is live and moves out
    // to `out_value`, and its key is the map's to destroy.
    unsafe {
        std::ptr::copy_nonoverlapping(map.value_of(layout, position), out_value, info.value.size);
        drop_in_place(&info.key, map.entry(layout, position));
        *map.live.add(position) = 0;
        *map.slots.add(slot) = SCIENCE_MAP_SLOT_TOMBSTONE as usize;
    }
    map.len -= 1;
    true
}

// --- Iteration, in insertion order --------------------------------------------
//
// **Two entry points, a bound and an accessor, rather than an iterator
// object.** `science-mir` lowers `for entry in counts:` the way it lowers
// `for x in xs:` over an `Array`: a shared borrow of the map taken before the
// loop and read on every turn, a cursor that is a local of the loop, and one
// call per turn that hands back the element *through that borrow*. That shape
// is what makes `for e in m: m.insert(…)` a borrow conflict with no rule of
// its own, and it needs exactly an extent and a positional accessor — an
// iterator struct the runtime owned would hold a pointer to the map that
// region inference cannot see, which is the hole
// `collections-and-chains.md`'s AMENDMENT 11a records one construct over.

/// The number of entry positions a loop over the map walks: every entry ever
/// written since the last rebuild, live or removed.
///
/// A loop reads it once, before its first turn. That is correct because the
/// loop holds a shared borrow of the map for its whole length, so nothing can
/// insert or remove while it runs.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`ScienceMap`].
#[no_mangle]
pub unsafe extern "C" fn science_map_extent(map: *const ScienceMap) -> i64 {
    // SAFETY: the caller guarantees a live map.
    unsafe { (*map).used as i64 }
}

/// The entry at `position` in insertion order, as a `(&Entry[K, V])?` — or,
/// for a `Set[T]`, whose entry is its element, a `(&T)?`.
///
/// The niche convention of the crate documentation, §5.3, as
/// [`science_map_get`] uses it: null when `position` is a removed entry's hole
/// or outside `0..extent`, and otherwise the borrow itself. The loop skips a
/// null and moves on, which is how a removal stays invisible to iteration.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`ScienceMap`], and
/// `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn science_map_entry_at(
    map: *const ScienceMap,
    info: *const ScienceMapInfo,
    position: i64,
) -> *const u8 {
    // SAFETY: the caller guarantees a live map and its descriptor.
    let (map, info) = unsafe { (&*map, &*info) };
    let Ok(position) = usize::try_from(position) else { return std::ptr::null() };
    // SAFETY: `position < used` is checked before the liveness byte is read.
    if position >= map.used || !unsafe { map.is_live(position) } {
        return std::ptr::null();
    }
    // SAFETY: a live position is inside the entry array.
    unsafe { map.entry(entry_layout(info), position) }
}

// --- Set: a `Map` whose value is `()` -----------------------------------------
//
// **`Set of T` has no representation of its own.** It is a [`ScienceMap`]
// whose [`ScienceMapInfo`] describes a zero-sized value, which every function
// above already handles without a case for it: [`science_alloc`] answers a
// zero-byte request with a dangling pointer, [`science_dealloc`] ignores one,
// and a zero-byte `copy_nonoverlapping` is a no-op. So `Set.new`, `contains`,
// `length` and a set's drop are [`science_map_new`], [`science_map_contains`],
// [`science_map_len`] and [`science_map_free`] with that descriptor, and the
// two below exist only because a set's `insert` and `remove` answer a
// different question from a map's. Iteration is [`science_map_extent`] and
// [`science_map_entry_at`] unchanged: a zero-sized value puts nothing after
// the key in an entry, so a set's entry *is* its element and the pointer the
// accessor returns is already the `&T` that `for x in set:` binds.
//
// `collections-and-chains.md` §5.1's own argument for `Set` is that without it
// *"users write `Map of (T, ())`, which is worse in every way"*. That is worse
// as a *surface*; as a representation it is exactly right, and building a
// second hash table to avoid it would be a second place for probing,
// tombstones and the load factor to be wrong.

/// `Set::insert(&mut self, value: T) -> Bool`.
///
/// **`true` when the value was not already present**, which is the question a
/// work-queue asks — *"is this node new?"* — and is the opposite polarity from
/// [`science_map_insert`]'s *"was something displaced?"*. The value is moved
/// in either way: when an equal one is already present, the set keeps the one
/// it was given and destroys the one it held, which is `science_map_insert`'s
/// behaviour and is observable only through `Drop`.
///
/// # Safety
///
/// `set` must be a non-null, aligned pointer to a live set; `info` must be the
/// descriptor it was created with, and describe a zero-sized value; `value`
/// must be non-null, aligned, point to an initialised value of the described
/// key type, and not alias the set's own storage.
#[no_mangle]
pub unsafe extern "C" fn science_set_insert(
    set: *mut ScienceMap,
    info: *const ScienceMapInfo,
    value: *const u8,
) -> bool {
    // SAFETY: the caller's obligations are `science_map_insert`'s; the value
    // slot is zero-sized, so a dangling pointer is a valid source and target.
    unsafe {
        let unit = dangling((*info).value.align);
        !science_map_insert(set, info, value, unit, unit)
    }
}

/// `Set::remove(&mut self, value: &T) -> Bool`: `true` when the value was
/// present and is now gone. The stored value belonged to the set and is
/// destroyed; the probe is borrowed and untouched.
///
/// # Safety
///
/// As [`science_set_insert`], except that `value` is only read.
#[no_mangle]
pub unsafe extern "C" fn science_set_remove(
    set: *mut ScienceMap,
    info: *const ScienceMapInfo,
    value: *const u8,
) -> bool {
    // SAFETY: as above.
    unsafe {
        let unit = dangling((*info).value.align);
        science_map_remove(set, info, value, unit)
    }
}

// --- Key support: the `hash_fn`/`eq_fn` pair for `Int` -----------------------
//
// **Decided here, because the measurement that prompted it is worth writing
// down.** `ScienceMapInfo` needs a `hash_fn` and an `eq_fn` for every key type,
// and until now this crate supplied exactly one pair's worth of them:
// `science_string_hash` and `science_string_eq` in `string.rs`, both documented
// as *"the natural `hash_fn`/`eq_fn` for a `Map` keyed by `String`"*. `Int` —
// the other key type the prelude's own examples use, in
// `examples/19_stdlib.science`'s `Map of (String, Int)` and in §3.6's
// discussion of `a Map of (Int, _)` — had **no pair at all**. `tests/map.rs`
// did not notice, because it declares `hash_u64` and `eq_u64` in its own
// `tests/common/mod.rs` and passes those; every test passed while the compiler
// had nothing it could name.
//
// The alternative was for codegen to emit the pair itself, which it can: they
// are a load and a compare, and `EMITTED_FN_SIGNATURES` in
// `science-codegen/src/runtime.rs` already states the two C signatures. It was
// rejected because `String`'s pair is a runtime symbol and a key type whose
// support lives in two different places is a key type whose support disagrees
// in two different places. One mechanism — look the symbol pair up by key type,
// declare it, take its address — covers both.
//
// **The cost, stated rather than hidden: this pair is for eight-byte integer
// keys only.** A `ScienceHashFn` receives a `*const u8` and no size, so it
// cannot ask how wide its key is; reading eight bytes out of a one-byte slot is
// not a worse hash, it is a read past the end of the key array. A `Map of (U8,
// _)` therefore still has no pair, and adding one means adding a symbol per
// width rather than widening this one.

/// `Hash` for an eight-byte integer key — `Int`, and any other 64-bit integer.
///
/// **Returns the key's bits unchanged, and that is the whole function.**
/// [`ScienceHashFn`] requires only that equal keys hash equal, and the table
/// puts every hash it is given through splitmix64 before the low bits select a
/// slot, so the identity is already a well-distributed hash *as the table uses
/// it*. Hashing here as well would be a second mix of the same bits, which
/// costs instructions in the hottest loop the map has and buys nothing the
/// first mix did not already buy.
///
/// [`ScienceHashFn`]: crate::ScienceHashFn
///
/// # Safety
///
/// `key` must be non-null, aligned for `i64`, and point to a live eight-byte
/// integer.
#[no_mangle]
pub unsafe extern "C" fn science_int_hash(key: *const u8) -> u64 {
    // SAFETY: the caller guarantees a live, aligned eight-byte integer.
    unsafe { key.cast::<u64>().read() }
}

/// `Eq::eq` for an eight-byte integer key.
///
/// Compares the bits, which for a fixed-width two's-complement integer is the
/// same relation as `==` at every signedness — so one symbol serves `Int`,
/// `I64` and `U64` rather than three. This is exactly the relation
/// [`science_int_hash`] agrees with: equal bits, equal hash.
///
/// # Safety
///
/// Both pointers must be non-null, aligned for `i64`, and point to live
/// eight-byte integers.
#[no_mangle]
pub unsafe extern "C" fn science_int_eq(a: *const u8, b: *const u8) -> bool {
    // SAFETY: the caller guarantees two live, aligned eight-byte integers.
    unsafe { a.cast::<u64>().read() == b.cast::<u64>().read() }
}
