//! Linear-time matching for sequential view children.
//!
//! Keep these matching rules equivalent to the DOM reconciler's previous
//! implementation: keyed children match the first unused child with the same
//! key, while unkeyed children prefer their previous positional slot and then
//! fall back to the first unused unkeyed child.

use std::collections::{HashMap, VecDeque};

use rustflow_view::{Key, View};

/// Return the previous-child index matched by each next child.
///
/// The index is absent when a child has no compatible predecessor. Keyed
/// lookups and unkeyed fallback lookups are indexed once, avoiding a full scan
/// of the previous sibling list for every next child.
pub(crate) fn match_sequence(previous: &[View], next: &[View]) -> Vec<Option<usize>> {
    let mut keyed_indices = HashMap::<Key, VecDeque<usize>>::new();
    let mut unkeyed_indices = VecDeque::new();

    for (index, view) in previous.iter().enumerate() {
        match view_key(view) {
            Some(key) => keyed_indices
                .entry(key.clone())
                .or_default()
                .push_back(index),
            None => unkeyed_indices.push_back(index),
        }
    }

    let mut used = vec![false; previous.len()];
    let mut matches = Vec::with_capacity(next.len());

    for (next_index, next_view) in next.iter().enumerate() {
        let matched = match view_key(next_view) {
            Some(key) => keyed_indices.get_mut(key).and_then(VecDeque::pop_front),
            None => {
                // Preserve the positional preference of the original reconciler.
                let positional = (next_index < previous.len()
                    && !used[next_index]
                    && view_key(&previous[next_index]).is_none())
                .then_some(next_index);

                positional.or_else(|| {
                    while let Some(index) = unkeyed_indices.pop_front() {
                        if !used[index] {
                            return Some(index);
                        }
                    }
                    None
                })
            }
        };

        if let Some(index) = matched {
            used[index] = true;
        }
        matches.push(matched);
    }

    matches
}

fn view_key(view: &View) -> Option<&Key> {
    match view {
        View::Element(element) => element.key(),
        View::Component(component) => component.key(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::match_sequence;
    use rustflow_view::{Element, View};

    fn keyed_element(key: &str) -> View {
        Element::new("span")
            .expect("valid element")
            .with_key(key)
            .build()
    }

    #[test]
    fn unkeyed_children_keep_their_previous_positions() {
        let previous = vec![View::text("old-a"), View::text("old-b")];
        let next = vec![View::text("new-a"), View::text("new-b")];

        assert_eq!(match_sequence(&previous, &next), vec![Some(0), Some(1)]);
    }

    #[test]
    fn keyed_children_match_the_first_unused_occurrence_of_their_key() {
        let previous = vec![keyed_element("duplicate"), keyed_element("duplicate")];
        let next = vec![keyed_element("duplicate")];

        assert_eq!(match_sequence(&previous, &next), vec![Some(0)]);
    }

    #[test]
    fn keyed_matches_do_not_consume_unkeyed_fallback_candidates() {
        let previous = vec![View::text("unkeyed"), keyed_element("stable")];
        let next = vec![keyed_element("stable"), View::text("new unkeyed")];

        assert_eq!(match_sequence(&previous, &next), vec![Some(1), Some(0)]);
    }

    #[test]
    fn unmatched_keys_never_match_unkeyed_children() {
        let previous = vec![View::text("unkeyed")];
        let next = vec![keyed_element("missing")];

        assert_eq!(match_sequence(&previous, &next), vec![None]);
    }

    #[test]
    fn fallback_skips_unkeyed_children_already_claimed_by_positional_matches() {
        let previous = vec![
            View::text("first"),
            keyed_element("middle"),
            View::text("last"),
        ];
        let next = vec![
            keyed_element("middle"),
            View::text("new-first"),
            View::text("new-last"),
        ];

        assert_eq!(
            match_sequence(&previous, &next),
            vec![Some(1), Some(0), Some(2)]
        );
    }

    #[test]
    fn ten_thousand_unkeyed_siblings_are_matched_in_order() {
        let previous = (0..10_000)
            .map(|index| View::text(index.to_string()))
            .collect::<Vec<_>>();
        let next = (0..10_000)
            .map(|index| View::text(format!("updated-{index}")))
            .collect::<Vec<_>>();

        let matches = match_sequence(&previous, &next);
        assert_eq!(matches.len(), 10_000);
        assert!(
            matches
                .into_iter()
                .enumerate()
                .all(|(index, matched)| matched == Some(index))
        );
    }
    fn sequence_from_pattern(mut pattern: usize, len: usize) -> Vec<View> {
        (0..len)
            .map(|_| {
                let item = pattern % 4;
                pattern /= 4;
                match item {
                    0 => View::text("unkeyed"),
                    1 | 3 => keyed_element("duplicate"),
                    _ => keyed_element("other"),
                }
            })
            .collect()
    }

    fn previous_matching_contract(previous: &[View], next: &[View]) -> Vec<Option<usize>> {
        let mut used = vec![false; previous.len()];
        next.iter()
            .enumerate()
            .map(|(next_index, next_view)| {
                let matched = if let Some(key) = super::view_key(next_view) {
                    previous.iter().enumerate().find_map(|(index, candidate)| {
                        (!used[index] && super::view_key(candidate) == Some(key)).then_some(index)
                    })
                } else if next_index < previous.len()
                    && !used[next_index]
                    && super::view_key(&previous[next_index]).is_none()
                {
                    Some(next_index)
                } else {
                    previous.iter().enumerate().find_map(|(index, candidate)| {
                        (!used[index] && super::view_key(candidate).is_none()).then_some(index)
                    })
                };

                if let Some(index) = matched {
                    used[index] = true;
                }
                matched
            })
            .collect()
    }

    #[test]
    fn indexed_matching_is_equivalent_to_the_previous_contract_for_mixed_sequences() {
        for len in 0..=4 {
            let pattern_count = 4usize.pow(len as u32);
            for previous_pattern in 0..pattern_count {
                let previous = sequence_from_pattern(previous_pattern, len);
                for next_pattern in 0..pattern_count {
                    let next = sequence_from_pattern(next_pattern, len);
                    assert_eq!(
                        match_sequence(&previous, &next),
                        previous_matching_contract(&previous, &next),
                        "matching changed for length {len}, previous pattern {previous_pattern}, next pattern {next_pattern}"
                    );
                }
            }
        }
    }
}

