# ADR 0022: Advanced SQL relation loading and mutation contract

- Status: Proposed for review with the Phase 6.7.4 implementation.
- Phase: 6.7.4 — Relations and loading behavior (advanced extensions).
- Related: ADR 0020 (SQL ORM/Data Mapper architecture), ADR 0016 (SQLx pools and migrations), ADR 0019 (database support matrix), ORM_MODEL_CONTRACT.md and DATABASES.md.
- Boundary: SQLite, PostgreSQL, MySQL and MariaDB only. MongoDB remains governed by ADR 0021.

## Context

Phase 6.7.2 established the SQL relationship baseline on SeaORM 1.1.20: belongs-to, one-to-one, one-to-many and many-to-many metadata; explicit batch loaders; bounded query-count tests; and junction lifecycle/rollback coverage. Phase 6.7.4 must add richer per-relation options, projections, deeper configurable graphs and first-class one-to-one/one-to-many mutation helpers without replacing SeaORM, introducing another SQL renderer, or hiding I/O.

The one-pool boundary from ADR 0020 remains mandatory. Every operation accepts the existing DatabaseConnection or an explicit DatabaseTransaction. Models remain connection-free values. No global identity map, implicit relation fetch, or lazy I/O is introduced.

## Decision

### 1. Reuse SeaORM's EntityOrSelect and Select

Keep the existing RustFlow load_one/load_many/load_many_to_many helpers source-compatible for entity arguments, but generalize them to accept SeaORM EntityOrSelect inputs. Introduce RelationLoadOptions<E> as a small typed wrapper around Select<E>. It begins with E::find() and offers a closure that composes the native SeaORM filter/order/select/query API.

The related query is passed to LoaderTrait, so SeaORM still owns the relation join keys, SQL rendering and grouping. RustFlow does not add a dialect renderer or issue one query per parent.

Ordering is only deterministic when the caller specifies a complete order. Documentation examples add a unique tiebreaker such as the related primary key. A Select limit/offset applies to the complete related batch, not independently per parent. Per-parent top-N is deliberately not claimed by this API.

### 2. Use explicit DTOs for partial relationship projections

A partial SELECT must not be decoded into a full Entity Model with missing fields. Provide load_many_projected for has-many projections only. The caller supplies:

- the related entity's typed foreign-key column,
- the parent-key extractor,
- a DTO-key extractor for the same association key,
- a RelationLoadOptions query whose projection selects the DTO columns.

RustFlow adds a bound IN predicate for the whole parent slice, executes one query and groups the DTOs by the supplied related key while retaining query order and parent order. The two extractor functions must return the same key type. The related-key mapping must describe the selected foreign-key value; a wrong mapping is a caller contract error and can produce empty/misgrouped results. DTOs are Clone so duplicate parent keys can receive their grouped values.

Projected DTOs are not entity models and cannot be used as the parent of a nested entity relation graph in this phase. The graph can, however, store a projected has-many branch as a leaf.

### 3. Compose explicit, bounded relation graphs

RelationGraph<P, C> is a typed builder of named relation edges:

- with_many for has-many/belongs-to collection traversal,
- with_one for one-to-one/belongs-to optional traversal,
- with_many_to_many for a relation with an explicit junction entity,
- with_many_projected for a projected has-many leaf.

Every edge has its own RelationLoadOptions and may have child graph branches. Sibling names must be unique path segments. The graph executes only when load(parents, connection) is called. Collection and optional results are retrieved through typed many(path) and one(path) accessors in RelationGraphResults.

The default nesting depth is 8; the caller may configure a depth from 1 through MAX_RELATION_GRAPH_DEPTH (16). Each relation edge is one explicit batch hop; many-to-many retains its two-stage junction/target behavior. Nested result groups correspond to the flattened sequence from the preceding edge. An empty intermediate collection does not trigger downstream queries.

The graph does not promise a fixed total query count independent of graph shape: its count depends on edge kind and number of configured branches. It does promise that one edge does not issue a query per parent. No relationship is automatically loaded outside the explicit graph invocation.

### 4. Make relation mutation explicit and transactional

Expose typed helpers over a related Model and its typed foreign-key Column:

- attach_one_to_many reassigns an existing child to a specified parent key.
- detach_one_to_many sets a nullable foreign key to NULL; it never deletes the child.
- replace_one_to_one detaches an optional previous row and associates an existing replacement inside a caller-owned DatabaseTransaction.

The helpers check that the model has a complete primary key and delegate the typed update to SeaORM. They do not create/delete related rows, commit/rollback the transaction, or override database FK actions. A one-to-one relationship requires a nullable foreign key and a UNIQUE constraint. The caller must roll back when a multi-step helper returns an error and commit only after the complete mutation succeeds.

Junction attach/update/detach remains expressible through the explicit junction EntityRepository and existing transaction APIs; no implicit cascade helper is added.

## Consequences and limitations

- Existing Entity arguments remain valid for the loader helpers; filtered and ordered forms are additive.
- Related filters affect only the related rows, not the parent query.
- Limit/offset is global to the related batch, not per parent.
- Projected has-many loading requires caller-supplied, matching association-key extractors and a DTO containing the key.
- Projected graph nodes are leaves; deeper nested traversal continues from full SeaORM Model nodes.
- RelationGraph path access is type-checked at the call site through its generic downcast accessor; a wrong path/type returns None.
- Query counts scale with configured graph edges and edge kinds, not parent count.
- There is no lazy loading, implicit transaction, implicit delete, or generated database schema.
- MongoDB relationship/population behavior is not changed by this ADR.

## Verification required before acceptance

The Phase 6.7.4 PR must pass:

1. Unit/mock tests for filtered and ordered query generation, projected DTO grouping/ordering, graph path typing, duplicate/invalid sibling names, depth bounds and per-edge query counts.
2. Real integration coverage for SQLite, PostgreSQL, MySQL and MariaDB, including filters, deterministic ordering, projections, nested/multi-branch graphs, absent related rows, reparent/detach/replacement, unique/FK failures, and transaction rollback.
3. Formatting, Clippy with warnings denied, the complete workspace tests, explicit database integration targets, Rustdoc, stable and Rust 1.85, WASM, Browser E2E/bundle, website/docs, governance and dependency review.
4. Public documentation whose examples use only the exact committed API and state global limit/offset and projection-key constraints.

This ADR remains Proposed until the final PR head satisfies these checks and the decision is accepted.
