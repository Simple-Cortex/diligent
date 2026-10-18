//! The seeded generator: reference vectors, bounded draws and sampling without replacement.

use std::collections::HashSet;

use diligent_inventory::prng::{sample_without_replacement, seed_state, XorShift64Star, PRNG_ID};

#[test]
fn algorithm_id_is_stable() {
    assert_eq!(PRNG_ID, "diligent-xorshift64star/1");
}

#[test]
fn reference_vectors_from_state_one() {
    let mut rng = XorShift64Star::from_state(1);
    assert_eq!(rng.next_u64(), 5_180_492_295_206_395_165);
    assert_eq!(rng.next_u64(), 12_380_297_144_915_551_517);
    assert_eq!(rng.next_u64(), 13_389_498_078_930_870_103);
}

#[test]
fn reference_vectors_from_seed_strings() {
    assert_eq!(seed_state("diligent"), 0xcf08_fccd_9d8b_fd57);
    let mut rng = XorShift64Star::from_seed("diligent");
    assert_eq!(rng.next_u64(), 11_815_162_611_370_927_224);
    assert_eq!(rng.next_u64(), 3_896_702_070_645_662_550);
    assert_eq!(rng.next_u64(), 15_025_632_453_180_051_863);

    assert_eq!(seed_state("run-2026-10-18-abcdef"), 0xe8f7_2f8f_3904_9168);
    let mut rng = XorShift64Star::from_seed("run-2026-10-18-abcdef");
    assert_eq!(rng.next_u64(), 13_982_366_010_765_060_781);
    assert_eq!(rng.next_u64(), 2_154_164_353_909_940_071);
    assert_eq!(rng.next_u64(), 14_987_882_293_175_992_688);
}

#[test]
fn zero_state_does_not_stick_at_zero() {
    let mut rng = XorShift64Star::from_state(0);
    assert_ne!(rng.next_u64(), 0);
    assert_ne!(rng.next_u64(), 0);
}

#[test]
fn below_stays_in_range_and_covers_it() {
    let mut rng = XorShift64Star::from_seed("range");
    let mut seen = HashSet::new();
    for _ in 0..2_000 {
        let v = rng.below(7);
        assert!(v < 7);
        seen.insert(v);
    }
    assert_eq!(seen.len(), 7);
    assert_eq!(rng.below(1), 0);
}

#[test]
fn sampling_is_distinct_deterministic_and_seed_dependent() {
    let items: Vec<u32> = (0..100).collect();
    let a = sample_without_replacement(&items, 10, &mut XorShift64Star::from_seed("alpha"));
    let a2 = sample_without_replacement(&items, 10, &mut XorShift64Star::from_seed("alpha"));
    let b = sample_without_replacement(&items, 10, &mut XorShift64Star::from_seed("beta"));
    assert_eq!(a, a2);
    assert_ne!(a, b);
    assert_eq!(a.iter().collect::<HashSet<_>>().len(), 10);
    assert!(a.iter().all(|x| *x < 100));
}

#[test]
fn sampling_more_than_available_returns_everything_once() {
    let items = [1, 2, 3];
    let mut got = sample_without_replacement(&items, 10, &mut XorShift64Star::from_seed("s"));
    got.sort_unstable();
    assert_eq!(got, vec![1, 2, 3]);
    assert!(sample_without_replacement(&items, 0, &mut XorShift64Star::from_seed("s")).is_empty());
    assert!(
        sample_without_replacement::<u8>(&[], 5, &mut XorShift64Star::from_seed("s")).is_empty()
    );
}

#[test]
fn seed_collisions_need_different_first_eight_bytes() {
    // Different seed strings give different states (SHA-256 prefix); the empty string is a valid seed.
    let states: HashSet<u64> = ["", "a", "b", "A", "a ", "run-2026-10-18-000000"]
        .iter()
        .map(|s| seed_state(s))
        .collect();
    assert_eq!(states.len(), 6);
}
