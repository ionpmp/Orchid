//! Property-style checks for the `orc` command tokenizer.

use orchid_core::parse_command_line;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.0
    }
}

fn letters(rng: &mut Lcg, min: usize, max: usize) -> String {
    let n = min + (rng.next() as usize % (max - min + 1));
    (0..n)
        .map(|_| char::from(b'a' + (rng.next() % 26) as u8))
        .collect()
}

fn positional(rng: &mut Lcg) -> String {
    let kind = rng.next() % 3;
    match kind {
        0 => letters(rng, 1, 8),
        1 => {
            let left = letters(rng, 1, 5);
            let right = letters(rng, 1, 5);
            format!("{left} {right}")
        }
        _ => {
            let mut s = letters(rng, 1, 6);
            if rng.next() % 2 == 0 {
                s.push('\\');
                s.push('"');
                s.push_str(&letters(rng, 1, 3));
            }
            s
        }
    }
}

fn quote_token(s: &str) -> String {
    if s.is_empty()
        || s.starts_with('-')
        || s.chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        let mut out = String::from('"');
        for c in s.chars() {
            if c == '"' || c == '\\' {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('"');
        out
    } else {
        s.to_string()
    }
}

#[test]
fn reconstructed_command_lines_parse_back() {
    let mut rng = Lcg(0x0C00_0000_0000_0001);
    for _ in 0..80 {
        let verb = letters(&mut rng, 1, 8);
        let n_pos = rng.next() as usize % 4;
        let positionals: Vec<String> = (0..n_pos).map(|_| positional(&mut rng)).collect();
        let n_flags = rng.next() as usize % 3;
        let flags: Vec<String> = (0..n_flags).map(|_| letters(&mut rng, 1, 6)).collect();
        let n_opts = rng.next() as usize % 3;
        let options: Vec<(String, String)> = (0..n_opts)
            .map(|_| (letters(&mut rng, 1, 6), letters(&mut rng, 1, 8)))
            .collect();

        let mut line = format!("orc {verb}");
        for p in &positionals {
            line.push(' ');
            line.push_str(&quote_token(p));
        }
        for f in &flags {
            line.push_str(" --");
            line.push_str(f);
        }
        for (k, v) in &options {
            line.push_str(" --");
            line.push_str(k);
            line.push(' ');
            line.push_str(&quote_token(v));
        }

        let parsed = parse_command_line(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
        assert_eq!(parsed.verb, verb, "{line}");
        assert_eq!(parsed.positional, positionals, "{line}");
        for f in &flags {
            assert!(parsed.flags.contains(f), "missing --{f} in {line}");
        }
        for (k, v) in &options {
            assert_eq!(parsed.options.get(k), Some(v), "{line}");
        }
    }
}

#[test]
fn parser_does_not_panic_on_noise() {
    let mut rng = Lcg(0x015E_0000_0000_0002);
    for _ in 0..64 {
        let n = 1 + (rng.next() as usize % 48);
        let s: String = (0..n)
            .map(|_| char::from((rng.next() % 96) as u8 + 32))
            .collect();
        let _ = parse_command_line(&s);
        let _ = parse_command_line(&format!("orc {s}"));
    }
}
