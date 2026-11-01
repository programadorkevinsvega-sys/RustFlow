# Third-party GitHub Actions

The dependency-review API currently reports GitHub Actions dependencies with missing license metadata (license: null), even when their upstream repositories declare a license. Do not treat this as evidence that the license is unknown at source.

The workflow excludes only the exact action versions below from automated license evaluation via allow-dependencies-licenses. Their licenses were checked against the upstream repositories' LICENSE files:

| Action | Pinned version | Upstream license |
| --- | --- | --- |
| actions/checkout | 7.0.1 | MIT |
| actions/dependency-review-action | 5.0.0 | MIT |
| dorny/paths-filter | 4.0.3 | MIT |

Sources:
- https://github.com/actions/checkout/blob/v7.0.1/LICENSE
- https://github.com/actions/dependency-review-action/blob/v5.0.0/LICENSE
- https://github.com/dorny/paths-filter/blob/v4.0.3/LICENSE

When upgrading any listed action, review the new version's LICENSE file and update both this inventory and the exact-version allow-dependencies-licenses entries in .github/workflows/dependency-review.yml in the same change. Do not add broad, unversioned exclusions. License checks remain enabled for other dependencies, and vulnerability checks remain enabled for all dependencies.
