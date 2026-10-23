//! Replicated growable array (RGA) used for sibling ordering.
//!
//! Every insert is anchored after an existing element (or at the start) and
//! carries the stamp that created it. Concurrent inserts after the same anchor
//! are ordered by stamp, so all replicas converge without coordination. Removes
//! leave tombstones so later operations that reference the removed element stay
//! resolvable; tombstones are pruned only when a snapshot replaces the log.

use std::fmt;

use crate::clock::Hlc;

/// Errors produced by RGA edits.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RgaError {
    /// The element is already present.
    #[error("element is already present in the list")]
    Duplicate,
    /// The anchor element does not exist (or is not alive).
    #[error("anchor element is not present in the list")]
    AnchorMissing,
}

struct Entry<T> {
    value: T,
    id: Hlc,
    anchor: Option<Hlc>,
    alive: bool,
}

/// Ordered replicated list of sibling ids.
pub struct RgaList<T: Clone + PartialEq> {
    entries: Vec<Entry<T>>,
}

impl<T: Clone + PartialEq> fmt::Debug for RgaList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RgaList")
            .field("entries", &self.entries.len())
            .field("alive", &self.len())
            .finish()
    }
}

impl<T: Clone + PartialEq> Default for RgaList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + PartialEq> RgaList<T> {
    /// Creates an empty list.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Number of live (non-tombstoned) elements.
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|entry| entry.alive).count()
    }

    /// Whether the list has no live elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total stored entries including tombstones (diagnostics/tests).
    pub fn total_entries(&self) -> usize {
        self.entries.len()
    }

    /// Whether `value` is present and alive.
    pub fn contains(&self, value: &T) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.alive && &entry.value == value)
    }

    /// Live values in order.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.entries
            .iter()
            .filter(|entry| entry.alive)
            .map(|entry| &entry.value)
    }

    /// Clones the live values into a vector.
    pub fn to_vec(&self) -> Vec<T> {
        self.values().cloned().collect()
    }

    /// Stamp that created the live element equal to `value`.
    pub fn stamp_of(&self, value: &T) -> Option<Hlc> {
        self.entries
            .iter()
            .find(|entry| entry.alive && &entry.value == value)
            .map(|entry| entry.id)
    }

    /// Inserts `value` after `after` (or at the start when `None`).
    ///
    /// Concurrent inserts after the same anchor are ordered by `id`, which makes
    /// the result deterministic on every replica.
    pub fn insert_after(&mut self, after: Option<&T>, value: T, id: Hlc) -> Result<(), RgaError> {
        if self.entries.iter().any(|entry| entry.value == value) {
            return Err(RgaError::Duplicate);
        }
        let anchor = match after {
            None => None,
            Some(target) => Some(
                self.entries
                    .iter()
                    .find(|entry| entry.alive && &entry.value == target)
                    .ok_or(RgaError::AnchorMissing)?
                    .id,
            ),
        };
        let mut position = match anchor {
            None => 0,
            Some(anchor_id) => self
                .entries
                .iter()
                .position(|entry| entry.id == anchor_id)
                .map(|index| index + 1)
                .unwrap_or(0),
        };
        // Insert before any concurrent sibling that has a greater stamp but the
        // same anchor.
        while position < self.entries.len() {
            let next = &self.entries[position];
            if next.anchor == anchor && next.id < id {
                position += 1;
            } else {
                break;
            }
        }
        self.entries.insert(
            position,
            Entry {
                value,
                id,
                anchor,
                alive: true,
            },
        );
        Ok(())
    }

    /// Marks the element as removed (tombstone). Returns whether it was live.
    pub fn remove(&mut self, value: &T) -> bool {
        let mut removed = false;
        for entry in &mut self.entries {
            if entry.alive && &entry.value == value {
                entry.alive = false;
                removed = true;
            }
        }
        removed
    }

    /// Position among live elements, if present.
    pub fn position(&self, value: &T) -> Option<usize> {
        self.values().position(|candidate| candidate == value)
    }

    /// Whether the element has a tombstone entry.
    pub fn is_tombstoned(&self, value: &T) -> bool {
        self.entries
            .iter()
            .any(|entry| !entry.alive && &entry.value == value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ActorId;
    use uuid::Uuid;

    fn stamp(lamport: u64, actor: u128) -> Hlc {
        Hlc::new(lamport, ActorId::from_uuid(Uuid::from_u128(actor)))
    }

    #[test]
    fn inserts_in_anchor_order() {
        let mut list = RgaList::new();
        list.insert_after(None, "a", stamp(1, 1)).unwrap();
        list.insert_after(Some(&"a"), "b", stamp(2, 1)).unwrap();
        list.insert_after(Some(&"b"), "c", stamp(3, 1)).unwrap();
        assert_eq!(list.to_vec(), vec!["a", "b", "c"]);
    }

    #[test]
    fn concurrent_inserts_converge_by_stamp() {
        // Two replicas insert after "a": replica 1 sees stamp 2, replica 2 stamp 3.
        let mut left = RgaList::new();
        let mut right = RgaList::new();
        left.insert_after(None, "a", stamp(1, 1)).unwrap();
        right.insert_after(None, "a", stamp(1, 1)).unwrap();
        left.insert_after(Some(&"a"), "x", stamp(2, 2)).unwrap();
        right.insert_after(Some(&"a"), "y", stamp(3, 1)).unwrap();
        // Cross-apply in different orders.
        left.insert_after(Some(&"a"), "y", stamp(3, 1)).unwrap();
        right.insert_after(Some(&"a"), "x", stamp(2, 2)).unwrap();
        assert_eq!(left.to_vec(), right.to_vec());
        assert_eq!(left.to_vec(), vec!["a", "x", "y"]);
    }

    #[test]
    fn duplicate_insert_is_rejected() {
        let mut list = RgaList::new();
        list.insert_after(None, "a", stamp(1, 1)).unwrap();
        assert_eq!(
            list.insert_after(None, "a", stamp(2, 1)),
            Err(RgaError::Duplicate)
        );
    }

    #[test]
    fn missing_anchor_is_rejected() {
        let mut list = RgaList::new();
        assert_eq!(
            list.insert_after(Some(&"ghost"), "a", stamp(1, 1)),
            Err(RgaError::AnchorMissing)
        );
    }

    #[test]
    fn removal_tombstones_and_keeps_anchors_resolvable() {
        let mut list = RgaList::new();
        list.insert_after(None, "a", stamp(1, 1)).unwrap();
        list.insert_after(Some(&"a"), "b", stamp(2, 1)).unwrap();
        assert!(list.remove(&"a"));
        assert!(!list.contains(&"a"));
        assert_eq!(list.to_vec(), vec!["b"]);
        assert_eq!(list.total_entries(), 2);
        assert!(list.is_tombstoned(&"a"));
        // A concurrent insert anchored after the removed element still resolves.
        list.insert_after(Some(&"b"), "c", stamp(3, 2)).unwrap();
        assert_eq!(list.to_vec(), vec!["b", "c"]);
    }

    #[test]
    fn ordering_is_independent_of_arrival_order() {
        // Three sibling inserts after the same anchor arrive in every order.
        let ids = [stamp(2, 1), stamp(3, 2), stamp(4, 3)];
        let names = ["x", "y", "z"];
        let permutations = [
            [0usize, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mut results = Vec::new();
        for permutation in permutations {
            let mut list = RgaList::new();
            list.insert_after(None, "a", stamp(1, 9)).unwrap();
            for index in permutation {
                list.insert_after(Some(&"a"), names[index], ids[index])
                    .unwrap();
            }
            results.push(list.to_vec());
        }
        assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(results[0], vec!["a", "x", "y", "z"]);
    }
}
