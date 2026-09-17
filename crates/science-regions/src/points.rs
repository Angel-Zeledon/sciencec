//! Decision 1, made concrete: a region *is* a bitset over a body's points.
//!
//! > **Decision 1.** A region is a set of MIR points, and the lattice is set
//! > inclusion. A point is a `(basic block, statement index)` pair.
//!
//! # 1. Why an index and not a `BTreeSet<Point>`
//!
//! `region-inference.md` §3 claims the algorithm is `O(points × regions)`
//! *"with a bitset representation"*, and `science-mir`'s §5 hands over the half
//! that makes one possible: [`Body::points`] enumerates in a *"dense, stable
//! order, so a region is a bitset indexed by position in that iteration"*.
//!
//! This module is that sentence, written down once. Everything above it says
//! [`Point`]; everything inside it says `usize`; [`PointIndex`] is the only
//! place the two meet, so a change to MIR's enumeration order is a change to
//! one function rather than to every set operation in the crate.
//!
//! **What it costs** is a `Vec<u32>` per body — one entry per block, holding
//! where that block's points start — and the discipline that a [`RegionSet`]
//! is meaningless without the [`PointIndex`] it was built against. Two bodies'
//! sets are not comparable and nothing here checks that, exactly as
//! `science_types::ty::Ty` does not check which table it came from.
//!
//! # 2. Indices, not pointers — Decision 11
//!
//! A [`RegionSet`] is a `Vec<u64>`, a [`PointIndex`] is a `Vec<u32>`, and
//! neither holds a reference to anything. §11's *"liveness sets are bitsets —
//! owned `Array of U64` per point. Fine."* is the line this file is written to
//! keep true.

use science_mir::mir::{BlockId, Body, Point};

/// The dense numbering of one body's points.
///
/// Built once per body and threaded everywhere, because a [`RegionSet`] built
/// against one numbering and read against another is a wrong answer with no
/// symptom.
#[derive(Debug, Clone)]
pub struct PointIndex {
    /// Where each block's points begin in the dense numbering.
    first_of_block: Vec<u32>,
    /// How many points there are in total.
    count: usize,
}

impl PointIndex {
    /// The numbering of a body's points, in [`Body::points`] order.
    pub fn of(body: &Body) -> PointIndex {
        let mut first_of_block = Vec::with_capacity(body.block_count());
        let mut next = 0u32;
        for (_, block) in body.blocks() {
            first_of_block.push(next);
            next += block.statements.len() as u32 + 1;
        }
        PointIndex { first_of_block, count: next as usize }
    }

    /// How many points the body has. [`Body::point_count`], by construction.
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// A point's position in the dense numbering.
    pub fn index(&self, point: Point) -> usize {
        self.first_of_block[point.block.index()] as usize + point.statement as usize
    }

    /// The point at a position, which is the inverse of [`PointIndex::index`].
    ///
    /// A binary search rather than a second `Vec<Point>`: a diagnostic turns an
    /// index back into a point a handful of times per error and the forward
    /// direction happens once per constraint per iteration of the fixpoint.
    pub fn point(&self, index: usize) -> Point {
        let block = match self.first_of_block.binary_search(&(index as u32)) {
            Ok(at) => at,
            Err(after) => after - 1,
        };
        Point {
            block: BlockId::from_index(block),
            statement: index as u32 - self.first_of_block[block],
        }
    }

    /// The first point of a block, which is where control enters it.
    pub fn entry_of(&self, block: BlockId) -> usize {
        self.first_of_block[block.index()] as usize
    }

    /// Every point, in the dense order.
    pub fn all(&self) -> impl Iterator<Item = usize> {
        0..self.count
    }
}

/// A dense set of small numbers, and the only set representation in the crate.
///
/// **One structure for regions and for liveness, on purpose.** §11 takes the
/// algorithm apart and says of two of its pieces — *"liveness sets are bitsets
/// — owned `Array of U64` per point"* and, of a region, that it is a bitset
/// over points — the same sentence. Two types would be two implementations of
/// `union` to keep in step, for a distinction that is carried by the variable's
/// name at every use site anyway.
///
/// `PartialOrd` is deliberately *not* derived. The lattice's order is set
/// inclusion, which is partial, and a derived lexicographic `<` on the word
/// vector would compile and mean something else. [`Bits::contains_all`] is the
/// order, spelled so it cannot be mistaken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bits {
    words: Vec<u64>,
}

/// One region: a set of MIR points, and one element of Decision 1's lattice.
///
/// An alias rather than a newtype for [`Bits`]'s reason. What makes a `Bits` a
/// *region* is that it is indexed by a [`PointIndex`], and that is a fact about
/// the numbering it was built against rather than about the bits.
pub type RegionSet = Bits;

impl Bits {
    /// The empty set over `len` elements — the lattice bottom.
    pub fn empty(len: usize) -> Bits {
        Bits { words: vec![0; len.div_ceil(64)] }
    }

    /// Every element — what a parameter's region gets, because a caller's
    /// region outlives the whole of this body. See [`crate::generate`]'s §4.
    pub fn full(len: usize) -> Bits {
        let mut set = Bits::empty(len);
        for point in 0..len {
            set.insert(point);
        }
        set
    }

    /// Adds a point, and answers whether the set changed.
    ///
    /// The answer is the whole of the worklist's termination argument: the
    /// fixpoint of `solve` runs until no `insert` anywhere returns `true`, and
    /// since a set only ever grows and is bounded by the point count, it does.
    pub fn insert(&mut self, point: usize) -> bool {
        let (word, bit) = (point / 64, point % 64);
        let mask = 1u64 << bit;
        let changed = self.words[word] & mask == 0;
        self.words[word] |= mask;
        changed
    }

    /// Removes an element. Liveness's kill half; regions never shrink.
    pub fn remove(&mut self, point: usize) {
        self.words[point / 64] &= !(1u64 << (point % 64));
    }

    pub fn contains(&self, point: usize) -> bool {
        self.words[point / 64] & (1u64 << (point % 64)) != 0
    }

    /// Unions `other` in, and answers whether the set changed.
    pub fn union_with(&mut self, other: &Bits) -> bool {
        let mut changed = false;
        for (slot, word) in self.words.iter_mut().zip(&other.words) {
            let merged = *slot | *word;
            changed |= merged != *slot;
            *slot = merged;
        }
        changed
    }

    /// Removes every point not in `other`. Decision 3's intersection, §4.3.
    pub fn intersect_with(&mut self, other: &Bits) {
        for (slot, word) in self.words.iter_mut().zip(&other.words) {
            *slot &= *word;
        }
    }

    /// Whether this region contains every point of `other` — the lattice's
    /// order, and what `'self: 'other` asserts.
    pub fn contains_all(&self, other: &Bits) -> bool {
        self.words.iter().zip(&other.words).all(|(mine, theirs)| mine & theirs == *theirs)
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|word| *word == 0)
    }

    pub fn len(&self) -> usize {
        self.words.iter().map(|word| word.count_ones() as usize).sum()
    }

    /// The points in the set, in the dense order.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(at, word)| {
            (0..64).filter(move |bit| word & (1u64 << bit) != 0).map(move |bit| at * 64 + bit)
        })
    }
}
