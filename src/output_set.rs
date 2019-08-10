use std::{cmp::min, fmt};

pub const MAX_CHANNELS: usize = 15;

pub type CVec<T> = arrayvec::ArrayVec<[T; MAX_CHANNELS]>;

const MAX_VALUES: usize = 1 << MAX_CHANNELS;

type VVec<T> = arrayvec::ArrayVec<[T; MAX_VALUES]>;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct OutputSet {
    channels: usize,
    values: Vec<u16>,
}

impl fmt::Debug for OutputSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        struct WrapValue(usize, u16);
        impl fmt::Debug for WrapValue {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "0b{0:01$b}", self.1, self.0)
            }
        }

        struct WrapValues<'a>(usize, &'a Vec<u16>);

        impl<'a> fmt::Debug for WrapValues<'a> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.debug_list()
                    .entries(self.1.iter().map(|&v| WrapValue(self.0, v)))
                    .finish()
            }
        }

        f.debug_struct("OutputSet")
            .field("channels", &self.channels)
            .field("values", &WrapValues(self.channels, &self.values))
            .finish()
    }
}

impl OutputSet {
    pub fn all_values(channels: usize) -> Self {
        assert!(channels <= MAX_CHANNELS);
        Self {
            channels,
            values: (0..1 << channels).collect(),
        }
    }

    pub fn apply_comparator(&self, a: usize, b: usize) -> Self {
        assert_ne!(a, b);
        assert!(a < self.channels && b < self.channels);

        let mask_a = 1 << a;
        let mask_b = 1 << b;

        let mask = mask_a | mask_b;

        let mut keep = VVec::<u16>::new();
        let mut swap = VVec::<u16>::new();

        for &value in self.values.iter() {
            if value & mask == mask_b {
                swap.push(value ^ mask);
            } else {
                keep.push(value);
            }
        }

        swap.push(!0);
        keep.push(!0);

        let mut values = Vec::with_capacity(self.values.len());

        let mut swap_pos = 0;
        let mut keep_pos = 0;

        loop {
            let swap_value = swap[swap_pos];
            let keep_value = keep[keep_pos];
            let value = min(swap_value, keep_value);
            if value == !0 {
                break;
            }
            values.push(value);

            swap_pos += (swap_value == value) as usize;
            keep_pos += (keep_value == value) as usize;
        }

        Self {
            channels: self.channels,
            values,
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn values(&self) -> &[u16] {
        &self.values
    }

    pub fn is_sorted(&self) -> bool {
        self.values.iter().all(|&value| value & (value + 1) == 0)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[rustfmt::skip]
    static SORT_11: &[(usize, usize)] = &[
        (0, 9), (1, 6), (2, 4), (3, 7), (5, 8),
        (0, 1), (3, 5), (4, 10), (6, 9), (7, 8),
        (1, 3), (2, 5), (4, 7), (8, 10),
        (0, 4), (1, 2), (3, 7), (5, 9), (6, 8),
        (0, 1), (2, 6), (4, 5), (7, 8), (9, 10),
        (2, 4), (3, 6), (5, 7), (8, 9),
        (1, 2), (3, 4), (5, 6), (7, 8),
        (2, 3), (4, 5), (6, 7),
    ];

    #[test]
    fn sort_11_sorts() {
        crate::logging::setup();

        let mut output_set = OutputSet::all_values(11);

        for (i, &(a, b)) in SORT_11.iter().enumerate() {
            assert!(!output_set.is_sorted());
            output_set = output_set.apply_comparator(a, b);
            log::info!("step {}: size = {}", i, output_set.values().len());
        }

        log::info!("result: {:?}", output_set);

        assert!(output_set.is_sorted());
        assert_eq!(output_set.values().len(), 12);
    }
}
