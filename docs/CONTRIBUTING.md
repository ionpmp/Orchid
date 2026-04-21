# Contributing to Orchid

Thank you for your interest in the project! Please read this document before contributing.

## Code of Conduct

Be respectful. We are building a product for a diverse audience and expect the same from contributors. See [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## How to Help

### Report a Bug

1. Check that there is no existing issue for it
2. Open a new issue using the "Bug Report" template
3. Include your Windows version, Orchid version, and reproduction steps

### Propose a Feature

1. Open a Discussion to evaluate fit and design
2. Once aligned, file an issue using the "Feature Request" template

### Submit Code

1. Fork the repository
2. Create a branch: `git checkout -b feat/my-feature` or `fix/issue-123`
3. Write code following the guidelines below
4. Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
5. Open a Pull Request describing your changes

## Code Standards

### Rust

- **Edition:** 2021
- **MSRV:** 1.82
- **Formatter:** `rustfmt` with settings from `rustfmt.toml`
- **Linter:** `clippy` with `-D warnings`
- **Naming:** idiomatic Rust (snake_case for functions/modules, PascalCase for types)

### Commits

We use [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` — new functionality
- `fix:` — bug fix
- `docs:` — documentation changes
- `refactor:` — refactoring without behavior change
- `perf:` — performance optimization
- `test:` — adding or changing tests
- `chore:` — dependency updates, infrastructure
- `build:` — build system changes

Example: `feat(terminal): add sixel graphics support`

### Pull Requests

- One PR = one logical task
- Description: what changes and why
- If UX changes — attach screenshots or video
- Link PRs to issues: `Closes #123`

### Tests

- Unit tests — alongside code, in `#[cfg(test)] mod tests`
- Integration tests — in `tests/` of each crate
- UI tests — in `crates/orchid-ui/tests/`

## Architecture

See [`ARCHITECTURE.md`](ARCHITECTURE.md). Discuss major changes in an issue or discussion before implementing.

## License

By submitting code, you agree that it will be distributed under AGPL-3.0 (see [`LICENSE`](../LICENSE)).
