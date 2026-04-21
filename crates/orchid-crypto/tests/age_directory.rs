//! Directory-level age encryption round-trip.

use std::collections::HashMap;
use std::path::PathBuf;

use orchid_crypto::{Decryptor, Encryptor, Identity};

fn collect_relative_files(root: &std::path::Path) -> HashMap<PathBuf, Vec<u8>> {
    let mut out = HashMap::new();
    for entry in walkdir::WalkDir::new(root).min_depth(1) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(root).unwrap().to_path_buf();
            let bytes = std::fs::read(entry.path()).unwrap();
            out.insert(rel, bytes);
        }
    }
    out
}

#[tokio::test]
async fn nested_directory_round_trip_preserves_tree() {
    let td = tempfile::tempdir().unwrap();
    let input = td.path().join("input");
    let encrypted = td.path().join("encrypted");
    let output = td.path().join("output");

    // Create a nested tree.
    std::fs::create_dir_all(input.join("sub/deeper")).unwrap();
    std::fs::write(input.join("top.txt"), b"top-level file").unwrap();
    std::fs::write(input.join("sub/middle.bin"), vec![0xABu8; 4096]).unwrap();
    std::fs::write(input.join("sub/deeper/leaf.md"), b"# hello\nworld\n").unwrap();

    let id = Identity::passphrase("dir-pass");
    let enc = Encryptor::new(id.clone());
    let dec = Decryptor::new(id);

    let meta = enc.encrypt_directory(&input, &encrypted).await.unwrap();
    assert!(encrypted.join(".orchid-encrypted.tar.age").exists());
    assert!(encrypted.join(".orchid-encrypted.meta").exists());
    assert_eq!(
        meta.content_type_hint.as_deref(),
        Some("directory"),
        "directory hint in metadata"
    );

    dec.decrypt_directory(&encrypted, &output).await.unwrap();

    let a = collect_relative_files(&input);
    let b = collect_relative_files(&output);
    assert_eq!(a.len(), b.len(), "same number of files");
    for (path, bytes) in &a {
        let got = b.get(path).unwrap_or_else(|| panic!("missing {:?}", path));
        assert_eq!(got, bytes, "contents for {:?}", path);
    }
}
