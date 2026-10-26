# Security Policy

RustFlow is an early-stage project. Do not assume it is suitable for production or security-sensitive workloads unless a specific release explicitly states otherwise.

## Supported versions

Until stable releases and a support policy exist, no release line is guaranteed security maintenance. Once versioned releases exist, this section must list supported versions and end-of-life dates.

## Reporting a vulnerability

**Do not report exploitable vulnerabilities in public issues, pull requests, or discussions.**

Use GitHub's Private vulnerability reporting feature for this repository, if enabled, or contact the maintainer through a private channel listed on the GitHub profile. If no private channel is available, request a secure reporting method without including exploit details publicly.

A useful report includes:
- Affected version, commit, crate, and feature flags.
- Rust toolchain, operating system, browser, and deployment context where relevant.
- Impact and threat scenario.
- Minimal reproduction steps or proof of concept.
- Known mitigations, if any.

Do not include real credentials, personal data, or third-party secrets.

## Response process

The maintainer will make a reasonable effort to:
1. Acknowledge receipt and request clarification if needed.
2. Reproduce and assess severity and affected versions.
3. Coordinate a fix, release, or mitigation.
4. Publish an advisory after remediation or a coordinated disclosure decision.
5. Credit the reporter if they consent and it is safe to do so.

Response times are not guaranteed while the project is maintained by a small team or remains pre-release. If a report is not acknowledged, follow up through the same private channel.

## Security scope

Areas of particular interest include:
- XSS, unsafe HTML rendering, and hydration mismatches.
- CSRF, cookie/session handling, and session fixation.
- Authentication, authorization, privilege escalation, and server-action boundaries.
- Request parsing, routing, deserialization, request smuggling, and denial of service.
- SSRF, path traversal, file uploads, and static asset serving.
- SQL injection, transaction handling, and secret leakage.
- Client/server build separation and accidental inclusion of server-only code or secrets.
- Unsafe Rust, memory safety, and supply-chain compromise.
- CLI code generation that overwrites files or executes untrusted commands.

## Security expectations

- Never commit API tokens, passwords, private keys, production environment files, or user data.
- Use parameterized database queries.
- Escape HTML by default; raw HTML must be explicit and carefully reviewed.
- Treat browser input, request bodies, cookies, headers, and serialized client state as untrusted.
- Enforce authentication and authorization on the server, not only in the UI.
- Use maintained cryptographic and password-hashing libraries; do not implement cryptographic primitives.
- Keep dependencies updated and review security advisories.
- Add regression tests for fixed vulnerabilities.

## Disclosure and credits

Allow a reasonable period for assessment and remediation before public disclosure. Timing depends on severity, exploitability, downstream impact, and maintainer capacity. This policy does not promise a fixed deadline.

This policy does not authorize testing systems or data you do not own or have permission to assess.
