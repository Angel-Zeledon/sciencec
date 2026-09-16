//! `Map[K, V]`: a hash table. §8's method set in full, over the
//! element-descriptor convention of the crate documentation, §6.
//!
//! # The table
//!
//! Open addressing with linear probing and tombstone deletion, over a
//! power-of-two capacity, kept below three-quarters full. Storage is three
//! parallel arrays — one state byte per slot, one key slot, one value slot —
//! rather than one array of key-value pairs, because the runtime does not know
//! the pair's layout: it would have to compute the padding between a `K` and a
//! `V` from their sizes and alignments, and get it right for every
//! instantiation. Three arrays make that question disappear, and cost one extra
//! allocation per table.
//!
//! Linear probing over a byte array of states is friendly to the cache, and it
//! keeps deletion simple: a removed slot becomes a tombstone, which lookups
//! walk past and insertions may reuse. Tombstones count against the load
//! factor, so a table that is inserted into and removed from forever is rebuilt
//! periodically instead of degrading into a linear scan.
//!
//! Nothing about the probe order is part of the ABI. Codegen must not assume
//! any iteration order, and §8 gives Link no way to observe one.

use crate::abi::LinkMapInfo;
use crate::mem::{capacity_overflow, dangling, link_alloc, link_dealloc};

/// Slot state: never used.
pub const LINK_MAP_SLOT_EMPTY: u8 = 0;
/// Slot state: holds a live key and value.
pub const LINK_MAP_SLOT_OCCUPIED: u8 = 1;
/// Slot state: held an entry that was removed. Lookups probe past it;
/// insertions may claim it.
pub const LINK_MAP_SLOT_TOMBSTONE: u8 = 2;

/// Link's `Map[K, V]`.
///
/// Layout: six words, `{ states, keys, values, len, tombstones, cap }`.
///
/// - `states`, `keys` and `values` are three parallel arrays of `cap` slots.
///   None is ever null; all three are dangling but aligned when `cap == 0`.
/// - Slot `i` holds a live key at `keys + i * key.size` and a live value at
///   `values + i * value.size` exactly when `states[i]` is
///   [`LINK_MAP_SLOT_OCCUPIED`].
/// - `len` is the number of occupied slots, and is what §8's `len` returns.
/// - `tombstones` is the number of slots in [`LINK_MAP_SLOT_TOMBSTONE`].
/// - `cap` is zero, or a power of two. A new `Map` allocates nothing.
///
/// As with `Array`, the key and value types appear nowhere in this struct: a
/// [`LinkMapInfo`] describes them on every call, and **it must be the same
/// descriptor every time**.
///
/// Implements no `Drop`: moved by copying six words, destroyed by
/// [`link_map_free`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LinkMap {
    /// One state byte per slot.
    pub states: *mut u8,
    /// `cap` key slots.
    pub keys: *mut u8,
    /// `cap` value slots.
    pub values: *mut u8,
    /// Occupied slots.
    pub len: usize,
    /// Tombstoned slots.
    pub tombstones: usize,
    /// Slot count: zero, or a power of two.
    pub cap: usize,
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

impl LinkMap {
    fn empty(info: &LinkMapInfo) -> Self {
        LinkMap {
            states: dangling(1),
            keys: dangling(info.key.align),
            values: dangling(info.value.align),
            len: 0,
            tombstones: 0,
            cap: 0,
        }
    }

    /// # Safety
    ///
    /// `index` must be less than `cap`, and `info` must describe this table.
    #[inline]
    unsafe fn key_slot(&self, info: &LinkMapInfo, index: usize) -> *mut u8 {
        // SAFETY: `index < cap`, so the offset is inside the key array.
        unsafe { self.keys.add(info.key.offset_of(index)) }
    }

    /// # Safety
    ///
    /// `index` must be less than `cap`, and `info` must describe this table.
    #[inline]
    unsafe fn value_slot(&self, info: &LinkMapInfo, index: usize) -> *mut u8 {
        // SAFETY: `index < cap`, so the offset is inside the value array.
        unsafe { self.values.add(info.value.offset_of(index)) }
    }

    /// # Safety
    ///
    /// `index` must be less than `cap`.
    #[inline]
    unsafe fn state(&self, index: usize) -> u8 {
        // SAFETY: `index < cap`, so the offset is inside the state array.
        unsafe { *self.states.add(index) }
    }

