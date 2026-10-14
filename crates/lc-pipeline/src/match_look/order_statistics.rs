//! Exact luminance ranks used by Match Look. Partition only the requested ranks;
//! the spatial/statistical reductions continue to use the untouched Lab raster.

const QUANTILES: [f64; 7] = [0.01, 0.05, 0.25, 0.5, 0.75, 0.95, 0.99];

pub(super) fn luminance(values: &mut [f64]) -> [f64; 7] {
    let mut selected = [0.0; 7];
    if !values.is_empty() {
        // Keep the original floating multiply and truncation, including repeated
        // indices for small rasters. No percentile interpolation is introduced.
        let ranks = QUANTILES.map(|q| ((values.len() - 1) as f64 * q) as usize);
        // The old sort detects monotonic runs. Preserve that linear fast path,
        // including equal-valued buffers, under exactly the same total order.
        if values.is_sorted_by(|a, b| a.total_cmp(b).is_le()) {
            return ranks.map(|rank| values[rank]);
        }
        if values.is_sorted_by(|a, b| a.total_cmp(b).is_ge()) {
            return ranks.map(|rank| values[values.len() - 1 - rank]);
        }
        partition(values, 0, &ranks, &mut selected);
    }
    selected
}

fn partition(values: &mut [f64], offset: usize, ranks: &[usize], selected: &mut [f64]) {
    if ranks.is_empty() {
        return;
    }
    // Seven requested ranks give at most three partition levels. Select all
    // copies of a repeated rank together, so neither child can include the pivot.
    let rank = ranks[ranks.len() / 2];
    let (left, pivot, right) = values.select_nth_unstable_by(rank - offset, f64::total_cmp);
    let first = ranks.partition_point(|requested| *requested < rank);
    let after = ranks.partition_point(|requested| *requested <= rank);
    selected[first..after].fill(*pivot);
    let (lower, rest) = selected.split_at_mut(first);
    let (_, upper) = rest.split_at_mut(after - first);
    partition(left, offset, &ranks[..first], lower);
    partition(right, rank + 1, &ranks[after..], upper);
}

#[cfg(test)]
mod tests {
    use super::super::{Look, appearance, measure_samples, measure_samples_with_quantiles};
    use super::*;

