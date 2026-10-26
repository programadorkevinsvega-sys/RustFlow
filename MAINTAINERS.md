# Maintainers and project governance

## Initial maintainer

- **Repository owner:** `programadorkevinsvega-sys` — initial administrative and project-maintenance responsibility, subject to confirmation by the repository owner.

The repository owner should keep this file current with the names or GitHub handles of active maintainers and their responsibilities. Do not infer that a contributor is a maintainer solely because they have made contributions.

## Responsibilities

Maintainers are responsible for:
- Reviewing changes for correctness, tests, security, accessibility, and architectural consistency.
- Managing releases, versioning, and supported Rust versions.
- Triage of issues and security reports.
- Maintaining the roadmap, architecture rules, and decision records.
- Applying the Code of Conduct fairly.

## Decision process

- **Routine changes:** accepted by maintainer review when the change meets repository standards.
- **Architectural changes:** require a design discussion and an Architecture Decision Record (ADR) when they affect crate boundaries, reactivity semantics, rendering/hydration, server/client boundaries, major dependencies, or public APIs.
- **Breaking changes before 1.0:** allowed, but must include rationale and migration notes.
- **Security-sensitive changes:** require focused threat analysis, regression tests, and private coordination when disclosing a vulnerability.
- **Disagreement:** discuss evidence and trade-offs in the PR or issue. The designated maintainer makes the final decision and records significant decisions in an ADR.
- **Conflicts of interest:** a maintainer should disclose the conflict and, where practical, request review from another maintainer.

## Pull request expectations

- Prefer review before merge.
- Do not merge a change that has failing required checks or unresolved high-severity security findings.
- A contributor should not be the sole approver of their own change when another maintainer is available.
- Repository branch protection and required checks are configured in GitHub settings, not by this document; see the governance checklist in [ROADMAP.md](ROADMAP.md).
