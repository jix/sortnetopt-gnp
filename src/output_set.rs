use std::{
    cmp::{max, min},
    fmt,
};

pub const MAX_CHANNELS: usize = 15;

pub type CVec<T> = arrayvec::ArrayVec<[T; MAX_CHANNELS]>;

const MAX_VALUES: usize = 1 << MAX_CHANNELS;

type VVec<T> = arrayvec::ArrayVec<[T; MAX_VALUES]>;

type AVec<T> = arrayvec::ArrayVec<[T; 512]>; // >= MAX_CHANNELS * MAX_CHANNELS * 2

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

    pub fn abstraction(&self) -> Abstraction {
        let mut values = (0..self.channels * self.channels * 2)
            .map(|_| 0)
            .collect::<AVec<u16>>();

        for &value in self.values.iter() {
            let pop_count = value.count_ones() as usize;
            for channel in 0..self.channels {
                let mask = 1 << channel;
                let channel_value = (value & mask != 0) as usize;

                let channel_pop_count = pop_count - channel_value;

                values[2 * (self.channels * channel + channel_pop_count) + channel_value] += 1;
            }
        }

        Abstraction {
            channels: self.channels,
            values,
        }
    }

    pub fn channel_weights(&self) -> CVec<u16> {
        let mut weights = (0..self.channels).map(|_| 0).collect::<CVec<u16>>();

        for &value in self.values.iter() {
            for channel in 0..self.channels {
                let mask = 1 << channel;
                let channel_value = value & mask != 0;
                weights[channel] += channel_value as u16;
            }
        }

        weights
    }

    pub fn permute_channels(&mut self, perm: CVec<usize>) {
        assert_eq!(perm.len(), self.channels);

        let perm_masks = perm
            .into_iter()
            .rev()
            .map(|j| 1u16 << j)
            .collect::<CVec<_>>();

        let mut mask_combined = 0;

        for &mask_from in perm_masks.iter() {
            mask_combined |= mask_from;
        }

        assert_eq!(mask_combined + 1, 1 << self.channels);

        for value in self.values.iter_mut() {
            let mut value_out = 0;
            for mask_from in perm_masks.iter() {
                value_out <<= 1;
                value_out |= (*value & mask_from != 0) as u16;
            }
            *value = value_out;
        }

        self.values.sort_unstable();
    }

    pub fn order_channels_by_weight(&mut self) -> CVec<usize> {
        let mut weights = self
            .channel_weights()
            .into_iter()
            .enumerate()
            .map(|(i, w)| (!w, i))
            .collect::<CVec<_>>();

        weights.sort_unstable();

        let perm = weights.into_iter().map(|(_, i)| i).collect::<CVec<_>>();

        self.permute_channels(perm.clone());

        perm
    }

    pub fn swap_channels(&mut self, a: usize, b: usize) {
        assert!(a < self.channels && b < self.channels);
        if a == b {
            return;
        }

        let mask_a = 1 << a;
        let mask_b = 1 << b;

        let mask = mask_a | mask_b;

        let mut keep = VVec::<u16>::new();
        let mut swap_a = VVec::<u16>::new();
        let mut swap_b = VVec::<u16>::new();

        for &value in self.values.iter() {
            if value & mask == mask_a {
                swap_a.push(value ^ mask);
            } else if value & mask == mask_b {
                swap_b.push(value ^ mask);
            } else {
                keep.push(value);
            }
        }

        swap_a.push(!0);
        swap_b.push(!0);
        keep.push(!0);

        self.values.clear();

        let mut swap_a_pos = 0;
        let mut swap_b_pos = 0;
        let mut keep_pos = 0;

        loop {
            let swap_a_value = swap_a[swap_a_pos];
            let swap_b_value = swap_b[swap_b_pos];
            let keep_value = keep[keep_pos];
            let value = min(min(swap_a_value, swap_b_value), keep_value);
            if value == !0 {
                break;
            }
            self.values.push(value);

            swap_a_pos += (swap_a_value == value) as usize;
            swap_b_pos += (swap_b_value == value) as usize;
            keep_pos += (keep_value == value) as usize;
        }
    }

    pub fn subsumes(&self, other: &OutputSet) -> bool {
        if other.values.len() < self.values.len() {
            return false;
        }

        let mut slack = other.values.len() - self.values.len();

        let mut other_pos = 0;

        for &value in self.values.iter() {
            loop {
                if let Some(&other_value) = other.values.get(other_pos) {
                    if other_value == value {
                        other_pos += 1;
                        break;
                    } else if other_value > value {
                        return false;
                    } else {
                        if slack == 0 {
                            return false;
                        }
                        slack -= 1;
                        other_pos += 1;
                    }
                } else {
                    return false;
                }
            }
        }

        true
    }
}

