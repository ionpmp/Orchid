//! RGA-style text CRDT with deterministic concurrent merge.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use orchid_crypto::content::hash_bytes;

/// Logical writer identity (human, AI agent, …).
pub type ActorId = u64;

/// Globally unique operation id: `(actor, per-actor counter)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpId {
    /// Actor that created the op.
    pub actor: ActorId,
    /// Monotonic counter for this actor.
    pub counter: u64,
}

/// One CRDT operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Insert UTF-8 text after `after` (`None` = start of document).
    Insert {
        /// Id of this insert.
        id: OpId,
        /// Lamport clock at creation (for tie-breaking / causality hints).
        lamport: u64,
        /// Parent insert id; `None` means insert at the beginning.
        after: Option<OpId>,
        /// UTF-8 text to insert (may be multiple chars).
        text: String,
    },
    /// Tombstone the insert identified by `target`.
    Delete {
        /// Id of this delete op.
        id: OpId,
        /// Lamport clock at creation.
        lamport: u64,
        /// Insert op to hide.
        target: OpId,
    },
}

impl Op {
    /// Operation id.
    #[must_use]
    pub fn id(&self) -> OpId {
        match self {
            Self::Insert { id, .. } | Self::Delete { id, .. } => *id,
        }
    }

    /// Lamport timestamp.
    #[must_use]
    pub fn lamport(&self) -> u64 {
        match self {
            Self::Insert { lamport, .. } | Self::Delete { lamport, .. } => *lamport,
        }
    }
}

/// Mutable CRDT document: op set + per-actor clocks.
#[derive(Debug, Clone, Default)]
pub struct CrdtDocument {
    ops: BTreeMap<OpId, Op>,
    next_counter: HashMap<ActorId, u64>,
    lamport: u64,
}

impl CrdtDocument {
    /// Empty document.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Highest Lamport seen.
    #[must_use]
    pub fn lamport(&self) -> u64 {
        self.lamport
    }

    /// All ops in id order (stable).
    pub fn ops(&self) -> impl Iterator<Item = &Op> {
        self.ops.values()
    }

    /// Number of ops in the log.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Whether the log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Allocate the next [`OpId`] for `actor`.
    pub fn next_id(&mut self, actor: ActorId) -> OpId {
        let counter = self.next_counter.entry(actor).or_insert(0);
        *counter += 1;
        OpId {
            actor,
            counter: *counter,
        }
    }

    fn bump_lamport(&mut self, observed: u64) -> u64 {
        self.lamport = self.lamport.max(observed) + 1;
        self.lamport
    }

    /// Insert `text` after `after` (`None` = beginning) as `actor`.
    pub fn local_insert(&mut self, actor: ActorId, after: Option<OpId>, text: impl Into<String>) -> OpId {
        let id = self.next_id(actor);
        let lamport = self.bump_lamport(0);
        let op = Op::Insert {
            id,
            lamport,
            after,
            text: text.into(),
        };
        self.apply_op(op);
        id
    }

    /// Delete the insert `target` as `actor`.
    pub fn local_delete(&mut self, actor: ActorId, target: OpId) -> OpId {
        let id = self.next_id(actor);
        let lamport = self.bump_lamport(0);
        let op = Op::Delete { id, lamport, target };
        self.apply_op(op);
        id
    }

    /// Integrate a remote op (idempotent by op id).
    pub fn apply_op(&mut self, op: Op) {
        let id = op.id();
        if self.ops.contains_key(&id) {
            return;
        }
        self.lamport = self.lamport.max(op.lamport());
        let entry = self.next_counter.entry(id.actor).or_insert(0);
        *entry = (*entry).max(id.counter);
        self.ops.insert(id, op);
    }

    /// Merge another document's ops into this one (union by op id).
    pub fn merge(&mut self, other: &CrdtDocument) {
        for op in other.ops.values() {
            self.apply_op(op.clone());
        }
    }

    /// Materialize visible UTF-8 text.
    #[must_use]
    pub fn materialize(&self) -> String {
        let deleted: HashSet<OpId> = self
            .ops
            .values()
            .filter_map(|op| match op {
                Op::Delete { target, .. } => Some(*target),
                Op::Insert { .. } => None,
            })
            .collect();

        // children: parent -> sorted insert ids (siblings ordered by OpId)
        let mut children: BTreeMap<Option<OpId>, BTreeSet<OpId>> = BTreeMap::new();
        for op in self.ops.values() {
            if let Op::Insert { id, after, .. } = op {
                children.entry(*after).or_default().insert(*id);
            }
        }

        let mut out = String::new();
        fn walk(
            id: Option<OpId>,
            children: &BTreeMap<Option<OpId>, BTreeSet<OpId>>,
            ops: &BTreeMap<OpId, Op>,
            deleted: &HashSet<OpId>,
            out: &mut String,
        ) {
            let Some(kids) = children.get(&id) else {
                return;
            };
            for child in kids {
                if let Some(Op::Insert { text, .. }) = ops.get(child) {
                    if !deleted.contains(child) {
                        out.push_str(text);
                    }
                    // Descend even if deleted so later inserts after a
                    // deleted parent remain reachable (RGA).
                    walk(Some(*child), children, ops, deleted, out);
                }
            }
        }
        walk(None, &children, &self.ops, &deleted, &mut out);
        out
    }

    /// BLAKE3 of materialized UTF-8 bytes.
    #[must_use]
    pub fn materialize_hash(&self) -> [u8; 32] {
        hash_bytes(self.materialize().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUMAN: ActorId = 1;
    const AGENT: ActorId = 2;

    #[test]
    fn sequential_inserts() {
        let mut d = CrdtDocument::new();
        let a = d.local_insert(HUMAN, None, "Hello");
        d.local_insert(HUMAN, Some(a), " world");
        assert_eq!(d.materialize(), "Hello world");
    }

    #[test]
    fn concurrent_inserts_commute() {
        let mut a = CrdtDocument::new();
        let root = a.local_insert(HUMAN, None, "X");

        let mut b = a.clone();
        a.local_insert(HUMAN, Some(root), "A");
        b.local_insert(AGENT, Some(root), "B");

        let mut ab = a.clone();
        ab.merge(&b);
        let mut ba = b.clone();
        ba.merge(&a);

        assert_eq!(ab.materialize(), ba.materialize());
        assert_eq!(ab.materialize_hash(), ba.materialize_hash());
        // Sibling order is by OpId: AGENT(2) > HUMAN(1) for same counter…
        // After root, human insert id (1,2) vs agent (2,1) — compare OpIds.
        let text = ab.materialize();
        assert!(text.starts_with('X'));
        assert_eq!(text.len(), 3);
        assert!(text.contains('A') && text.contains('B'));
    }

    #[test]
    fn delete_hides_insert() {
        let mut d = CrdtDocument::new();
        let a = d.local_insert(HUMAN, None, "ab");
        d.local_delete(HUMAN, a);
        assert_eq!(d.materialize(), "");
    }
}
