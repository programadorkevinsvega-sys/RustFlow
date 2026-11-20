# CI/CD and path-based validation

RustFlow uses path-based job conditions so a pull request validates the areas it changes instead of running every expensive suite for every edit. Pull requests in Draft also defer the expensive Rust, WASM, browser, and cross-browser jobs until the author marks the pull request ready for review.

The workflows still start for pull requests and classify the changed files. While a pull request is Draft, lightweight governance, dependency, documentation, and performance-tool checks continue when applicable; the expensive Rust matrix, WASM build, browser E2E/bundle/benchmarks, and cross-browser stress profile are skipped. The required Rust scope gates explicitly report that validation is deferred in Draft; a Draft pull request cannot be merged.

The `ready_for_review` event runs the expensive jobs for the final candidate. Further pushes to a non-Draft pull request run the applicable jobs again, so any fixes made after marking it ready are revalidated before merge. Keep active work in Draft until it is ready for that final CI pass. A workflow-level path filter can leave a required status check pending, while a job skipped by an `if` condition reports a successful skipped result. See the [GitHub required-status-check guidance](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks#handling-skipped-but-required-checks).

## Change scopes

The shared source of truth is `.github/filters.yml`. The CI, website, and dependency-review workflows consume the same path classification.

| Scope | Files that select it | Jobs affected |
| --- | --- | --- |
| `rust` | Workspace and crate manifests/lockfiles, Rust toolchain/Cargo configuration, `crates/**`, Rust example sources and manifests | Rust test matrix plus the required-check gates `Rust checks (1.94)` and `Rust checks (stable)` |
| `browser` | Browser example, DOM/view/runtime/macro crates, and workspace Cargo/toolchain configuration | `WebAssembly target check`, `Browser E2E and bundle size` |
| `performance_tools` | Performance benchmark comparators, aggregators, patchers, budget checkers, and `scripts/tests/**` | `Performance tooling tests` |
| `website` | `website/**` and the website workflow definition | `Build documentation` |
| `dependencies` | Cargo/npm manifests and lockfiles, GitHub Actions workflow/action definitions | `Review dependency changes` |
| `full_ci` | `.github/workflows/**`, `.github/actions/**`, and `.github/filters.yml` | Forces Rust, WASM, browser, and performance tooling test jobs to run |

Governance checks are intentionally unconditional and remain fast. Website validation is independent from Rust validation. Changes to the public website do not run the Rust matrix; changes limited to backend crates do not run WASM/browser checks or rebuild the website.

The browser example's own JavaScript, HTML, CSS, package, script, or Playwright changes run the WASM/browser validation path. They do not trigger the two-toolchain Rust workspace matrix unless a Rust crate, Rust source, or Cargo manifest also changes. The browser build still compiles its Rust-to-WASM application as required by the browser E2E contract.

Performance tooling is validated separately from the browser runtime: edits to the comparator, upstream aggregator/patcher, budget-check scripts, or `scripts/tests/**` run Python syntax checks and the complete standard-library test suite in `Performance tooling tests`. This keeps script-only changes from being silently skipped when they do not touch browser runtime paths.

## Required-check safety

Do not add workflow-level `paths` or `paths-ignore` filters to workflows that publish required checks. The Rust test matrix is conditional; two lightweight, always-emitted required-check gates retain the branch-protection contexts `Rust checks (1.94)` and `Rust checks (stable)`.

Each expensive job uses `always()` with its change-detection dependency. If path detection fails, the affected workflow runs the conservative fallback (Rust + WASM + browser + performance tooling, website build, or dependency review) rather than silently skipping validation, and the Rust scope gates fail closed. For documentation-only, frontend-only, or other non-Rust changes, the Rust gates pass without requiring backend tests. In Draft, the Rust gates explicitly defer the matrix; on `ready_for_review` and later non-Draft pushes, Rust or `full_ci` scope requires the complete Rust matrix to succeed. The change-detection job needs `contents: read` and `pull-requests: read`; the latter is needed to read the changed-file list for pull requests.

A change to the filters or any workflow definition deliberately selects the broad CI path. Draft pull requests still defer its expensive jobs; marking the pull request ready runs that broad path so changes to the pipeline itself receive full validation. When adding a new crate or browser dependency, review `.github/filters.yml` and update these scope descriptions in the same pull request.

## Cloudflare Pages

The Pages project is connected to GitHub outside this repository. GitHub Actions path filters do not control Cloudflare's independent Git integration.

Configure the Pages project in Cloudflare Dashboard under **Workers & Pages → the RustFlow Pages project → Settings → Build → Build watch paths**:

- **Include paths:** `website/*`
- **Exclude paths:** leave empty unless a narrower exclusion is intentionally required.

Cloudflare documents that `*` matches nested path separators and that a build is triggered when at least one changed path matches the include rules: [Build watch paths](https://developers.cloudflare.com/pages/configuration/build-watch-paths/).

This dashboard setting cannot be committed in this repository. Until it is applied and verified in Cloudflare, Git-connected Pages builds may still start for changes outside `website/`. A push with 3,000 or more changed files or 20 or more commits bypasses Cloudflare path matching and triggers a build, per its documented behavior.

## Expected behavior examples

| Pull request changes (Ready for review) | Rust tests / required gates | WASM / Browser E2E | Astro site build | Dependency review | Performance tooling tests |
| --- | ---: | ---: | ---: | ---: | ---: |
| `README.md`, `ROADMAP.md`, or `docs/**` only | Matrix skips; both gates pass | Skip | Skip | Skip | Skip |
| `website/**` only | Matrix skips; both gates pass | Skip | Run | Skip | Skip |
| `crates/rustflow-server/**` only | Run; both gates require success | Skip | Skip | Skip | Skip |
| `crates/rustflow-dom/**` | Run; both gates require success | Run | Skip | Skip | Skip |
| `examples/browser/src/**` or `examples/browser/tests/**` | Matrix skips; both gates pass | Run | Skip | Skip | Skip |
| `scripts/compare-browser-performance.py` or `scripts/tests/**` | Matrix skips; both gates pass | Skip | Skip | Skip | Run |
| Cargo/npm manifest, lockfile, or Actions workflow changes | Per matching scopes; workflow changes force broad CI | Per matching scopes; workflow changes force broad CI | If website or docs workflow changed | Run | Run only when performance tools also change or broad CI is forced |

For all rows above, a Draft pull request defers the Rust matrix, WASM, Browser E2E/bundle/benchmarks, and cross-browser profile. Lightweight applicable checks still run. The broad `full_ci` scope is enforced when the pull request is marked ready.

The `main` ruleset continues to require `Rust checks (1.94)` and `Rust checks (stable)`, but these are now stable scope gates rather than conditionally skipped matrix jobs. A skipped Rust matrix is accepted only when the change detector succeeded and Rust is not in scope; a failed detector activates the conservative fallback and fails the gates.

## CLI integration test cost

The workspace matrix runs `cli_integration` only on the `stable` toolchain. The Rust 1.94 matrix runs the workspace suite while skipping that integration binary, because generated CLI projects compile against RustFlow rather than validating a toolchain-specific behavior.

CLI integration tests share `RUSTFLOW_CLI_TEST_TARGET_DIR` so generated server and CRUD projects reuse compiled dependencies within the stable matrix job. GitHub Actions caches that directory with `Swatinem/rust-cache`; local runs use a process-independent directory under the system temporary directory when the environment variable is not set.

## Benchmark triggers

Browser and framework benchmarks do not run as a required step of every normal PR push. The official framework benchmark has three triggers:

- **Manual:** run the `Official framework benchmarks` workflow from Actions.
- **PR comment:** authorized contributors can comment `/bench` on an open PR whose branch belongs to this repository.
- **Scheduled:** run the pinned comparison weekly on `main`.

Fork PRs cannot dispatch benchmarks from a comment. A comment request validates the PR state and head repository before calling `workflow_dispatch`; it cannot execute code from the commenting user or from an external fork. Scheduled and manual runs remain explicit benchmark moments and do not block the ordinary functional CI path.

## SQLx patch and dependency warnings

`cargo install --path crates/rustflow-cli --locked` prints unused-patch warnings for the workspace-level SQLx git patches. The patches are inherited by Cargo when it resolves the workspace, but the CLI executable itself does not link SQLx or SeaORM. The warnings are therefore contextual rather than evidence that the server patch is broken. The patch table must remain at the workspace root because Cargo ignores `[patch]` tables in a non-root member manifest; the RustFlow server graph is verified to use the patched `sqlx`, `sqlx-core`, `sqlx-mysql`, `sqlx-postgres`, and `sqlx-sqlite` packages for the bundled SQLite advisory remediation.

The browser dependency graph also uses `rustflow-dom` and `rustflow` directly. `rustflow-browser` imports both crates, and `rustflow-dom` imports `rustflow` for components and reactivity. They are not unused manifest entries.

## Fast local feedback

During active development, run the focused checks for the area being changed:

~~~sh
cargo fmt --all -- --check
cargo check -p rustflow-server
cargo test -p rustflow-server
cargo clippy -p rustflow-server --all-targets --all-features -- -D warnings
python3 -m py_compile scripts/compare-browser-performance.py
~~~

Use the affected crate in place of `rustflow-server`; add a test-name filter to `cargo test` to narrow the run further. Run `py_compile` on the changed Python script only. Before marking a subphase ready, run the applicable full test suite once; then use the Ready-for-review CI run as the final multi-toolchain, browser, bundle, and performance gate. These local commands do not replace the required GitHub checks.
