use arrayvec::ArrayVec;
use crossbeam::queue::{ArrayQueue, SegQueue};
use parking_lot::Mutex;
use rayon::{iter::plumbing, prelude::*};

use crate::{
    matching::Matching,
    output_set::{Abstraction, CVec, OutputSet},
};

pub trait SubsumeIndexItem: Send {
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

impl<T> AbstractedPair<T> {
    pub fn new(output_set: OutputSet, item: T) -> Self {
        Self {
            abstraction: output_set.abstraction(),
            output_set,
            item,
        }
    }
    fn mutex_wrap(self) -> AbstractedPair<Mutex<T>> {
        let Self {
            abstraction,
            output_set,
            item,
        } = self;
        AbstractedPair {
            abstraction,
            output_set,
            item: Mutex::new(item),
        }
    }
}

impl<T> AbstractedPair<Mutex<T>> {
    fn mutex_unwrap(self) -> AbstractedPair<T> {
        let Self {
            abstraction,
            output_set,
            item,
        } = self;
        AbstractedPair {
            abstraction,
            output_set,
            item: item.into_inner(),
        }
    }
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

    pub fn is_empty(&self) -> bool {
        self.trees.is_empty()
    }

    pub fn insert(&mut self, pair: AbstractedPair<T>) {
        self.combine_with_subsuming(pair).unwrap_or_else(|pair| {
            self.len += 1;
            self.trees.push(Node::Leaf(pair.mutex_wrap()));
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
        for tree in self.trees.iter_mut() {
            match tree.combine_with_subsuming(pair) {
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

            let last_tree = self.trees.pop().unwrap();

            self.len -= last_tree.len();

            let second_last_tree = self.trees.pop().unwrap();

            self.len -= second_last_tree.len();

            let mut pairs = vec![];
            second_last_tree.drain_using(
                &mut |pair| match last_tree.combine_with_subsuming(pair) {
                    Ok(()) => (),
                    Err(returned_pair) => pairs.push(returned_pair),
                },
            );

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

pub enum Node<T> {
    Leaf(AbstractedPair<Mutex<T>>),
    Inner {
        abstraction: Abstraction,
        children: Box<[Node<T>; 2]>,
        len: usize,
    },
}

impl<T: SubsumeIndexItem> Node<T> {
    pub fn new(mut items: Vec<AbstractedPair<T>>) -> Self {
        assert!(!items.is_empty());

        while let Some(last) = items.pop() {
            if let Some(second_last) = items.last_mut() {
                if second_last.output_set == last.output_set {
                    second_last
                        .item
                        .combine((0..last.output_set.channels()).collect(), last.item);
                    continue;
                }
            }
            items.push(last);
            break;
        }

        let len = items.len();
        if len == 1 {
            Node::Leaf(items.pop().unwrap().mutex_wrap())
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

            let (child_0, child_1) = rayon::join(|| Self::new(items_0), || Self::new(items_1));

            Node::Inner {
                abstraction: min_abstraction,
                children: Box::new([child_0, child_1]),
                len,
            }
        }
    }

    pub fn len(&self) -> usize {
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
            Node::Leaf(pair) => target(pair.mutex_unwrap()),
            Node::Inner { children, .. } => {
                for child in ArrayVec::from(*children) {
                    child.drain_using(target);
                }
            }
        }
    }

    pub fn combine_with_subsuming(&self, pair: AbstractedPair<T>) -> Result<(), AbstractedPair<T>> {
        let channels = pair.output_set.channels();
        self.combine_with_subsuming_rec(pair, Matching::new(channels))
    }

    fn combine_with_subsuming_rec(
        &self,
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
                .combine_with_subsuming_rec(pair, matching.clone())
                .or_else(|pair| children[1].combine_with_subsuming_rec(pair, matching)),
        }
    }

    fn combine_permuted(
        node_pair: &AbstractedPair<Mutex<T>>,
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
                let mut item = node_pair.item.lock();
                item.combine(perm, pair.item);
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

    pub fn minimal_elements(self) -> Vec<AbstractedPair<T>> {
        match self {
            Node::Inner { children, .. } => {
                let [child_0, child_1] = *children;

                let child_0 = Self::new(child_0.minimal_elements());

                let child_1_pairs = child_1
                    .flat_map(|pair| child_0.combine_with_subsuming(pair).err())
                    .collect::<Vec<_>>();

                if child_1_pairs.is_empty() {
                    return child_0.collect::<Vec<_>>();
                }

                let child_1 = Self::new(child_1_pairs);

                let mut child_0_pairs = child_0
                    .flat_map(|pair| child_1.combine_with_subsuming(pair).err())
                    .collect::<Vec<_>>();

                child_0_pairs.extend(child_1.minimal_elements());
                child_0_pairs
            }
            Node::Leaf(pair) => vec![pair.mutex_unwrap()],
        }
    }
}

pub fn incremental_minimal_elements<T, In, G>(
    inputs: Vec<In>,
    generator: G,
) -> Vec<AbstractedPair<T>>
where
    T: SubsumeIndexItem,
    G: Sync + Fn(In) -> Vec<AbstractedPair<T>>,
    In: Send,
{
    let mut node: Option<Node<T>> = None;
    let mut chunk_size = 1024;

    let input_queue = SegQueue::<In>::new();
    let spill_queue = SegQueue::<AbstractedPair<T>>::new();

    for input in inputs {
        input_queue.push(input);
    }

    while !input_queue.is_empty() || !spill_queue.is_empty() {
        let output_queue = ArrayQueue::<AbstractedPair<T>>::new(chunk_size);

        rayon::scope(|s| {
            for _ in 0..rayon::current_num_threads() {
                s.spawn(|_| {
                    while let Ok(pair) = spill_queue.pop() {
                        let res = if let Some(node) = &node {
                            node.combine_with_subsuming(pair)
                        } else {
                            Err(pair)
                        };

                        if let Err(pair) = res {
                            if let Err(pair) = output_queue.push(pair) {
                                spill_queue.push(pair.0);
                                break;
                            }
                        }
                    }

                    while !output_queue.is_full() {
                        if let Ok(item) = input_queue.pop() {
                            for pair in generator(item) {
                                let res = if let Some(node) = &node {
                                    node.combine_with_subsuming(pair)
                                } else {
                                    Err(pair)
                                };

                                if let Err(pair) = res {
                                    if let Err(pair) = output_queue.push(pair) {
                                        spill_queue.push(pair.0);
                                    }
                                }
                            }
                        } else {
                            break;
                        }
                    }
                })
            }
        });

        let mut outputs = Vec::with_capacity(chunk_size);

        while let Ok(pair) = output_queue.pop() {
            outputs.push(pair);
        }

        if outputs.is_empty() {
            continue;
        }

        let node_1 = Node::new(outputs);

        if let Some(node_0) = node {
            let mut node_0_pairs = node_0
                .flat_map(|pair| node_1.combine_with_subsuming(pair).err())
                .collect::<Vec<_>>();

            node_0_pairs.extend(node_1.minimal_elements());

            node = Some(Node::new(node_0_pairs));
        } else {
            node = Some(Node::new(node_1.minimal_elements()));
        }
        chunk_size *= 2;
    }

    node.into_par_iter().flatten().collect::<Vec<_>>()
}

pub struct NodeIter<T> {
    nodes: Vec<Node<T>>,
}

impl<T: SubsumeIndexItem> Iterator for NodeIter<T> {
    type Item = AbstractedPair<T>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.nodes.pop() {
            match node {
                Node::Leaf(pair) => return Some(pair.mutex_unwrap()),
                Node::Inner { children, .. } => self.nodes.extend(ArrayVec::from(*children)),
            }
        }

        None
    }
}

impl<T: SubsumeIndexItem> IntoIterator for Node<T> {
    type Item = AbstractedPair<T>;
    type IntoIter = NodeIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        NodeIter { nodes: vec![self] }
    }
}

impl<T: SubsumeIndexItem> plumbing::UnindexedProducer for Node<T> {
    type Item = AbstractedPair<T>;

    fn split(self) -> (Self, Option<Self>) {
        match self {
            Node::Inner { children, .. } => {
                let [child_0, child_1] = *children;
                (child_0, Some(child_1))
            }
            node => (node, None),
        }
    }

    fn fold_with<F>(self, folder: F) -> F
    where
        F: plumbing::Folder<Self::Item>,
    {
        folder.consume_iter(self.into_iter())
    }
}

impl<T: SubsumeIndexItem> ParallelIterator for Node<T> {
    type Item = AbstractedPair<T>;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: plumbing::UnindexedConsumer<Self::Item>,
    {
        plumbing::bridge_unindexed(self, consumer)
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

            let abstracted_pairs = some_output_sets
                .into_iter()
                .map(|output_set| AbstractedPair::new(output_set, 1))
                .collect();

            let minimal = Node::new(abstracted_pairs).minimal_elements();

            log::info!("minimal output sets: {}", minimal.len());
            assert_eq!(minimal.len(), expected);

            for pair in minimal {
                log::info!(
                    "minimal output set: size = {:?} paths = {}",
                    pair.output_set.values().len(),
                    pair.item
                );
            }
        }
    }
}
