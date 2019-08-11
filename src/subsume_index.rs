use arrayvec::ArrayVec;

use crate::{
    matching::Matching,
    output_set::{Abstraction, CVec, OutputSet},
};

pub trait SubsumeIndexItem {
    fn combine(&mut self, perm: CVec<usize>, other: Self);
}

impl SubsumeIndexItem for usize {
    fn combine(&mut self, _perm: CVec<usize>, other: usize) {
        *self += other;
    }
}

impl SubsumeIndexItem for () {
    fn combine(&mut self, _perm: CVec<usize>, _other: ()) {}
}

#[derive(Clone, Debug)]
pub struct AbstractedPair<T> {
    pub abstraction: Abstraction,
    pub output_set: OutputSet,
    pub item: T,
}

pub struct SubsumeIndex<T> {
    trees: Vec<Node<T>>,
    len: usize,
}

impl<T: SubsumeIndexItem> Default for SubsumeIndex<T> {
    fn default() -> Self {
        Self {
            trees: Default::default(),
            len: 0,
        }
    }
}

impl<T: SubsumeIndexItem> SubsumeIndex<T> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn insert(&mut self, pair: AbstractedPair<T>) {
        self.combine_with_subsuming(pair).unwrap_or_else(|pair| {
            self.len += 1;
            self.trees.push(Node::Leaf(pair));
            self.merge_trees(false);
        })
    }

    pub fn subsume_all(&mut self) {
        self.merge_trees(true);
    }

    fn combine_with_subsuming(
        &mut self,
        mut pair: AbstractedPair<T>,
    ) -> Result<(), AbstractedPair<T>> {
        let channels = pair.output_set.channels();
        for tree in self.trees.iter_mut() {
            match tree.combine_with_subsuming(pair, Matching::new(channels)) {
                Ok(()) => return Ok(()),
                Err(returned_pair) => pair = returned_pair,
            }
        }
        Err(pair)
    }

    fn merge_trees(&mut self, all: bool) {
        while self.trees.len() >= 2 {
            let last_trees = &self.trees[self.trees.len() - 2..];
            if !all && last_trees[0].len() > last_trees[1].len() {
                return;
            }

            let mut last_tree = self.trees.pop().unwrap();

            self.len -= last_tree.len();

            let second_last_tree = self.trees.pop().unwrap();

            self.len -= second_last_tree.len();

            let mut pairs = vec![];
            second_last_tree.drain_using(&mut |pair| {
                let channels = pair.output_set.channels();
                match last_tree.combine_with_subsuming(pair, Matching::new(channels)) {
                    Ok(()) => (),
                    Err(returned_pair) => pairs.push(returned_pair),
                }
            });

            last_tree.drain_using(&mut |pair| pairs.push(pair));

            let new_tree = Node::new(pairs);

            self.len += new_tree.len();

            self.trees.push(new_tree);
        }
    }

    pub fn drain_using(self, mut target: impl FnMut(AbstractedPair<T>)) {
        for tree in self.trees {
            tree.drain_using(&mut target)
        }
    }
}

enum Node<T> {
    Leaf(AbstractedPair<T>),
    Inner {
        abstraction: Abstraction,
        children: Box<[Node<T>; 2]>,
        len: usize,
    },
}

impl<T: SubsumeIndexItem> Node<T> {
    fn new(mut items: Vec<AbstractedPair<T>>) -> Self {
        assert!(!items.is_empty());
        let len = items.len();
        if len == 1 {
            Node::Leaf(items.pop().unwrap())
        } else {
            let mut min_abstraction = items[0].abstraction.clone();
            let mut max_abstraction = min_abstraction.clone();

            for pair in items.iter().skip(1) {
                min_abstraction.update_min(&pair.abstraction);
                max_abstraction.update_max(&pair.abstraction);
            }

            let index = min_abstraction.largest_range(&max_abstraction).unwrap_or(0);

            items.sort_unstable_by_key(|pair| pair.abstraction.values()[index]);

            let items_1 = items.drain(len / 2..).collect::<Vec<_>>();
            let items_0 = items;

            let child_0 = Self::new(items_0);
            let child_1 = Self::new(items_1);

            Node::Inner {
                abstraction: min_abstraction,
                children: Box::new([child_0, child_1]),
                len,
            }
        }
    }

    fn len(&self) -> usize {
        match self {
            &Node::Leaf(..) => 1,
            &Node::Inner { len, .. } => len,
        }
    }

    fn abstraction(&self) -> &Abstraction {
        match self {
            Node::Leaf(pair) => &pair.abstraction,
            Node::Inner { abstraction, .. } => abstraction,
        }
    }

    fn drain_using(self, target: &mut impl FnMut(AbstractedPair<T>)) {
        match self {
            Node::Leaf(pair) => target(pair),
            Node::Inner { children, .. } => {
                for child in ArrayVec::from(*children) {
                    child.drain_using(target);
                }
            }
        }
    }

