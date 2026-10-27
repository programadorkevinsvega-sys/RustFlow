# Contributing to RustFlow

Thank you for considering a contribution. RustFlow is in its design and foundation phase; its APIs and crate boundaries may change.

## Before you start

1. Read [README.md](README.md), [ROADMAP.md](ROADMAP.md), [ARCHITECTURE_RULES.md](ARCHITECTURE_RULES.md), [AGENTS.md](AGENTS.md), and [SECURITY.md](SECURITY.md).
2. For a large change, open an issue or discussion first so scope and architecture can be agreed.
3. Check existing issues and pull requests to avoid duplicated work.
4. Never include credentials, production data, or other secrets in issues, tests, commits, or fixtures.

## Contribution workflow

1. Fork the repository or create a feature branch.
2. Keep each pull request focused on one logical change.
3. Explain the problem, proposed solution, alternatives, trade-offs, and any migration impact.
4. Add tests for behavior, edge cases, and failure paths.
5. Update docs, examples, and roadmap status when appropriate.
6. Open a pull request against `main`; do not merge your own changes unless repository governance explicitly authorizes it.
7. Respond to review feedback and keep the branch current with the target branch.

Suggested branch names: `feat/short-description`, `fix/short-description`, `docs/short-description`, and `chore/short-description`.

## Quality checks

Once the Cargo workspace exists, run the checks that apply to your change:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test -p rustflow-cli
```

For CLI changes, run `cargo test -p rustflow-cli`; the integration suite creates isolated temporary applications and builds a generated project. For browser/runtime changes, run the relevant WASM build and browser end-to-end tests. For security-sensitive changes, add regression tests and explain the threat model. If a command cannot run because the relevant workspace or tool is not yet implemented, state that clearly in the PR.

Do not claim that a check passed unless it actually ran and passed.

## Architecture and design

- Follow [ARCHITECTURE_RULES.md](ARCHITECTURE_RULES.md).
- Keep domain and application policies independent from concrete adapters.
- Prefer small, testable changes over speculative abstractions.
- Add or update an Architecture Decision Record for major crate-boundary, runtime, hydration, dependency, or public API decisions.
- Mark roadmap tasks complete only when acceptance criteria are supported by code, tests, or verifiable repository evidence.

## Commit messages

Use concise imperative messages with an optional scope, for example:

- `docs: clarify MSRV policy`
- `feat(reactivity): add signal tracking`
- `fix(router): reject malformed path parameters`

## Versioning, MSRV, and compatibility

RustFlow is pre-1.0. Public APIs are not stable, and breaking changes may occur between minor releases. Such changes must be documented with rationale and migration notes.

- **Version format:** use `MAJOR.MINOR.PATCH` once versioned releases begin.
- **Before 1.0:** `0.y.z` denotes pre-stable development. A breaking public API change increments the minor version; patch releases are reserved for fixes intended not to break the documented API. Experimental APIs may change, but must be clearly labelled.
- **After 1.0:** follow Semantic Versioning. Breaking public API changes require a new major version except where the SemVer specification permits otherwise.
- **Deprecation:** after 1.0, mark deprecated Rust APIs with `#[deprecated]` where applicable, explain the replacement, and provide migration notes. Keep deprecated APIs for at least one subsequent minor release before removal in a major release, except where a security or legal issue requires faster action. Document exceptions in release notes.
- **MSRV:** the initial Minimum Supported Rust Version is 1.85. Use the stable channel for development. Once Cargo manifests exist, every published crate must declare `rust-version = "1.85"` (or the then-approved project MSRV), and CI must test the declared MSRV plus current stable. Raising MSRV requires a documented rationale, a roadmap/release note, and an update to all relevant manifests and documentation. Before 1.0, MSRV may be raised in a minor release; after 1.0, MSRV policy changes must be announced and follow the project's compatibility policy.
- **Toolchain:** `rust-toolchain.toml` selects `stable` and installs `rustfmt` and `clippy`. The toolchain file intentionally tracks stable; the MSRV is a separate compatibility floor.


## Security reports

Do not publish exploit details in public issues. Follow [SECURITY.md](SECURITY.md) and use GitHub Private Vulnerability Reporting if it is enabled.

## Code of conduct

Participation is subject to [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