    /// Index of the slot holding `key`, if any.
    ///
    /// # Safety
    ///
    /// `info` must describe this table, and `key` must point to a live key.
    unsafe fn find(&self, info: &LinkMapInfo, key: *const u8) -> Option<usize> {
        if self.cap == 0 {
            return None;
        }
        let mask = self.cap - 1;
        // SAFETY: the caller guarantees `key` is a live key for this table.
        let mut index = (mix(unsafe { (info.hash_fn)(key) }) as usize) & mask;
        loop {
            // SAFETY: `index` is masked into range.
            match unsafe { self.state(index) } {
                LINK_MAP_SLOT_EMPTY => return None,
                LINK_MAP_SLOT_OCCUPIED => {
                    // SAFETY: an occupied slot holds a live key, and `key` is
                    // live; both are of the type `info.key` describes.
                    if unsafe { (info.eq_fn)(self.key_slot(info, index), key) } {
                        return Some(index);
                    }
                }
                _ => {}
            }
            index = (index + 1) & mask;
            // The loop terminates because the table always keeps at least one
            // empty slot: `reserve_one` rebuilds it before occupied plus
            // tombstoned slots could reach capacity.
        }
    }

    /// Store a key and value in a table known not to contain the key and known
    /// to have no tombstones. Used only while rebuilding.
    ///
    /// # Safety
    ///
    /// `info` must describe this table; `cap` must be non-zero; `key` and
    /// `value` must point to initialised values that are being moved in; the
    /// key must be absent.
    unsafe fn insert_fresh(&mut self, info: &LinkMapInfo, key: *const u8, value: *const u8) {
        let mask = self.cap - 1;
        // SAFETY: the caller guarantees `key` is live.
        let mut index = (mix(unsafe { (info.hash_fn)(key) }) as usize) & mask;
        // SAFETY: `index` is masked into range.
        while unsafe { self.state(index) } != LINK_MAP_SLOT_EMPTY {
            index = (index + 1) & mask;
        }
        // SAFETY: `index` is in range and its slot is empty, so both slots are
        // uninitialised and writable; the sources are distinct allocations.
        unsafe {
            std::ptr::copy_nonoverlapping(key, self.key_slot(info, index), info.key.size);
            std::ptr::copy_nonoverlapping(value, self.value_slot(info, index), info.value.size);
            *self.states.add(index) = LINK_MAP_SLOT_OCCUPIED;
        }
        self.len += 1;
    }

    /// Rebuild the table at `new_cap` slots, relocating every live entry.
    ///
    /// Relocation is a bitwise move: no destructor and no clone runs, because
    /// moving an owned value is invisible in Link (§6.1 rule 2).
    ///
    /// # Safety
    ///
    /// `info` must describe this table, and `new_cap` must be a power of two
    /// large enough for `len` entries at the load factor.
    unsafe fn rehash(&mut self, info: &LinkMapInfo, new_cap: usize) {
        let old = *self;

        let Some(key_bytes) = new_cap.checked_mul(info.key.size) else {
            capacity_overflow()
        };
        let Some(value_bytes) = new_cap.checked_mul(info.value.size) else {
            capacity_overflow()
        };

        // SAFETY: the sizes are checked above and the alignments come from the
        // descriptor, which guarantees powers of two.
        unsafe {
            self.states = link_alloc(new_cap, 1);
            std::ptr::write_bytes(self.states, LINK_MAP_SLOT_EMPTY, new_cap);
            self.keys = link_alloc(key_bytes, info.key.align);
            self.values = link_alloc(value_bytes, info.value.align);
        }
        self.cap = new_cap;
        self.len = 0;
        self.tombstones = 0;

        for index in 0..old.cap {
            // SAFETY: `index < old.cap`, so the old state array covers it.
            if unsafe { old.state(index) } != LINK_MAP_SLOT_OCCUPIED {
                continue;
            }
            // SAFETY: an occupied slot holds a live key and value, and the new
            // table is empty, so neither is already present.
            unsafe {
                self.insert_fresh(info, old.key_slot(info, index), old.value_slot(info, index))
            }
        }

        // SAFETY: the three old blocks came from `link_alloc` with exactly
        // these sizes and alignments; every entry they held has been moved out.
        unsafe {
            link_dealloc(old.states, old.cap, 1);
            link_dealloc(old.keys, info.key.offset_of(old.cap), info.key.align);
            link_dealloc(old.values, info.value.offset_of(old.cap), info.value.align);
        }
    }

