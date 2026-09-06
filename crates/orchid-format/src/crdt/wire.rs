//! Binary wire encoding for [`super::CrdtDocument`].

use crate::crdt::rga::{ActorId, CrdtDocument, Op, OpId};
use crate::{FormatError, Result};

/// Magic for CRDT Structured payloads (`ORCD` + `T` for text CRDT).
pub const CRDT_PAYLOAD_MAGIC: &[u8; 4] = b"ORCT";

const WIRE_VERSION: u32 = 1;

/// Encode a CRDT document to bytes (`orchid.structured.crdt.v1`).
pub fn encode_crdt_payload(doc: &CrdtDocument) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    buf.extend_from_slice(CRDT_PAYLOAD_MAGIC);
    buf.extend_from_slice(&WIRE_VERSION.to_le_bytes());
    // snapshot watermark reserved (0 = no compacted snapshot yet)
    buf.extend_from_slice(&0u64.to_le_bytes());
    let ops: Vec<&Op> = doc.ops().collect();
    buf.extend_from_slice(&(ops.len() as u32).to_le_bytes());
    for op in ops {
        encode_op(&mut buf, op);
    }
    Ok(buf)
}

/// Decode a CRDT payload into a document.
pub fn decode_crdt_payload(bytes: &[u8]) -> Result<CrdtDocument> {
    if bytes.len() < 4 + 4 + 8 + 4 {
        return Err(FormatError::RegionDecode("CRDT payload too short".into()));
    }
    if &bytes[0..4] != CRDT_PAYLOAD_MAGIC.as_slice() {
        return Err(FormatError::RegionDecode("CRDT magic mismatch".into()));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != WIRE_VERSION {
        return Err(FormatError::RegionDecode(format!(
            "unsupported CRDT wire version {version}"
        )));
    }
    // skip watermark at 8..16
    let mut off = 16;
    let count = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
    off += 4;
    let mut doc = CrdtDocument::new();
    for _ in 0..count {
        let (op, next) = decode_op(bytes, off)?;
        doc.apply_op(op);
        off = next;
    }
    if off != bytes.len() {
        return Err(FormatError::RegionDecode(format!(
            "CRDT trailing bytes: {} leftover",
            bytes.len() - off
        )));
    }
    Ok(doc)
}

fn encode_op(buf: &mut Vec<u8>, op: &Op) {
    match op {
        Op::Insert {
            id,
            lamport,
            after,
            text,
        } => {
            buf.push(1);
            encode_opid(buf, *id);
            buf.extend_from_slice(&lamport.to_le_bytes());
            match after {
                None => buf.push(0),
                Some(p) => {
                    buf.push(1);
                    encode_opid(buf, *p);
                }
            }
            let raw = text.as_bytes();
            buf.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            buf.extend_from_slice(raw);
        }
        Op::Delete { id, lamport, target } => {
            buf.push(2);
            encode_opid(buf, *id);
            buf.extend_from_slice(&lamport.to_le_bytes());
            encode_opid(buf, *target);
        }
    }
}

fn decode_op(bytes: &[u8], mut off: usize) -> Result<(Op, usize)> {
    if off >= bytes.len() {
        return Err(FormatError::RegionDecode("CRDT op truncated".into()));
    }
    let tag = bytes[off];
    off += 1;
    let id;
    (id, off) = decode_opid(bytes, off)?;
    if off + 8 > bytes.len() {
        return Err(FormatError::RegionDecode("CRDT lamport truncated".into()));
    }
    let lamport = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
    off += 8;
    match tag {
        1 => {
            if off >= bytes.len() {
                return Err(FormatError::RegionDecode("CRDT after flag truncated".into()));
            }
            let flag = bytes[off];
            off += 1;
            let after = if flag == 0 {
                None
            } else {
                let p;
                (p, off) = decode_opid(bytes, off)?;
                Some(p)
            };
            if off + 4 > bytes.len() {
                return Err(FormatError::RegionDecode("CRDT text len truncated".into()));
            }
            let len = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
            if off + len > bytes.len() {
                return Err(FormatError::RegionDecode("CRDT text truncated".into()));
            }
            let text = std::str::from_utf8(&bytes[off..off + len])
                .map_err(|e| FormatError::RegionDecode(format!("CRDT text utf8: {e}")))?
                .to_string();
            off += len;
            Ok((
                Op::Insert {
                    id,
                    lamport,
                    after,
                    text,
                },
                off,
            ))
        }
        2 => {
            let target;
            (target, off) = decode_opid(bytes, off)?;
            Ok((Op::Delete { id, lamport, target }, off))
        }
        other => Err(FormatError::RegionDecode(format!(
            "unknown CRDT op tag {other}"
        ))),
    }
}

fn encode_opid(buf: &mut Vec<u8>, id: OpId) {
    buf.extend_from_slice(&id.actor.to_le_bytes());
    buf.extend_from_slice(&id.counter.to_le_bytes());
}

fn decode_opid(bytes: &[u8], off: usize) -> Result<(OpId, usize)> {
    if off + 16 > bytes.len() {
        return Err(FormatError::RegionDecode("CRDT OpId truncated".into()));
    }
    let actor = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
    let counter = u64::from_le_bytes(bytes[off + 8..off + 16].try_into().unwrap());
    Ok((
        OpId {
            actor: actor as ActorId,
            counter,
        },
        off + 16,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crdt::rga::CrdtDocument;

    #[test]
    fn roundtrip_wire() {
        let mut d = CrdtDocument::new();
        let a = d.local_insert(1, None, "hi");
        d.local_insert(2, Some(a), "!");
        let bytes = encode_crdt_payload(&d).unwrap();
        assert_eq!(&bytes[0..4], b"ORCT");
        let back = decode_crdt_payload(&bytes).unwrap();
        assert_eq!(back.materialize(), d.materialize());
        assert_eq!(back.materialize_hash(), d.materialize_hash());
    }
}
