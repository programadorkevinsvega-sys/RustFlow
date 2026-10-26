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
```

For browser/runtime changes, run the relevant WASM build and browser end-to-end tests. For security-sensitive changes, add regression tests and explain the threat model. If a command cannot run because the relevant workspace or tool is not yet implemented, state that clearly in the PR.

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

## Versioning and compatibility

RustFlow is pre-1.0. Public APIs are not stable, and breaking changes may occur between minor releases. Such changes must be documented with rationale and migration notes. After 1.0, the project will follow Semantic Versioning and a documented deprecation policy.

## Security reports

Do not publish exploit details in public issues. Follow [SECURITY.md](SECURITY.md) and use GitHub Private Vulnerability Reporting if it is enabled.

## Code of conduct

Participation is subject to [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
