//! In-crate RGA text CRDT for Structured regions (`orchid.structured.crdt.v1`).

mod rga;
mod wire;

pub use rga::{ActorId, CrdtDocument, Op, OpId};
pub use wire::{decode_crdt_payload, encode_crdt_payload, CRDT_PAYLOAD_MAGIC};
