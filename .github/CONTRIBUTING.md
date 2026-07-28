# Lattice Contribution Guidelines

This document defines the mandatory standards for contributing to the Lattice monorepo, including:

- `crates/lattice` — the CLI: argument parsing, commands, JSON schema emission
- `crates/lattice-config` — `lattice.json` types, loading, and validation
- `crates/lattice-workspace` — workspace discovery, driver detection, toolchain provisioning
- `crates/dagger` — the task dependency graph and scheduler
- `crates/lattice-cache` — the content-addressed output cache
- `crates/lattice-runner` — the async task executor
- `crates/lattice-output` — terminal output and the interactive run UI
- `apps/web` — the Astro documentation site at [latticeandcompany.github.io/lattice](https://latticeandcompany.github.io/lattice)

This project operates under a centralized governance model, strict architectural standards, a controlled branching system (`feature/*` → `mega`), and testing requirements that apply to every change. Contributions that do not meet them are rejected.

## Rationale

These guidelines exist to:

- Keep Lattice acting on evidence: it runs the tools a repository already declares, and does not substitute a command of its own
- Keep every build reproducible — a cache key that lies is worse than no cache
- Keep everything Lattice writes inside `./.lattice/`, removable with `rm -rf`
- Prevent architectural drift across the crates in the workspace
- Keep `scripts/stress-test.sh` authoritative
- Preserve long-term extensibility as driver and engine coverage grows

This is not a consensus-driven repository. Final authority rests with maintainers.

## Governance Model

This repository operates under centralized maintainership.

- **Primary Maintainer:** Ryan Mullin

Maintainers retain unilateral authority over:

- Merge approvals
- Branch protections
- Release timing
- Publishing and distribution
- Documentation site deployment
- Contributor access

Contribution does not grant governance rights.

## Branching Policy (Mandatory)

### Branch Hierarchy

- `mega` → the production branch
- `feature/*`, `fix/*`, `refactor/*`, `docs/*` → individual development branches

### Required Merge Flow

All work must follow:

`feature/* → mega`

Pull requests target `mega`. Direct pushes to `mega` are reserved for maintainers.

### Branch Rules

Branches must:

- Be created from the current tip of `mega`
- Be singular in scope
- Avoid unrelated changes
- Remain focused and clean

Naming examples:

- `feature/add-bun-driver`
- `fix/cache-key-ignores-env`
- `refactor/split-runner-scheduler`
- `docs/toolchain-table`

### Merge Requirements

A branch may be merged when:

- `cargo fmt --all --check` passes
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- `cargo test --workspace` passes
- `scripts/stress-test.sh` exits `0`
- No behavior regressions in the example repositories
- No cache-correctness regressions (no false hits, no missed invalidations)
- No architectural violations

Merges are typically performed solely by Ryan Mullin. This is not a voting process.

Merges are **not** squashed. Individual commits survive in history, so write them accordingly.

### Emergency Policy

If `mega` is compromised:

- Immediate revert
- Root cause identification
- Patch on a `fix/*` branch
- Re-validation including the full stress test
- Controlled re-merge

Hotfix authority remains with maintainers.

## Development Setup

### Prerequisites

- Rust stable (1.86+), with `rustfmt` and `clippy`
- Node 26+ and npm — only if you are touching `apps/web`
- A POSIX shell for the stress test
- No global installs unless unavoidable

### Build and test

```bash
git clone https://github.com/latticeandcompany/lattice
cd lattice
cargo build
cargo test --workspace
```

| Command | Description |
|---|---|
| `cargo build` | Debug build of every crate |
| `cargo build --release` | Optimized binary at `target/release/lattice` |
| `cargo test --workspace` | Unit, integration, and end-to-end tests |
| `cargo fmt --all --check` | Formatting gate (CI runs this) |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint gate (CI runs this) |
| `scripts/stress-test.sh` | Full hermetic end-to-end suite |
| `scripts/dev-link.sh` | Point `./.lattice/bin/lattice` at your dev build |
| `scripts/dev-unlink.sh` | Restore the pinned release binary |

### Dogfooding

Lattice builds Lattice. The repository root has its own `lattice.json` declaring every crate and the docs site as workspaces, so once you have a dev binary linked you can drive the repo with the tool you are changing:

```bash
scripts/dev-link.sh          # .lattice/bin/lattice → target/debug/lattice
.lattice/bin/lattice run build
.lattice/bin/lattice run test --filter lattice-cache
```

Run your change against the example repositories before opening a pull request:

```bash
cd examples/polyglot     # several languages, mixed auto and manual workspaces
../../target/debug/lattice run build
../../target/debug/lattice run build      # second run must be all cache hits

cd ../nested-repo        # a subtree with its own task runner, wrapped as one workspace
../../target/debug/lattice run build
```

If you change detection, hashing, or execution behavior, dogfooding is the fastest way to notice you broke something the tests do not yet cover. Then add the test.

## Code Standards

### Rust

Formatting is `rustfmt` with the repository defaults. Run it before you commit.

Required:

- Clippy clean at `-D warnings`. If an `allow` is genuinely correct, it carries a comment explaining why.
- Errors propagate. Return `Result` and add context; do not swallow failures or log-and-continue.
- `unwrap()`, `expect()`, and `panic!` are forbidden on any path reachable by user input — that includes malformed `lattice.json`, missing tools, absent files, and failed commands. They are acceptable in tests and for genuine invariants, where `expect()` states the invariant.
- Public items in library crates carry doc comments. Keep them short.
- Prefer borrowing over cloning; clone deliberately, not defensively.
- New user-facing strings go through `lattice-output`, not raw `println!`.

See `.agents/CODESTYLE.md` for the standards covering `apps/web` (tabs, CRLF, single quotes, camelCase, arrow functions only, Bootstrap for structure and Tailwind for utilities).

### Comments

Comment sparingly, and only where the code is genuinely ambiguous or the reasoning is non-obvious — a cache-key subtlety, a platform quirk, a deliberate ordering. Do not narrate what the code already says.

### Tool detection

A workspace's tool is resolved from evidence, in this order:

- A user's explicit declaration wins.
- Then a native declaration file.
- Then a lockfile.
- If nothing identifies a tool, Lattice reports that and stops.

Any change that assumes a tool, injects a default command, or reorders that ladder will be rejected.

## Dependencies

Allowed:

- Crates with a stated justification, added in the same pull request that uses them
- Everything Lattice installs at runtime under `./.lattice/`

Prohibited:

- Network access on any path that is not explicitly provisioning a pinned toolchain
- Writing outside `./.lattice/` and declared task outputs
- Global installs, caches shared outside the repository, or a feature that requires an account
- Telemetry of any kind

## Testing Policy

Every change ships tests, in the same pull request.

- Unit tests live beside the code they cover; integration and end-to-end tests live in each crate's `tests/`.
- Behavior changes require an update to `scripts/stress-test.sh`, which must exit `0`.
- Cache work requires proof in both directions: a hit when nothing changed, and a miss when any declared input, command, environment value, or lockfile changed.
- Driver and detection work requires a fixture with the real evidence — the actual lockfile or declaration file — not a mock of the detector.
- Bug fixes come with a regression test that fails before the fix.

Use `std` `#[test]` with fixtures. Do not add a new test framework or assertion library without maintainer approval; the existing suite is deliberately dependency-light and hermetic.

## AI-Assisted Contributions

AI-assisted contributions are welcome. Lattice is built with them.

Requirements:

- State in the pull request description that AI assistance was used, and which tool.
- You are accountable for every line you submit, including lines you did not type. "The model wrote it" is not a defense for a bug, a fabricated API, or an invented benchmark.
- Read the whole change before submitting, and confirm it does what the description claims.
- The same bar applies: tests, CHANGELOG, stress test, clippy, formatting.

If you are an agent working in this repository, read `AGENTS.md` first. It is the operating contract, and it takes precedence over inferring conventions from surrounding code.

## Pull Request Requirements

All pull requests must:

- Target `mega`
- Remain focused in scope
- Include tests for the change
- Update `CHANGELOG.md` under `## Unreleased`
- Update `scripts/stress-test.sh` when behavior changes
- Update documentation (`apps/web/src/content/docs/`, `docs/`) when behavior or configuration changes
- Disclose AI assistance
- Avoid opportunistic refactors

Pull requests may be closed without merge. Maintainer decisions are final.

## Commit Policy

Commit frequently.

### Commit Title

- Short
- Clear
- Emoji permitted
- No `type(scope):` prefixes — no `feat:`, no `fix(cache):`. Write a sentence.

### Commit Body

Must be:

- Detailed
- Explicit
- Long-form
- Reference issues, pull requests, contributors, files changed

Superficial commit messages will be rejected.

## Prohibited Contributions

The following will be rejected:

- Hardcoding a preferred toolchain, inventing a command with no evidence behind it, assuming a tool the user never declared, or reordering the detection ladder
- Network calls outside toolchain provisioning, telemetry, required accounts, global installs, or writes outside `./.lattice/`
- New crates without justification, or a dependency added "for later"
- Speculative abstractions, or an abstraction where a function would do
- `unwrap()`/`panic!` on user-reachable paths, swallowed errors, bare `#[allow]`, overcommenting
- Untested changes, including a stale stress test
- Benchmarks we have not measured, or documentation for features that do not ship
- Opportunistic refactors bundled into feature work

Repeated violations may result in access removal.

## Stability

`mega` is what users who pinned a `latticeVersion` install, so it stays releasable. Behavior changes land on a feature branch, pass the full stress test, and are merged only after review.

## Conduct

Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). Security issues follow [SECURITY.md](SECURITY.md) and never go in a public issue.

## Final Authority Clause

Lattice and its maintainers reserve full discretion over:

- Branch protections
- Merge approvals
- Release timing
- Contributor access
- Policy modification

Participation in this repository constitutes acceptance of these guidelines.