#[derive(Clone, Debug)]
pub struct Abstraction {
    channels: usize,
    values: AVec<u16>,
}

impl Abstraction {
    pub fn update_min(&mut self, other: &Abstraction) {
        assert_eq!(self.channels, other.channels);

        for (my, other) in self.values.iter_mut().zip(other.values.iter()) {
            *my = min(*my, *other);
        }
    }

    pub fn update_max(&mut self, other: &Abstraction) {
        assert_eq!(self.channels, other.channels);

        for (my, other) in self.values.iter_mut().zip(other.values.iter()) {
            *my = max(*my, *other);
        }
    }

    pub fn values(&self) -> &[u16] {
        &self.values
    }

    pub fn largest_range(&self, other: &Abstraction) -> Option<usize> {
        self.values
            .iter()
            .zip(other.values.iter())
            .map(|(&a, &b)| max(a, b) - min(a, b))
            .enumerate()
            .max_by_key(|&(_, range)| range)
            .filter(|&(_, range)| range > 0)
            .map(|(index, _)| index)
    }

    pub fn channel_le(&self, my_channel: usize, other: &Abstraction, other_channel: usize) -> bool {
        assert_eq!(self.channels, other.channels);

        let channel_values_len = self.channels * 2;

        let my_offset = channel_values_len * my_channel;
        let other_offset = channel_values_len * other_channel;

        let my_channel_values = &self.values[my_offset..my_offset + channel_values_len];
        let other_channel_values = &other.values[other_offset..other_offset + channel_values_len];

        my_channel_values
            .iter()
            .zip(other_channel_values.iter())
            .all(|(my, other)| my <= other)
    }

    pub fn swap_channels(&mut self, a: usize, b: usize) {
        if a == b {
            return;
        }

        let channel_values_len = self.channels * 2;
        let a_offset = channel_values_len * a;
        let b_offset = channel_values_len * b;

        for i in 0..channel_values_len {
            self.values.swap(a_offset + i, b_offset + i);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn avec_capacity() {
        assert!(AVec::<usize>::new().capacity() >= MAX_CHANNELS * MAX_CHANNELS * 2);
    }

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

    #[test]
    fn sort_11_order_channels() {
        crate::logging::setup();

        let mut output_set = OutputSet::all_values(11);

        for (i, &(a, b)) in SORT_11.iter().enumerate() {
            assert!(!output_set.is_sorted());

            let mut ordered_output_set = output_set.clone();

            let channel_weights = output_set.channel_weights();

            let perm = ordered_output_set.order_channels_by_weight();

            log::info!("perm: {:?}", perm);

            let ordered_channel_weights = ordered_output_set.channel_weights();

            log::info!("weights: {:?}", ordered_channel_weights);

            assert!(ordered_channel_weights
                .iter()
                .zip(ordered_channel_weights.iter().skip(1))
                .all(|(a, b)| a >= b));

            assert!(perm
                .iter()
                .map(|&i| channel_weights[i])
                .zip(ordered_channel_weights.iter().cloned())
                .all(|(a, b)| a == b));

            let mut inv_perm = perm.clone();

            for (from, &to) in perm.iter().enumerate() {
                inv_perm[to] = from;
            }

            ordered_output_set.permute_channels(inv_perm);

            assert_eq!(output_set, ordered_output_set);

            output_set = output_set.apply_comparator(a, b);
            log::info!("step {}: size = {}", i, output_set.values().len());
        }

        log::info!("result: {:?}", output_set);
    }
}