    /// Guarantee room for one more entry, rebuilding if the table is too full.
    ///
    /// The load factor counts tombstones as well as live entries, so a table
    /// that is repeatedly inserted into and removed from is rebuilt rather than
    /// filling with tombstones and degrading into a linear scan. The rebuild
    /// sizes for the live entries alone, which is what reclaims them.
    ///
    /// # Safety
    ///
    /// `info` must describe this table.
    unsafe fn reserve_one(&mut self, info: &LinkMapInfo) {
        let used = self.len.saturating_add(self.tombstones).saturating_add(1);
        if self.cap != 0 && used.saturating_mul(4) <= self.cap.saturating_mul(3) {
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
        // SAFETY: `new_cap` is a power of two and leaves the table below the
        // load factor; `info` is the caller's obligation.
        unsafe { self.rehash(info, new_cap) }
    }
}

/// `Map::new()`.
///
/// Allocates nothing.
///
/// # Safety
///
/// `info` must be a non-null, aligned pointer to a valid [`LinkMapInfo`].
#[no_mangle]
pub unsafe extern "C" fn link_map_new(info: *const LinkMapInfo) -> LinkMap {
    // SAFETY: the caller guarantees a valid descriptor.
    LinkMap::empty(unsafe { &*info })
}

/// Release a `Map`, running the key and value destructors over every entry it
/// still holds.
///
/// **Codegen support:** this is `Map`'s drop glue (§6.1 rule 6). Entries whose
/// value was moved out by [`link_map_remove`] are not destroyed, since they are
/// no longer the map's.
///
/// The receiver is left as a valid empty map.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`LinkMap`] that has not
/// already been freed, and `info` must be the descriptor it was created with.
#[no_mangle]
pub unsafe extern "C" fn link_map_free(map: *mut LinkMap, info: *const LinkMapInfo) {
    // SAFETY: the caller guarantees a live map and its descriptor.
    let (map, info) = unsafe { (&mut *map, &*info) };

    if info.key.drop_fn.is_some() || info.value.drop_fn.is_some() {
        for index in 0..map.cap {
            // SAFETY: `index < cap`.
            if unsafe { map.state(index) } != LINK_MAP_SLOT_OCCUPIED {
                continue;
            }
            if let Some(drop_fn) = info.key.drop_fn {
                // SAFETY: an occupied slot holds a live key of the described
                // type.
                unsafe { drop_fn(map.key_slot(info, index)) }
            }
            if let Some(drop_fn) = info.value.drop_fn {
                // SAFETY: likewise for the value.
                unsafe { drop_fn(map.value_slot(info, index)) }
            }
        }
    }

    // SAFETY: the three blocks came from `link_alloc` with exactly these sizes
    // and alignments.
    unsafe {
        link_dealloc(map.states, map.cap, 1);
        link_dealloc(map.keys, info.key.offset_of(map.cap), info.key.align);
        link_dealloc(map.values, info.value.offset_of(map.cap), info.value.align);
    }
    *map = LinkMap::empty(info);
}

/// `Map::len(&self) -> Int`, in entries.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`LinkMap`].
#[no_mangle]
pub unsafe extern "C" fn link_map_len(map: *const LinkMap) -> i64 {
    // SAFETY: the caller guarantees a live map.
    unsafe { (*map).len as i64 }
}

/// `Map::insert(&mut self, key: K, value: V) -> Option[V]`.
///
/// Both `key` and `value` are **moved** into the map: after the call the
/// caller's slots are logically uninitialised and must not be dropped.
///
/// The owned-`Option` convention of the crate documentation, §5.3: returns
/// `true` when the key was already present, having moved the displaced value
/// into `out_old`, which the caller now owns; returns `false` when the key is
/// new, in which case `out_old` is **not written**.
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
/// `map` must be a non-null, aligned pointer to a live [`LinkMap`]; `info` must
/// be the descriptor it was created with; `key` and `value` must be non-null,
/// aligned, point to initialised values of the described types, and not alias
/// the map's own storage; `out_old` must be non-null, aligned for the value
/// type, and writable for `value.size` bytes.
#[no_mangle]
pub unsafe extern "C" fn link_map_insert(
    map: *mut LinkMap,
    info: *const LinkMapInfo,
    key: *const u8,
    value: *const u8,
    out_old: *mut u8,
) -> bool {
    // SAFETY: the caller guarantees a live map and its descriptor.
    let (map, info) = unsafe { (&mut *map, &*info) };
    // SAFETY: `info` describes this map.
    unsafe { map.reserve_one(info) };

    let mask = map.cap - 1;
    // SAFETY: the caller guarantees `key` is a live key of the described type.
    let start = (mix(unsafe { (info.hash_fn)(key) }) as usize) & mask;
    let mut index = start;
    let mut first_tombstone: Option<usize> = None;

    loop {
        // SAFETY: `index` is masked into range.
        match unsafe { map.state(index) } {
            LINK_MAP_SLOT_EMPTY => {
                // Prefer the earliest tombstone on the probe path, so the chain
                // stays as short as it can.
                let slot = match first_tombstone {
                    Some(slot) => {
                        map.tombstones -= 1;
                        slot
                    }
                    None => index,
                };
                // SAFETY: `slot` is in range and holds no live key or value, so
                // both of its slots are writable; the sources are the caller's
                // distinct allocations.
                unsafe {
                    std::ptr::copy_nonoverlapping(key, map.key_slot(info, slot), info.key.size);
                    std::ptr::copy_nonoverlapping(
                        value,
                        map.value_slot(info, slot),
                        info.value.size,
                    );
                    *map.states.add(slot) = LINK_MAP_SLOT_OCCUPIED;
                }
                map.len += 1;
                return false;
            }
            LINK_MAP_SLOT_TOMBSTONE => {
                if first_tombstone.is_none() {
                    first_tombstone = Some(index);
                }
            }
            _ => {
                // SAFETY: an occupied slot holds a live key, and `key` is live.
                if unsafe { (info.eq_fn)(map.key_slot(info, index), key) } {
                    // SAFETY: the slot holds a live value, which moves out to
                    // `out_old`; the new value then moves in. The stored key is
                    // destroyed and replaced by the one just supplied.
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            map.value_slot(info, index),
                            out_old,
                            info.value.size,
                        );
                        std::ptr::copy_nonoverlapping(
                            value,
                            map.value_slot(info, index),
                            info.value.size,
                        );
                        if let Some(drop_fn) = info.key.drop_fn {
                            drop_fn(map.key_slot(info, index));
                        }
                        std::ptr::copy_nonoverlapping(
                            key,
                            map.key_slot(info, index),
                            info.key.size,
                        );
                    }
                    return true;
                }
            }
        }
        index = (index + 1) & mask;
    }
}

