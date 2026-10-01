//! The bundled `random` module, **built, linked, run** — and checked against
//! an implementation of the same algorithms written here, in Rust.
//!
//! # Why a second implementation and not a pinned printout
//!
//! A `.stdout` blessed from the module's own output certifies whatever the
//! module did the day it was blessed, and `NEXT-SESSION.md` records three pins
//! that certified a defect that way. The expected numbers below are computed
//! by [`reference`], which is written from the published descriptions of
//! Threefry-2x64-20 (Salmon et al., *Parallel Random Numbers: As Easy as 1, 2,
//! 3*, SC'11, and Random123's `threefry.h`), xoshiro256** and SplitMix64
//! (Blackman and Vigna), and Wichura's AS 241 — and `reference` is itself
//! checked against the known-answer vectors those sources publish, so a
//! mistake has to be made twice, the same way, in two languages, to pass.
//!
//! # Bit for bit, at both optimisation levels
//!
//! `stdlib-standard.md` §5.5 makes the `Key` face's numbers part of the
//! language's contract, so every comparison here is of bits, never within a
//! tolerance — including `normal`, whose `ln` and `sqrt` are the module's own
//! (`reproducibility.md` Decision 2) and are ported operation for operation
//! into [`reference`]. The programs are built at `-O0` and at `-O2`, because
//! constant folding is where a compiler would first round differently.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build `source` at `opt`, run it, and return its stdout — which must be the
/// whole of what it did.
fn prints_at(name: &str, source: &str, opt: OptLevel) -> String {
    let dir = scratch("random", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), opt);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// [`prints_at`] at `-O0` and at `-O2`, which must agree, and the one answer.
fn prints(name: &str, source: &str) -> String {
    let unoptimised = prints_at(&format!("{name}_o0"), source, OptLevel::O0);
    let optimised = prints_at(&format!("{name}_o2"), source, OptLevel::O2);
    assert_eq!(unoptimised, optimised, "`-O0` and `-O2` disagree on `{name}`");
    optimised
}

/// The algorithms `random.science` states, transcribed from their published
/// descriptions — and, for `ln`, `sqrt` and the inverse normal CDF, from the
/// module's own comments, operation for operation, because those three are
/// the module's and the contract is their bits.
mod reference {
    /// Skein's key-schedule parity constant, `threefry.h`'s `SKEIN_KS_PARITY64`.
    const PARITY: u64 = 0x1BD1_1BDA_A9FC_1A22;
    /// `threefry.h`'s `R_64x2_*` rotation constants, rounds 0 to 7.
    const ROTATIONS: [u32; 8] = [16, 42, 12, 31, 16, 32, 24, 21];

    /// Threefry-2x64 with `rounds` rounds, `threefry2x64_R` exactly.
    pub fn threefry(rounds: usize, key: [u64; 2], counter: [u64; 2]) -> [u64; 2] {
        let schedule = [key[0], key[1], PARITY ^ key[0] ^ key[1]];
        let mut x0 = counter[0].wrapping_add(schedule[0]);
        let mut x1 = counter[1].wrapping_add(schedule[1]);
        for round in 0..rounds {
            x0 = x0.wrapping_add(x1);
            x1 = x1.rotate_left(ROTATIONS[round % 8]) ^ x0;
            if round % 4 == 3 {
                let injection = round / 4 + 1;
                x0 = x0.wrapping_add(schedule[injection % 3]);
                x1 = x1.wrapping_add(schedule[(injection + 1) % 3]).wrapping_add(injection as u64);
            }
        }
        [x0, x1]
    }

    pub fn threefry20(key: [u64; 2], counter: [u64; 2]) -> [u64; 2] {
        threefry(20, key, counter)
    }

    // The counter's second word, by purpose — `random.science`'s layout.
    pub const SPLIT: u64 = 0;
    pub const FOLD: u64 = 1;
    pub const DRAW: u64 = 2;

    pub fn from_seed(seed: u64) -> [u64; 2] {
        [0, seed]
    }

    pub fn split_many(key: [u64; 2], count: u64) -> Vec<[u64; 2]> {
        (0..count).map(|i| threefry20(key, [i, SPLIT])).collect()
    }

    pub fn fold_in(key: [u64; 2], data: u64) -> [u64; 2] {
        threefry20(key, [data, FOLD])
    }