    // Deliberately retain the old full stable sort and its original rank formula
    // as the oracle, independently of the partition helper's implementation.
    fn old_sort(values: &mut [f64]) -> [f64; 7] {
        values.sort_by(f64::total_cmp);
        if values.is_empty() { [0.0; 7] } else { [0.01, 0.05, 0.25, 0.5, 0.75, 0.95, 0.99].map(|q| values[((values.len() - 1) as f64 * q) as usize]) }
    }
    fn assert_ranks(values: &[f64]) {
        let expected = old_sort(&mut values.to_vec()).map(f64::to_bits);
        let actual = luminance(&mut values.to_vec()).map(f64::to_bits);
        assert_eq!(actual, expected, "rank mismatch for {} samples", values.len());
    }
    fn random_words(len: usize) -> Vec<u64> {
        let mut state = 0x3828_c27d_5be4_7a91u64;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state
            })
            .collect()
    }
    #[test]
    fn exact_ranks_match_old_sort_for_ties_total_order_and_raster_sizes() {
        let special = [
            f64::NEG_INFINITY,
            -f64::MAX,
            -1.0,
            -f64::MIN_POSITIVE,
            f64::from_bits(0x8000_0000_0000_0001),
            -0.0,
            0.0,
            f64::from_bits(1),
            f64::MIN_POSITIVE,
            1.0,
            f64::MAX,
            f64::INFINITY,
            f64::from_bits(0x7ff0_0000_0000_0001),
            f64::from_bits(0x7ff8_0000_0000_0001),
            f64::from_bits(0x7ff8_0000_0000_0123),
            f64::from_bits(0xfff0_0000_0000_0001),
            f64::from_bits(0xfff8_0000_0000_0001),
            f64::from_bits(0xfff8_0000_0000_0123),
        ];
        for len in (0..130).chain([257, 3600, 65536, 262144]) {
            assert_ranks(&vec![0.18; len]);
            assert_ranks(&(0..len).map(|i| special[i % special.len()]).collect::<Vec<_>>());
            let mut words = random_words(len).into_iter().map(f64::from_bits).collect::<Vec<_>>();
            assert_ranks(&words);
            words.sort_by(f64::total_cmp);
            assert_ranks(&words);
            words.reverse();
            assert_ranks(&words);
        }
    }
    fn look_bits(look: &Look) -> Vec<u64> {
        look.features
            .iter()
            .chain(&look.support)
            .chain(&look.signature)
            .chain(&look.hue_distribution)
            .chain(&look.band_mass)
            .chain(&look.content)
            .chain(std::iter::once(&look.importance))
            .copied()
            .chain(look.clusters.iter().flat_map(|cluster| [cluster.ab[0], cluster.ab[1], cluster.weight]))
            .map(f64::to_bits)
            .collect()
    }
    #[test]
    fn every_look_bit_matches_old_sort_for_spatial_color_and_nonfinite_distributions() {
        let cases = [(0, 0), (1, 1), (1, 97), (13, 7), (64, 48), (256, 192)];
        for (width, height) in cases {
            for distribution in 0..5 {
                let labs = random_words(width * height)
                    .iter()
                    .enumerate()
                    .map(|(i, word)| {
                        let l = match distribution {
                            0 => 0.18,
                            1 => ((i % width.max(1)) as f64 / width.max(1) as f64) * 0.85 + 0.05,
                            2 => {
                                if i % 4 == 0 {
                                    0.0
                                } else if i % 4 == 1 {
                                    1.0
                                } else {
                                    (word % 1000) as f64 / 1000.0
                                }
                            }
                            3 => [-0.0, 0.0, f64::from_bits(1), f64::from_bits(0x8000_0000_0000_0001)][i % 4],
                            _ => {
                                [f64::NEG_INFINITY, 0.2, f64::INFINITY, f64::from_bits(0x7ff8_0000_0000_0123), f64::from_bits(0xfff0_0000_0000_0001)]
                                    [i % 5]
                            }
                        };
                        [l, ((*word >> 12) % 17) as f64 * 0.006 - 0.03, ((*word >> 29) % 19) as f64 * 0.005 - 0.02]
                    })
                    .collect::<Vec<_>>();
                let membership = appearance::memberships_with_dimensions(&labs, width, height);
                let disabled = appearance::disabled();
                let faces = [[0.1, 0.15, 0.6, 0.65]];
                for descriptor in [false, true] {
                    for membership in [None, Some(&membership), Some(&disabled)] {
                        let actual = measure_samples(width, height, labs.iter().copied(), &faces, descriptor, membership);
                        let expected = measure_samples_with_quantiles(width, height, labs.iter().copied(), &faces, descriptor, membership, old_sort);
                        assert_eq!(actual.content.len(), expected.content.len());
                        assert_eq!(actual.clusters.len(), expected.clusters.len());
                        assert_eq!(actual.monochrome, expected.monochrome);
                        assert_eq!(
                            look_bits(&actual),
                            look_bits(&expected),
                            "Look changed at {width}x{height}, distribution={distribution}, descriptor={descriptor}"
                        );
                        // Independently preserve the original feature-to-percentile mapping.
                        if !labs.is_empty() {
                            let mut l = labs.iter().map(|lab| lab[0]).collect::<Vec<_>>();
                            l.sort_by(f64::total_cmp);
                            for (feature, q) in [(0, 0.05), (1, 0.25), (2, 0.5), (3, 0.75), (4, 0.95), (30, 0.01), (31, 0.99)] {
                                assert_eq!(actual.features[feature].to_bits(), l[((l.len() - 1) as f64 * q) as usize].to_bits());
                            }
                        }
                    }
                }
            }
        }
    }
}
