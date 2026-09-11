//! Property-style round-trips for framing, embeddings, zstd, and CRDT wire.

use orchid_format::{
    align_up, compress_zstd, decode_crdt_payload, decompress_zstd, document_embedding,
    encode_crdt_payload, pad_len, CrdtDocument, EmbeddingPayload, Footer, Header, RegionHeader,
    ALIGNMENT,
};

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0
    }

    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| (self.next() >> 24) as u8).collect()
    }
}

#[test]
fn header_region_footer_roundtrip_many_seeds() {
    let mut rng = Lcg(0xF0_F0_F0_F0_F0_F0_F0_01);
    for _ in 0..64 {
        let mut uuid = [0u8; 16];
        for b in &mut uuid {
            *b = (rng.next() >> 16) as u8;
        }
        let h = Header::new(uuid, rng.next(), rng.next());
        assert_eq!(Header::decode(&h.encode()).unwrap(), h);

        let r = RegionHeader {
            type_id: (rng.next() % 64) as u16,
            length: rng.next(),
        };
        assert_eq!(RegionHeader::decode(&r.encode(), 4096).unwrap(), r);

        let mut toc = [0u8; 32];
        for b in &mut toc {
            *b = (rng.next() >> 8) as u8;
        }
        let f = Footer {
            toc_offset_from_end: rng.next(),
            toc_blake3: toc,
        };
        assert_eq!(Footer::decode(&f.encode()).unwrap(), f);
    }
}

#[test]
fn alignment_is_idempotent_and_covers_offset() {
    let mut rng = Lcg(0xA11_64_0000_0001);
    for _ in 0..128 {
        let n = rng.next() % (ALIGNMENT * 8);
        let up = align_up(n);
        assert!(up >= n);
        assert_eq!(up, n + pad_len(n));
        if n == 0 {
            assert_eq!(up, 0);
        } else {
            assert_eq!(up % ALIGNMENT, 0);
        }
        assert_eq!(align_up(up), up);
    }
}

#[test]
fn embedding_payload_roundtrip() {
    let mut rng = Lcg(0x00EB_ED00_00EB_ED01);
    for _ in 0..48 {
        let dims = 1 + (rng.next() as usize % 32);
        let vector: Vec<f32> = (0..dims)
            .map(|_| ((rng.next() as i16) as f32) / 1000.0)
            .collect();
        let span = rng.next() % 10_000;
        let tokens = (rng.next() % 512) as u32;
        let payload = document_embedding("orchid.prop.v1", span, tokens, vector.clone());
        let back = EmbeddingPayload::decode(&payload.encode().unwrap()).unwrap();
        assert_eq!(back, payload);
        assert_eq!(back.document_vector().unwrap(), vector.as_slice());
    }
}

#[test]
fn zstd_roundtrip_random_payloads() {
    let mut rng = Lcg(0x257D_257D_257D_257D);
    for _ in 0..24 {
        let n = (rng.next() as usize) % 2048;
        let src = rng.bytes(n);
        let c = compress_zstd(&src).unwrap();
        assert_eq!(decompress_zstd(&c).unwrap(), src);
    }
}

#[test]
fn crdt_wire_roundtrip_preserves_materialized_text() {
    let mut rng = Lcg(0x0C0D_7000_0000_0001);
    for _ in 0..24 {
        let mut doc = CrdtDocument::new();
        let mut after = None;
        let chunks = 1 + (rng.next() as usize % 5);
        for _ in 0..chunks {
            let len = 1 + (rng.next() as usize % 12);
            let text: String = (0..len)
                .map(|_| char::from(b'a' + (rng.next() % 26) as u8))
                .collect();
            after = Some(doc.local_insert(1, after, text));
        }
        let bytes = encode_crdt_payload(&doc).unwrap();
        let back = decode_crdt_payload(&bytes).unwrap();
        assert_eq!(back.materialize(), doc.materialize());
    }
}

#[test]
fn framing_and_embedding_decoders_do_not_panic_on_noise() {
    let mut rng = Lcg(0x015E_0000_0000_0001);
    for _ in 0..64 {
        let blob = rng.bytes(80);
        let _ = Header::decode(&blob);
        let _ = RegionHeader::decode(&blob, 0);
        let _ = Footer::decode(&blob);
        let _ = EmbeddingPayload::decode(&blob);
        let _ = decode_crdt_payload(&blob);
        let _ = decompress_zstd(&blob);
    }
}
