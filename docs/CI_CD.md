# CI/CD and path-based validation

RustFlow uses path-based job conditions so a pull request validates the areas it changes instead of running every expensive suite for every edit.

The workflows still start for every pull request. They classify the changed files and conditionally skip individual jobs. This distinction is intentional: a workflow skipped by a workflow-level path filter can leave a required status check pending, while a job skipped by an `if` condition reports a successful skipped result. See the [GitHub required-status-check guidance](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks#handling-skipped-but-required-checks).

## Change scopes

The shared source of truth is `.github/filters.yml`. The CI, website, and dependency-review workflows consume the same path classification.

| Scope | Files that select it | Jobs affected |
| --- | --- | --- |
| `rust` | Workspace and crate manifests/lockfiles, Rust toolchain/Cargo configuration, `crates/**`, Rust example sources and manifests | `Rust checks (1.85)`, `Rust checks (stable)` |
| `browser` | Browser example, DOM/view/runtime/macro crates, and workspace Cargo/toolchain configuration | `WebAssembly target check`, `Browser E2E and bundle size` |
| `website` | `website/**` and the website workflow definition | `Build documentation` |
| `dependencies` | Cargo/npm manifests and lockfiles, GitHub Actions workflow/action definitions | `Review dependency changes` |
| `full_ci` | `.github/workflows/**`, `.github/actions/**`, and `.github/filters.yml` | Forces the Rust, WASM, and browser jobs to run |

Governance checks are intentionally unconditional and remain fast. Website validation is independent from Rust validation. Changes to the public website do not run the Rust matrix; changes limited to backend crates do not run WASM/browser checks or rebuild the website.

The browser example's own JavaScript, HTML, CSS, package, script, or Playwright changes run the WASM/browser validation path. They do not trigger the two-toolchain Rust workspace matrix unless a Rust crate, Rust source, or Cargo manifest also changes. The browser build still compiles its Rust-to-WASM application as required by the browser E2E contract.

## Required-check safety

Keep the existing required job names stable. Do not add workflow-level `paths` or `paths-ignore` filters to workflows that publish required checks.

Each expensive job uses `always()` with its change-detection dependency. If path detection fails, the affected workflow runs the conservative fallback (Rust + WASM + browser, website build, or dependency review) rather than silently skipping validation. The change-detection job needs `contents: read` and `pull-requests: read`; the latter is needed to read the changed-file list for pull requests.

A change to the filters or any workflow definition deliberately runs the broad CI path so changes to the pipeline itself receive full validation. When adding a new crate or browser dependency, review `.github/filters.yml` and update these scope descriptions in the same pull request.

## Cloudflare Pages

The Pages project is connected to GitHub outside this repository. GitHub Actions path filters do not control Cloudflare's independent Git integration.

Configure the Pages project in Cloudflare Dashboard under **Workers & Pages → the RustFlow Pages project → Settings → Build → Build watch paths**:

- **Include paths:** `website/*`
- **Exclude paths:** leave empty unless a narrower exclusion is intentionally required.

Cloudflare documents that `*` matches nested path separators and that a build is triggered when at least one changed path matches the include rules: [Build watch paths](https://developers.cloudflare.com/pages/configuration/build-watch-paths/).

This dashboard setting cannot be committed in this repository. Until it is applied and verified in Cloudflare, Git-connected Pages builds may still start for changes outside `website/`. A push with 3,000 or more changed files or 20 or more commits bypasses Cloudflare path matching and triggers a build, per its documented behavior.

## Expected behavior examples

| Pull request changes | Rust matrix | WASM / Browser E2E | Astro site build | Dependency review |
| --- | ---: | ---: | ---: | ---: |
| `README.md`, `ROADMAP.md`, or `docs/**` only | Skip | Skip | Skip | Skip |
| `website/**` only | Skip | Skip | Run | Skip |
| `crates/rustflow-server/**` only | Run | Skip | Skip | Skip |
| `crates/rustflow-dom/**` | Run | Run | Skip | Skip |
| `examples/browser/src/**` or `examples/browser/tests/**` | Skip | Run | Skip | Skip |
| Cargo/npm manifest, lockfile, or Actions workflow changes | Per matching scopes; workflow changes force broad CI | Per matching scopes; workflow changes force broad CI | If website or docs workflow changed | Run |

All checks required by the `main` ruleset retain their existing names. A skipped job is successful for required-check evaluation; a failed change detector activates the conservative fallback instead of relying on a skipped required check.