/// `Map::get(&self, key: &K) -> Option[&V]`.
///
/// The niche convention of the crate documentation, §5.3: the return value
/// **is** the `Option[&V]`. Null is `None`; anything else is `Some(&V)`.
///
/// `key` is borrowed, not moved: the caller still owns it afterwards.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`LinkMap`]; `info` must
/// be the descriptor it was created with; `key` must be non-null, aligned, and
/// point to a live key of the described type.
#[no_mangle]
pub unsafe extern "C" fn link_map_get(
    map: *const LinkMap,
    info: *const LinkMapInfo,
    key: *const u8,
) -> *const u8 {
    // SAFETY: the caller guarantees a live map, its descriptor and a live key.
    let (map, info) = unsafe { (&*map, &*info) };
    // SAFETY: as above.
    match unsafe { map.find(info, key) } {
        // SAFETY: `find` only returns an in-range, occupied slot.
        Some(index) => unsafe { map.value_slot(info, index) },
        None => std::ptr::null(),
    }
}

/// `Map::contains(&self, key: &K) -> Bool`.
///
/// # Safety
///
/// As [`link_map_get`].
#[no_mangle]
pub unsafe extern "C" fn link_map_contains(
    map: *const LinkMap,
    info: *const LinkMapInfo,
    key: *const u8,
) -> bool {
    // SAFETY: the caller's obligations are `link_map_get`'s.
    unsafe { (*map).find(&*info, key).is_some() }
}

/// `Map::remove(&mut self, key: &K) -> Option[V]`.
///
/// The owned-`Option` convention of the crate documentation, §5.3: returns
/// `true` after **moving** the entry's value into `out_value`, which the caller
/// now owns, or `false` when the key is absent, in which case `out_value` is
/// **not written**.
///
/// The entry's key belonged to the map, so it is destroyed here. The `key`
/// parameter is only a borrowed probe and is untouched.
///
/// # Safety
///
/// `map` must be a non-null, aligned pointer to a live [`LinkMap`]; `info` must
/// be the descriptor it was created with; `key` must be non-null, aligned, and
/// point to a live key; `out_value` must be non-null, aligned for the value
/// type, and writable for `value.size` bytes.
#[no_mangle]
pub unsafe extern "C" fn link_map_remove(
    map: *mut LinkMap,
    info: *const LinkMapInfo,
    key: *const u8,
    out_value: *mut u8,
) -> bool {
    // SAFETY: the caller guarantees a live map and its descriptor.
    let (map, info) = unsafe { (&mut *map, &*info) };
    // SAFETY: as above, plus a live key.
    let Some(index) = (unsafe { map.find(info, key) }) else {
        return false;
    };
    // SAFETY: `find` returns an in-range, occupied slot, so its value is live
    // and moves out to `out_value`, and its key is the map's to destroy.
    unsafe {
        std::ptr::copy_nonoverlapping(map.value_slot(info, index), out_value, info.value.size);
        if let Some(drop_fn) = info.key.drop_fn {
            drop_fn(map.key_slot(info, index));
        }
        *map.states.add(index) = LINK_MAP_SLOT_TOMBSTONE;
    }
    map.len -= 1;
    map.tombstones += 1;
    true
}