    /// The `index`th 64-bit word a key draws.
    pub fn word(key: [u64; 2], index: u64) -> u64 {
        threefry20(key, [index / 2, DRAW])[(index % 2) as usize]
    }

    /// A source of words: a key's draw sequence, or a stream.
    pub trait Words {
        fn next_word(&mut self) -> u64;
    }

    pub struct KeyWords {
        pub key: [u64; 2],
        pub index: u64,
    }

    impl Words for KeyWords {
        fn next_word(&mut self) -> u64 {
            let w = word(self.key, self.index);
            self.index += 1;
            w
        }
    }

    pub fn key_words(key: [u64; 2]) -> KeyWords {
        KeyWords { key, index: 0 }
    }

    pub fn splitmix64(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub struct Xoshiro(pub [u64; 4]);

    impl Xoshiro {
        pub fn from_seed(seed: u64) -> Xoshiro {
            let mut state = seed;
            Xoshiro([
                splitmix64(&mut state),
                splitmix64(&mut state),
                splitmix64(&mut state),
                splitmix64(&mut state),
            ])
        }
    }

    impl Words for Xoshiro {
        fn next_word(&mut self) -> u64 {
            let s = &mut self.0;
            let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
            let t = s[1] << 17;
            s[2] ^= s[0];
            s[3] ^= s[1];
            s[1] ^= s[2];
            s[0] ^= s[3];
            s[2] ^= t;
            s[3] = s[3].rotate_left(45);
            result
        }
    }

    pub const TWO_TO_MINUS_53: f64 = 1.0 / 9_007_199_254_740_992.0;

    pub fn unit(word: u64) -> f64 {
        (word >> 11) as f64 * TWO_TO_MINUS_53
    }

    pub fn open_unit(word: u64) -> f64 {
        ((word >> 11) as f64 + 0.5) * TWO_TO_MINUS_53
    }

    /// `[low, high)` by rejection: `random.science`'s `bounded`.
    pub fn integer(words: &mut impl Words, low: i64, high: i64) -> i64 {
        let span = (high as u64).wrapping_sub(low as u64);
        let threshold = 0u64.wrapping_sub(span) % span;
        loop {
            let w = words.next_word();
            if w >= threshold {
                return low.wrapping_add((w % span) as i64);
            }
        }
    }

    pub fn shuffle<T>(words: &mut impl Words, items: &mut [T]) {
        let n = items.len();
        for step in 0..n.saturating_sub(1) {
            let i = n - 1 - step;
            let j = integer(words, 0, i as i64 + 1) as usize;
            items.swap(i, j);
        }
    }

    // --- The module's own `ln`, `sqrt` and inverse normal CDF ------------

    const LN2_HIGH: f64 = 6.931_471_803_691_238_164_90e-1;
    const LN2_LOW: f64 = 1.908_214_929_270_587_700_02e-10;
    const SQRT_HALF: f64 = 0.707_106_781_186_547_6;

    pub fn ln(x: f64) -> f64 {
        let mut m = x;
        let mut e = 0i64;
        while m < SQRT_HALF {
            m *= 2.0;
            e -= 1;
        }
        while m >= 2.0 * SQRT_HALF {
            m *= 0.5;
            e += 1;
        }
        let s = (m - 1.0) / (m + 1.0);
        let s2 = s * s;
        let mut sum = 1.0 / 23.0;
        let mut k = 10i64;
        while k >= 0 {
            sum = sum * s2 + 1.0 / (2 * k + 1) as f64;
            k -= 1;
        }
        let ef = e as f64;
        ef * LN2_HIGH + (2.0 * s * sum + ef * LN2_LOW)
    }

    pub fn sqrt(x: f64) -> f64 {
        let mut y = if x > 1.0 { x } else { 1.0 };
        loop {
            let next = 0.5 * (y + x / y);
            if next >= y {
                return y;
            }
            y = next;
        }
    }

    fn polynomial(c: &[f64], x: f64) -> f64 {
        let mut sum = c[c.len() - 1];
        for coefficient in c[..c.len() - 1].iter().rev() {
            sum = sum * x + coefficient;
        }
        sum
    }

    const A: [f64; 8] = [
        3.387_132_872_796_366_608_0,
        1.331_416_678_917_843_774_5e2,
        1.971_590_950_306_551_442_7e3,
        1.373_169_376_550_946_112_5e4,
        4.592_195_393_154_987_145_7e4,
        6.726_577_092_700_870_085_3e4,
        3.343_057_558_358_812_810_5e4,
        2.509_080_928_730_122_672_7e3,
    ];
    const B: [f64; 8] = [
        1.0,
        4.231_333_070_160_091_125_2e1,
        6.871_870_074_920_579_083_0e2,
        5.394_196_021_424_751_107_7e3,
        2.121_379_430_158_659_586_7e4,
        3.930_789_580_009_271_061_0e4,
        2.872_908_573_572_194_267_4e4,
        5.226_495_278_852_854_561_0e3,
    ];
    const C: [f64; 8] = [
        1.423_437_110_749_683_577_34,
        4.630_337_846_156_545_295_90,
        5.769_497_221_460_691_405_50,
        3.647_848_324_763_204_605_04,
        1.270_458_252_452_368_382_58,
        2.417_807_251_774_506_117_70e-1,
        2.272_384_498_926_918_458_33e-2,
        7.745_450_142_783_414_076_40e-4,
    ];
    const D: [f64; 8] = [
        1.0,
        2.053_191_626_637_758_821_87,
        1.676_384_830_183_803_849_40,
        6.897_673_349_851_000_045_50e-1,
        1.481_039_764_274_800_745_90e-1,
        1.519_866_656_361_645_719_66e-2,
        5.475_938_084_995_344_946_00e-4,
        1.050_750_071_644_416_843_24e-9,
    ];
    const E: [f64; 8] = [
        6.657_904_643_501_103_777_20,
        5.463_784_911_164_114_369_90,
        1.784_826_539_917_291_335_80,
        2.965_605_718_285_048_912_30e-1,
        2.653_218_952_657_612_309_30e-2,
        1.242_660_947_388_078_438_60e-3,
        2.711_555_568_743_487_578_15e-5,
        2.010_334_399_292_288_132_65e-7,
    ];
    const F: [f64; 8] = [
        1.0,
        5.998_322_065_558_879_376_90e-1,
        1.369_298_809_227_358_053_10e-1,
        1.487_536_129_085_061_485_25e-2,
        7.868_691_311_456_132_591_00e-4,
        1.846_318_317_510_054_681_80e-5,
        1.421_511_758_316_445_888_70e-7,
        2.044_263_103_389_939_785_64e-15,
    ];

    /// Wichura's AS 241, `PPND16`, over the module's own `ln` and `sqrt`.
    pub fn inverse_normal(p: f64) -> f64 {
        let q = p - 0.5;
        if q.abs() <= 0.425 {
            let r = 0.180625 - q * q;
            return q * polynomial(&A, r) / polynomial(&B, r);
        }
        let tail = if q < 0.0 { p } else { 1.0 - p };
        let r = sqrt(-ln(tail));
        let value = if r <= 5.0 {
            let r = r - 1.6;
            polynomial(&C, r) / polynomial(&D, r)
        } else {
            let r = r - 5.0;
            polynomial(&E, r) / polynomial(&F, r)
        };
        if q < 0.0 { -value } else { value }
    }

    pub fn normal(word: u64, mean: f64, standard_deviation: f64) -> f64 {
        mean + standard_deviation * inverse_normal(open_unit(word))
    }
}

/// The reference against the vectors its sources publish. Random123's
/// `kat_vectors` for `threefry2x64` at 13 and 20 rounds, and Vigna's
/// SplitMix64 and xoshiro256** reference outputs. If this fails, nothing else
/// in this file means anything.
#[test]
fn the_reference_reproduces_the_published_vectors() {
    use reference::*;
    // `kat_vectors` lists the counter before the key; `threefry` takes the
    // key first.
    let pi_key = [0xa4093822299f31d0, 0x082efa98ec4e6c89];
    let pi_counter = [0x243f6a8885a308d3, 0x13198a2e03707344];
    let ones = [u64::MAX, u64::MAX];
    assert_eq!(threefry(13, [0, 0], [0, 0]), [0xf167b032c3b480bd, 0xe91f9fee4b7a6fb5]);
    assert_eq!(threefry(13, ones, ones), [0xccdec5c917a874b1, 0x4df53abca26ceb01]);
    assert_eq!(threefry(13, pi_key, pi_counter), [0xc3aac71561042993, 0x3fe7ae8801aff316]);
    assert_eq!(threefry(20, [0, 0], [0, 0]), [0xc2b6e3a8c2c69865, 0x6f81ed42f350084d]);
    assert_eq!(threefry(20, ones, ones), [0xe02cb7c4d95d277a, 0xd06633d0893b8b68]);
    assert_eq!(threefry(20, pi_key, pi_counter), [0x263c7d30bb0f0af1, 0x56be8361d3311526]);
    let mut state = 0u64;
    assert_eq!(splitmix64(&mut state), 0xe220a8397b1dcdaf);
    let mut x = Xoshiro([1, 2, 3, 4]);
    let first: Vec<u64> = (0..4).map(|_| x.next_word()).collect();
    assert_eq!(first, [11520, 0, 1509978240, 1215971899390074240]);
}

/// The module's `ln`, `sqrt` and inverse CDF are not libm's, and they are
/// accurate anyway: a few units in the last place against the standard
/// library, and Φ⁻¹ against quantiles known to sixteen digits.
#[test]
fn the_reference_special_functions_are_accurate() {
    use reference::*;
    for &x in &[1e-300, 1e-17, 1.1102230246251565e-16, 0.001, 0.025, 0.3, 0.5, 0.7071, 0.99, 1.0, 2.5, 745.0] {
        let ours = ln(x);
        let std = x.ln();
        assert!((ours - std).abs() <= 4.0 * f64::EPSILON * std.abs().max(1e-300), "ln({x}): {ours} vs {std}");
    }
    for &x in &[1e-10, 0.5, 2.0, 2.6, 37.0, 745.0, 1e10] {
        let ours = sqrt(x);
        assert!((ours - x.sqrt()).abs() <= f64::EPSILON * x.sqrt(), "sqrt({x}): {ours}");
    }
    // √2 · erfinv(2p − 1) at thirty digits (mpmath), rounded to `f64`.
    for &(p, z) in &[
        (0.5, 0.0),
        (0.975, 1.959963984540054),
        (0.025, -1.959963984540054),
        (0.001, -3.0902323061678135),
        (1e-10, -6.361340902404056),
        (1.1102230246251565e-16, -8.209536151601387),
    ] {
        let ours = inverse_normal(p);
        assert!((ours - z).abs() <= 1e-13 * z.abs().max(1.0), "Φ⁻¹({p}): {ours} vs {z}");
    }
}

/// The lines of a program's output, as `u64`s.
fn words(output: &str) -> Vec<u64> {
    output.lines().map(|line| line.parse().unwrap_or_else(|_| panic!("not a U64: {line:?}"))).collect()
}

/// Shortest round-trip decimal parses back to the exact `F64` it printed, so
/// comparing the parsed bits is comparing the program's bits.
fn float_bits(line: &str) -> u64 {
    line.parse::<f64>().unwrap_or_else(|_| panic!("not an F64: {line:?}")).to_bits()
}

/// `Key.from_seed`, `split`, `split_many`, `fold_in` and `bits`: the key
/// layout and the derivations §5.5 freezes, word for word.
#[test]
fn a_key_derives_and_draws_exactly_the_reference_words() {
    use reference::*;
    let out = prints(
        "key_words",
        "use random (Key, bits)

def main():
    print(bits(Key.from_seed(0)))
    print(bits(Key.from_seed(42)))
    print(bits(Key.from_seed(0xFFFFFFFFFFFFFFFF)))
    let a, b be Key.from_seed(42).split()
    print(bits(a))
    print(bits(b))
    let mutable many be Key.from_seed(42).split_many(3)
    loop:
        let next be many.pop()
        if not next?:
            break
        print(bits(next))
    print(bits(Key.from_seed(42).fold_in(7)))
",
    );
    let seeded = |seed| word(from_seed(seed), 0);
    let children = split_many(from_seed(42), 3);
    let expected = vec![
        seeded(0),
        seeded(42),
        seeded(u64::MAX),
        word(children[0], 0),
        word(children[1], 0),
        word(children[2], 0),
        word(children[1], 0),
        word(children[0], 0),
        word(fold_in(from_seed(42), 7), 0),
    ];
    assert_eq!(words(&out), expected);
}

/// `uniform`, `integer` and `normal` on keys, compared as bits.
#[test]
fn a_key_maps_its_words_exactly_as_the_reference_does() {
    use reference::*;
    let out = prints(
        "key_maps",
        "use random (Key, uniform, integer, normal)

def main():
    for seed in 0..8:
        print(uniform(Key.from_seed(seed as U64)))
    for seed in 0..8:
        print(integer(Key.from_seed(seed as U64), -3, 4))
    for seed in 0..8:
        print(normal(Key.from_seed(seed as U64), 10.0, 2.5))
",
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 24, "{out}");
    for seed in 0..8u64 {
        let key = from_seed(seed);
        let i = seed as usize;
        assert_eq!(float_bits(lines[i]), unit(word(key, 0)).to_bits(), "uniform, seed {seed}");
        assert_eq!(
            lines[8 + i].parse::<i64>().unwrap(),
            integer(&mut key_words(key), -3, 4),
            "integer, seed {seed}"
        );
        assert_eq!(float_bits(lines[16 + i]), normal(word(key, 0), 10.0, 2.5).to_bits(), "normal, seed {seed}");
    }
}

/// The tails of `normal`, where AS 241 leaves its central polynomial and the
/// module's own `ln` and `sqrt` do the work. Two thousand keys reach the
/// `r ≤ 5` tail branch about three hundred times. The `r > 5` branch needs a
/// uniform below about `10⁻¹¹`, which no key a test can enumerate draws: it
/// is checked in the reference only (`the_reference_special_functions_are_
/// accurate`), and not here.
#[test]
fn normal_matches_the_reference_in_both_tails() {
    use reference::*;
    let out = prints(
        "normal_tails",
        "use random (Key, normal)

def main():
    for seed in 0..2000:
        print(normal(Key.from_seed(seed as U64), 0.0, 1.0))
",
    );
    let mut tails = 0;
    for (seed, line) in out.lines().enumerate() {
        let w = word(from_seed(seed as u64), 0);
        assert_eq!(float_bits(line), normal(w, 0.0, 1.0).to_bits(), "seed {seed}");
        if (open_unit(w) - 0.5).abs() > 0.425 {
            tails += 1;
        }
    }
    assert!(tails > 100, "only {tails} draws reached the tail branches");
}

/// The `Stream` face: SplitMix64 seeding, xoshiro256** steps, and the same
/// mappings, in call order.
#[test]
fn a_stream_draws_exactly_the_reference_sequence() {
    use reference::*;
    let out = prints(
        "stream",
        "use random (Stream, bits_from, uniform_from, integer_from, normal_from)

def main():
    let mutable stream be Stream.from_seed(12345)
    for step in 0..4:
        print(bits_from(&mut stream))
    print(uniform_from(&mut stream))
    print(integer_from(&mut stream, 0, 1000))
    print(normal_from(&mut stream, 0.0, 1.0))
    print(bits_from(&mut stream))
",
    );
    let lines: Vec<&str> = out.lines().collect();
    let mut x = Xoshiro::from_seed(12345);
    for line in &lines[..4] {
        assert_eq!(line.parse::<u64>().unwrap(), x.next_word());
    }
    assert_eq!(float_bits(lines[4]), unit(x.next_word()).to_bits());
    assert_eq!(lines[5].parse::<i64>().unwrap(), integer(&mut x, 0, 1000));
    assert_eq!(float_bits(lines[6]), normal(x.next_word(), 0.0, 1.0).to_bits());
    assert_eq!(lines[7].parse::<u64>().unwrap(), x.next_word());
}

/// **No modulo bias, shown where it would be large.** `[Int.min, 2⁶²)` spans
/// `3·2⁶²`, so `2⁶⁴ mod span` is `2⁶²`: a quarter of all words are rejected.
/// Reducing them instead would land the bottom third of the range twice as
/// often as the rest. The program must agree with the reference, which
/// rejects, on every draw — a reduction diverges at the first rejected word.
#[test]
fn an_integer_rejects_rather_than_reducing() {
    use reference::*;
    let out = prints(
        "rejection",
        "use random (Stream, integer_from)

def main():
    let mutable stream be Stream.from_seed(7)
    for step in 0..200:
        print(integer_from(&mut stream, -9223372036854775807 - 1, 4611686018427387904))
",
    );
    let mut x = Xoshiro::from_seed(7);
    for line in out.lines() {
        let value: i64 = line.parse().unwrap();
        assert!(value < 1 << 62, "{value} is out of range");
        assert_eq!(value, integer(&mut x, i64::MIN, 1 << 62));
    }
    let mut probe = Xoshiro::from_seed(7);
    let rejected = (0..200).filter(|_| probe.next_word() < 1 << 62).count();
    assert!(rejected > 10, "the seed must exercise the rejection, and {rejected} words did");
}

/// Bounds, from many draws: `uniform` stays in `[0, 1)`, `integer` in its
/// half-open range, and the six faces of a die come out near-equally.
#[test]
fn draws_stay_in_their_bounds() {
    let out = prints(
        "bounds",
        "use random (Stream, uniform_from, integer_from)

def main():
    let mutable stream be Stream.from_seed(2026)
    let mutable inside be 0
    for step in 0..100000:
        let u be uniform_from(&mut stream)
        if u >= 0.0 and u < 1.0:
            inside be inside + 1
    print(inside)
    let mutable counts be Array[Int].new()
    for face in 0..6:
        counts.push(0)
    for step in 0..60000:
        let face be integer_from(&mut stream, 1, 7)
        assert(face >= 1 and face <= 6)
        counts[face - 1] be counts[face - 1] + 1
    for count in counts:
        print(count)
",
    );
    let lines: Vec<i64> = out.lines().map(|l| l.parse().unwrap()).collect();
    assert_eq!(lines[0], 100000);
    let counts = &lines[1..];
    assert_eq!(counts.len(), 6);
    assert_eq!(counts.iter().sum::<i64>(), 60000);
    // χ² with five degrees of freedom; 20.5 is the 0.999 quantile.
    let chi: f64 = counts.iter().map(|&c| (c as f64 - 10000.0).powi(2) / 10000.0).sum();
    assert!(chi < 20.5, "χ² = {chi} over {counts:?}");
}

/// `shuffle`, `shuffle_from` and `permutation` produce permutations, and the
/// reference's: Fisher–Yates over the same words.
#[test]
fn a_shuffle_is_the_reference_permutation() {
    use reference::*;
    let out = prints(
        "shuffle",
        "use random (Key, Stream, shuffle, shuffle_from, permutation)

def main():
    for index in permutation(Key.from_seed(5), 20):
        print(index)
    let mutable names be Array[String].new()
    for index in 0..20:
        names.push(f\"{index}\")
    shuffle(Key.from_seed(6), &mut names)
    for name in names:
        print(name)
    let mutable stream be Stream.from_seed(8)
    let mutable values be Array[Int].new()
    for index in 0..20:
        values.push(index)
    shuffle_from(&mut stream, &mut values)
    for value in values:
        print(value)
",
    );
    let got: Vec<usize> = out.lines().map(|l| l.parse().unwrap()).collect();
    assert_eq!(got.len(), 60);
    let identity: Vec<usize> = (0..20).collect();
    let mut a = identity.clone();
    shuffle(&mut key_words(from_seed(5)), &mut a);
    let mut b = identity.clone();
    shuffle(&mut key_words(from_seed(6)), &mut b);
    let mut c = identity.clone();
    shuffle(&mut Xoshiro::from_seed(8), &mut c);
    for (chunk, expected) in got.chunks(20).zip([a, b, c]) {
        let mut sorted = chunk.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, identity, "not a permutation: {chunk:?}");
        assert_eq!(chunk, &expected[..]);
        assert_ne!(chunk, &identity[..], "twenty items left in place");
    }
}

/// `Stream.from_entropy()` runs, and two of them do not agree — which a
/// constant seed behind the name would.
#[test]
fn an_entropy_stream_is_seeded_from_outside_the_program() {
    let out = prints(
        "entropy",
        "use random (Stream, bits_from)

def main():
    let mutable first be Stream.from_entropy()
    let mutable second be Stream.from_entropy()
    print(bits_from(&mut first) is not bits_from(&mut second))
",
    );
    assert_eq!(out, "true\n");
}

/// `wrapping_add`, `wrapping_sub` and `wrapping_mul` are the plain
/// instruction at the receiver's width, which `random` is written over.
#[test]
fn the_wrapping_operations_wrap_at_their_width() {
    assert_eq!(
        prints(
            "wrapping",
            "def main():
    let top: U64 be 0xFFFFFFFFFFFFFFFF
    print(top.wrapping_add(2))
    print(top.wrapping_mul(3))
    print((0 as U64).wrapping_sub(1))
    let byte be 200 as U8
    print(byte.wrapping_add(100 as U8))
    print(byte.wrapping_mul(2 as U8))
    let big: Int be 9223372036854775807
    print(big.wrapping_add(1))
    let small: I32 be -2147483648
    print(small.wrapping_sub(1 as I32))
",
        ),
        "1\n18446744073709551613\n18446744073709551615\n44\n144\n-9223372036854775808\n2147483647\n"
    );
}