    fn combine_with_subsuming(
        &mut self,
        pair: AbstractedPair<T>,
        mut matching: Matching,
    ) -> Result<(), AbstractedPair<T>> {
        let node_abstraction = self.abstraction();

        if matching.filter(|node_channel, pair_channel| {
            node_abstraction.channel_le(node_channel, &pair.abstraction, pair_channel)
        }) {
            return Err(pair);
        }
        match self {
            Node::Leaf(node_pair) => {
                let perm = (0..node_pair.output_set.channels()).collect();
                Self::combine_permuted(node_pair, pair, perm, matching)
            }
            Node::Inner { children, .. } => children[0]
                .combine_with_subsuming(pair, matching.clone())
                .or_else(|pair| children[1].combine_with_subsuming(pair, matching)),
        }
    }

    fn combine_permuted(
        node_pair: &mut AbstractedPair<T>,
        mut pair: AbstractedPair<T>,
        mut perm: CVec<usize>,
        mut matching: Matching,
    ) -> Result<(), AbstractedPair<T>> {
        let channels = node_pair.output_set.channels();

        let mut unique_matched = 0;

        let orig_abstraction = pair.abstraction.clone();
        let orig_output_set = pair.output_set.clone();

        for channel_a in 0..channels {
            if let Some(channel_b) = matching.unique_match_a(channel_a) {
                unique_matched += 1;
                if channel_b != channel_a {
                    matching.swap_channels_b(channel_b, channel_a);
                    perm.swap(channel_b, channel_a);
                    pair.abstraction.swap_channels(channel_b, channel_a);
                    pair.output_set.swap_channels(channel_b, channel_a);
                }
            }
        }

        if unique_matched == channels {
            if node_pair.output_set.subsumes(&pair.output_set) {
                node_pair.item.combine(perm, pair.item);
                return Ok(());
            }
        } else {
            // TODO check of fixed channels?
            let (count_a, channel_a) = (0..channels)
                .map(|a| (matching.matches_a(a).count_ones(), a))
                .filter(|&(count, _)| count > 1)
                .min()
                .unwrap();
            let (count_b, channel_b) = (0..channels)
                .map(|a| (matching.matches_a(a).count_ones(), a))
                .filter(|&(count, _)| count > 1)
                .min()
                .unwrap();

            if count_a < count_b {
                for channel_b in 0..channels {
                    let mut next_matching = matching.clone();
                    if !next_matching.select(channel_a, channel_b) {
                        match Self::combine_permuted(node_pair, pair, perm.clone(), next_matching) {
                            Ok(perm) => return Ok(perm),
                            Err(returned_pair) => pair = returned_pair,
                        }
                    }
                }
            } else {
                for channel_a in 0..channels {
                    let mut next_matching = matching.clone();
                    if !next_matching.select(channel_a, channel_b) {
                        match Self::combine_permuted(node_pair, pair, perm.clone(), next_matching) {
                            Ok(perm) => return Ok(perm),
                            Err(returned_pair) => pair = returned_pair,
                        }
                    }
                }
            }
        }

        pair.abstraction = orig_abstraction;
        pair.output_set = orig_output_set;
        Err(pair)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn gen_some_output_sets(channels: usize) -> Vec<OutputSet> {
        let all_values = OutputSet::all_values(channels);

        let mut some_outputs = vec![];

        for j in 0..channels {
            for i in 0..j {
                let tmp = all_values.apply_comparator(i, j);
                for j_2 in 0..channels {
                    for i_2 in 0..j_2 {
                        let tmp_2 = tmp.apply_comparator(i_2, j_2);
                        for j_3 in 0..channels {
                            for i_3 in 0..j_3 {
                                let mut tmp_3 = tmp_2.apply_comparator(i_3, j_3);
                                tmp_3.order_channels_by_weight();
                                some_outputs.push(tmp_3);
                            }
                        }
                    }
                }
            }
        }

        some_outputs
    }

    #[test]
    fn build_index() {
        crate::logging::setup();

        for (i, &expected) in [1, 4, 6, 7, 7, 7].iter().enumerate() {
            let some_output_sets = gen_some_output_sets(i + 3);

            log::info!("initial output sets: {}", some_output_sets.len());

            let mut index = SubsumeIndex::<usize>::default();

            for output_set in some_output_sets.iter() {
                let pair = AbstractedPair {
                    abstraction: output_set.abstraction(),
                    output_set: output_set.clone(),
                    item: 1,
                };

                index.insert(pair);
            }

            log::info!("index output sets: {}", index.len());

            index.subsume_all();

            log::info!("minimal output sets: {}", index.len());
            assert_eq!(index.len(), expected);

            index.drain_using(|pair| {
                log::info!(
                    "minimal output set: size = {:?} paths = {}",
                    pair.output_set.values().len(),
                    pair.item
                );
            })
        }
    }
}
