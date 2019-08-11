use std::mem::swap;

use crate::output_set::CVec;

#[derive(Clone)]
pub struct Matching {
    matches_a: CVec<u16>,
    matches_b: CVec<u16>,
    incomplete: bool,
}

impl Matching {
    pub fn new(channels: usize) -> Matching {
        let all = (1u16 << channels) - 1;
        Matching {
            matches_a: (0..channels).map(|_| all).collect(),
            matches_b: (0..channels).map(|_| all).collect(),
            incomplete: false,
        }
    }

    pub fn contains(&self, channel_a: usize, channel_b: usize) -> bool {
        self.matches_a[channel_a] & (1 << channel_b) != 0
    }

    pub fn remove(&mut self, channel_a: usize, channel_b: usize) -> bool {
        if self.incomplete {
            return true;
        }

        let col_a = 1 << channel_b;
        let mut row_a = self.matches_a[channel_a];
        if row_a & col_a == 0 {
            return false;
        }
        row_a &= !col_a;
        self.matches_a[channel_a] = row_a;

        let col_b = 1 << channel_a;
        let mut row_b = self.matches_b[channel_b];
        row_b &= !col_b;
        self.matches_b[channel_b] = row_b;

        if row_a == 0 {
            self.incomplete = true;
            return true;
        }

        if row_b == 0 {
            self.incomplete = true;
            return true;
        }

        if row_a.is_power_of_two() {
            let target = row_a.trailing_zeros() as usize;

            for other_channel_a in 0..self.matches_a.len() {
                if other_channel_a != channel_a {
                    if self.remove(other_channel_a, target) {
                        return true;
                    }
                }
            }
        }

        if row_b.is_power_of_two() {
            let target = row_b.trailing_zeros() as usize;

            for other_channel_b in 0..self.matches_b.len() {
                if other_channel_b != channel_b {
                    if self.remove(target, other_channel_b) {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn select(&mut self, channel_a: usize, channel_b: usize) -> bool {
        if self.incomplete {
            return true;
        }

        if !self.contains(channel_a, channel_b) {
            self.incomplete = true;
            return true;
        }

        for other_channel_lo in 0..self.matches_a.len() {
            if other_channel_lo != channel_a {
                if self.remove(other_channel_lo, channel_b) {
                    return true;
                }
            }
        }

        for other_channel_hi in 0..self.matches_b.len() {
            if other_channel_hi != channel_b {
                if self.remove(channel_a, other_channel_hi) {
                    return true;
                }
            }
        }

        false
    }

    pub fn swap_channels_a(&mut self, channel_a_0: usize, channel_a_1: usize) {
        self.matches_a.swap(channel_a_0, channel_a_1);

        let col_b_0 = 1u16 << channel_a_0;
        let col_b_1 = 1u16 << channel_a_1;

        let col_b_both = col_b_0 | col_b_1;

        for row_b in self.matches_b.iter_mut() {
            let exchange = *row_b & col_b_both;
            let flip = (exchange == col_b_0) | (exchange == col_b_1);
            *row_b ^= col_b_both * (flip as u16);
        }
    }

    pub fn swap_channels_b(&mut self, channel_b_0: usize, channel_b_1: usize) {
        swap(&mut self.matches_a, &mut self.matches_b);
        self.swap_channels_a(channel_b_0, channel_b_1);
        swap(&mut self.matches_a, &mut self.matches_b);
    }

    pub fn filter(&mut self, mut pred: impl FnMut(usize, usize) -> bool) -> bool {
        if self.incomplete {
            return true;
        }

        for a in 0..self.matches_a.len() {
            for b in 0..self.matches_b.len() {
                if self.contains(a, b) {
                    if !pred(a, b) {
                        if self.remove(a, b) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    pub fn unique_match_a(&self, channel_a: usize) -> Option<usize> {
        if self.incomplete {
            return None;
        }

        let row_a = self.matches_a[channel_a];

        if row_a.is_power_of_two() {
            Some(row_a.trailing_zeros() as usize)
        } else {
            None
        }
    }

    pub fn matches_a(&self, channel_a: usize) -> u16 {
        self.matches_a[channel_a]
    }

    pub fn matches_b(&self, channel_b: usize) -> u16 {
        self.matches_b[channel_b]
    }
}
