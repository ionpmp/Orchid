//! Phase 1–2 CLI: create and inspect sealed `.orchid` files.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use orchid_crypto::Identity;
use orchid_format::{write_sealed_file, SealedCreateRequest, SealedFile, EXTENSION, MIME_TYPE};

#[derive(Debug, Parser)]
#[command(
    name = "orchid-format",
    about = "Create and read sealed .orchid containers",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Write a sealed .orchid with Raw + Clean-Text + Structured regions.
    Create {
        /// Output path (`.orchid` is appended when missing).
        #[arg(short, long)]
        output: PathBuf,
        /// UTF-8 Clean-Text file (searchable body).
        #[arg(long)]
        clean_text: PathBuf,
        /// Structured snapshot file (any bytes; zstd on disk).
        #[arg(long)]
        structured: PathBuf,
        /// Optional Raw attachment (uncompressed).
        #[arg(long)]
        raw: Option<PathBuf>,
        /// Optional MIME for the Raw region.
        #[arg(long)]
        raw_content_type: Option<String>,
        /// Encrypt all regions to this passphrase (age).
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Open a sealed .orchid and print header / TOC / Clean-Text summary.
    Read {
        /// Path to a `.orchid` file.
        path: PathBuf,
        /// Passphrase for private regions.
        #[arg(long)]
        passphrase: Option<String>,
        /// Also write Clean-Text plaintext to this path.
        #[arg(long)]
        dump_clean_text: Option<PathBuf>,
        /// Also write Structured plaintext to this path.
        #[arg(long)]
        dump_structured: Option<PathBuf>,
        /// Also write Raw plaintext to this path.
        #[arg(long)]
        dump_raw: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Commands::Create {
            output,
            clean_text,
            structured,
            raw,
            raw_content_type,
            passphrase,
        } => {
            let output = ensure_extension(output);
            let raw_bytes = match raw {
                Some(p) => fs::read(p)?,
                None => Vec::new(),
            };
            write_sealed_file(
                &output,
                &SealedCreateRequest {
                    file_uuid: None,
                    created_unix_ms: None,
                    raw: raw_bytes,
                    raw_content_type,
                    raw_name: None,
                    clean_text: fs::read(clean_text)?,
                    structured: fs::read(structured)?,
                    structured_content_type: Some("application/octet-stream".into()),
                    encrypt_with: passphrase.map(Identity::passphrase),
                },
            )?;
            println!("wrote {} ({MIME_TYPE})", output.display());
        }
        Commands::Read {
            path,
            passphrase,
            dump_clean_text,
            dump_structured,
            dump_raw,
        } => {
            let file = SealedFile::open(&path)?;
            let identity = passphrase.map(Identity::passphrase);
            let id_ref = identity.as_ref();
            let header = file.header();
            let toc = file.toc()?;
            println!("path: {}", path.display());
            println!("mime: {MIME_TYPE}");
            println!(
                "version: {}.{}",
                header.version_major, header.version_minor
            );
            println!("file_uuid: {}", hex_uuid(&header.file_uuid));
            println!("created_unix_ms: {}", header.created_unix_ms);
            println!("capability_flags: {:#x}", header.capability_flags);
            println!("generation: {}", toc.generation());
            let regions = toc.regions().ok_or("TOC missing regions")?;
            println!("regions: {}", regions.len());
            for i in 0..regions.len() {
                let r = regions.get(i);
                let name = r.name().unwrap_or("");
                let enc = if r.encryption().is_some() {
                    "encrypted"
                } else {
                    "plain"
                };
                println!(
                    "  [{i}] type={} offset={} length={} compression={} storage={} {enc} name={name}",
                    r.type_().0,
                    r.offset(),
                    r.length(),
                    r.compression().0,
                    r.storage().0
                );
            }
            let clean = file.clean_text(id_ref)?;
            println!("clean_text_bytes: {}", clean.len());
            if let Some(out) = dump_clean_text {
                fs::write(out, &clean)?;
            }
            if let Some(out) = dump_structured {
                fs::write(out, file.structured(id_ref)?)?;
            }
            if let Some(out) = dump_raw {
                fs::write(out, file.raw(id_ref)?)?;
            }
        }
    }
    Ok(())
}

fn ensure_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("orchid"))
    {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(EXTENSION);
        PathBuf::from(s)
    }
}

fn hex_uuid(bytes: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}
