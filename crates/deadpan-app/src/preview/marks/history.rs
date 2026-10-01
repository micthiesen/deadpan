//! A bounded navigation trail. Validation happens before its owner changes it.

use std::collections::VecDeque;

const LIMIT: usize = 128;

#[derive(Debug)]
pub(super) struct History<T> {
    past: VecDeque<T>,
    future: Vec<T>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            past: VecDeque::new(),
            future: Vec::new(),
        }
    }
}

impl<T: PartialEq> History<T> {
    pub fn target(&self, forward: bool) -> Option<&T> {
        if forward {
            self.future.last()
        } else {
            self.past.back()
        }
    }

    pub fn jump(&mut self, from: T, to: &T) {
        if &from == to {
            return;
        }
        self.future.clear();
        self.push_past(from);
    }

    /// Called only after the exact target from `target` was admitted. Failed
    /// lookups leave both branches intact, so failure cannot skip a location.
    pub fn complete(&mut self, forward: bool, from: T) {
        if forward {
            if self.future.pop().is_some() {
                self.push_past(from);
            }
        } else if self.past.pop_back().is_some() && self.future.last() != Some(&from) {
            self.future.push(from);
        }
    }

    fn push_past(&mut self, from: T) {
        if self.past.back() == Some(&from) {
            return;
        }
        if self.past.len() == LIMIT {
            self.past.pop_front();
        }
        self.past.push_back(from);
    }

    pub fn for_each_mut(&mut self, mut apply: impl FnMut(&mut T)) {
        for entry in self.past.iter_mut().chain(self.future.iter_mut()) {
            apply(entry);
        }
    }

    /// Drop destinations that no longer belong to the current navigation
    /// context without changing the order of either surviving branch.
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        self.past.retain(&mut keep);
        self.future.retain(keep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_branches_only_after_a_successful_new_jump() {
        let mut history = History::default();
        history.jump(0, &10);
        history.jump(10, &20);
        assert_eq!(history.target(false), Some(&10));
        // Repeated failed admission reads do not consume a destination.
        assert_eq!(history.target(false), Some(&10));
        history.complete(false, 21); // Ordinary motion since the last jump.
        assert_eq!(history.target(false), Some(&0));
        assert_eq!(history.target(true), Some(&21));
        history.complete(true, 10);
        assert_eq!(history.target(false), Some(&10));
        assert_eq!(history.target(true), None);
        history.complete(false, 21);
        history.jump(10, &10); // No movement must not destroy forward history.
        assert_eq!(history.target(true), Some(&21));
        history.jump(10, &30);
        assert_eq!(history.target(true), None);
        assert_eq!(history.target(false), Some(&10));
    }

    #[test]
    fn capacity_and_duplicate_suppression_bound_both_branches() {
        let mut history = History::default();
        for n in 0..400 {
            history.jump(n, &(n + 1));
        }
        assert_eq!(history.past.len(), LIMIT);
        assert_eq!(history.past.front(), Some(&272));
        history.jump(399, &500);
        assert_eq!(history.past.len(), LIMIT);
        for n in (272..400).rev() {
            assert_eq!(history.target(false), Some(&n));
            history.complete(false, n + 1);
        }
        assert_eq!(history.target(false), None);
        assert_eq!(history.future.len(), LIMIT);
        history.complete(false, 0);
        assert_eq!(history.future.len(), LIMIT);
    }

    #[test]
    fn retain_preserves_surviving_order_in_both_branches() {
        let mut history = History::default();
        for (from, to) in [(0, 1), (1, 2), (2, 3), (3, 4), (4, 5)] {
            history.jump(from, &to);
        }
        history.complete(false, 5);
        history.complete(false, 4);

        history.retain(|position| matches!(*position, 0 | 2 | 4 | 5));

        assert_eq!(history.past.iter().copied().collect::<Vec<_>>(), vec![0, 2]);
        assert_eq!(history.future, vec![5, 4]);
        assert_eq!(history.target(false), Some(&2));
        assert_eq!(history.target(true), Some(&4));
    }
}
