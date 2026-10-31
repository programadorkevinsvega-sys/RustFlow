# ADR 0021: MongoDB document mapper / ODM architecture

- Status: Accepted; Phase 6.7.3 passed all required checks on final PR #60 head `659f1750d611e57315263216167d43375da23574` and was squash-merged into `main` on 2026-10-31 as `7ffb89b5ca36017a428cf3f828b68f590f8d404b`.
- Phase: 6.7.3 — MongoDB document mapper / ODM.
- Related: ADR 0018 (MongoDB document adapter), ADR 0019 (database support matrix), ADR 0020 (SQL ORM architecture).
- Boundary: this ADR applies only to MongoDB and does not amend the SQL ORM contract.

## Context

RustFlow already owns one MongoDB adapter in rustflow-server, introduced in Phase 6.6.2. It owns the official driver's Client, selected Database, configuration, URI redaction, timeout defaults, BSON helpers, native collection handles, collection/index bootstrap and safe HTTP error mapping. The dependency is pinned to mongodb = 3.6.0 and BSON comes from that driver's re-export.

The workspace MSRV is Rust 1.85 and CI verifies Rust 1.85 and stable against MongoDB Server 7.0.45. The MongoDB driver's compatibility information lists 3.6 with Rust 1.85, while the 3.7 line raises the minimum Rust version to 1.88. Keep the existing exact 3.6.0 pin until a separate MSRV/dependency decision authorizes an upgrade. See the [driver compatibility table](https://www.mongodb.com/docs/drivers/rust/current/compatibility/) and [driver releases](https://github.com/mongodb/mongo-rust-driver/releases).

MongoDB is a document database. Its _id, BSON values, collections, update operators, aggregation pipeline, indexes, sessions and transaction deployment constraints must remain native MongoDB concepts. The SQL ORM selected by ADR 0020 is not a shared abstraction for this subsystem.

## Decision

### 1. Pattern: explicit Data Mapper with a RustFlow document-model trait

Build a small RustFlow-owned ODM facade over the existing official driver. Use an explicit document Data Mapper: model values contain no client, collection, session, identity map or hidden I/O. The application obtains a typed DocumentRepository<M> from the existing MongoDatabase; all persistence is explicit and asynchronous.

Use a public DocumentModel trait instead of a procedural derive in this subphase. The trait is the equivalent model contract and avoids creating a second macro/parser framework for metadata that MongoDB already represents through BSON and Serde. A model declares its collection and ID type, and provides explicit ID read/write access. Its Serde representation must map that ID to BSON _id. An application can use serde(rename = "_id"). When a model has no ID, RustFlow checks before insert that its associated Id type can deserialize a BSON ObjectId; only then does it omit _id and set the driver's inserted_id result on the returned model. Custom String/UUID IDs must be supplied explicitly. replace_one requires an existing model ID.

This trait is independent from ModelDiagnostics, SeaORM entity derives and EntityRepository. It does not introduce SQL table/column metadata, ActiveModel state, foreign keys or relational relationships. A later derive may be considered only if it can validate additional compile-time information beyond this trait without duplicating Serde's mapping rules.

### 2. Dependency and driver strategy

Keep using the existing exact MongoDB Rust driver dependency and its BSON re-export. Do not add a second client, pool, driver version, BSON version, async runtime or serialization layer. The ODM remains in rustflow-server; the browser/WASM crates must not gain MongoDB dependencies.

A review of the maintained mongodm crate found that its current manifest enables the MongoDB driver's dns-resolver feature and does not declare a Rust 1.85 MSRV. That would union the DNS SRV feature back into RustFlow's existing driver feature graph, contrary to the explicit security/dependency boundary in ADR 0018, and would not independently establish the required MSRV. It is therefore not adopted. See the [mongodm manifest](https://docs.rs/crate/mongodm/latest/source/Cargo.toml.orig).

Do not enable mongodb+srv:// as part of this work. The resolver remains disabled until its dependency/advisory impact is separately reviewed and accepted. Keep mongodb:// as the supported URI scheme.

### 3. Typed document model and IDs

The model trait contract is:

- Serialize + DeserializeOwned + Clone + Send + Sync + 'static for the model.
- An associated Id type that is BSON/Serde serializable and deserializable, cloneable and thread-safe.
- A static collection name.
- Explicit methods to read and set the model ID.
- The serialized ID field maps to _id; before writes RustFlow verifies that a supplied model ID matches the BSON _id value and rejects a model whose ID does not serialize under _id. For inserts with a missing ID, RustFlow first proves the associated Id type can deserialize an ObjectId, then omits _id and sets the returned driver-generated ID on the value returned to the caller. Custom ID types must be supplied explicitly; replacement requires an explicit model ID.
- Caller-supplied IDs are retained. A duplicate _id is a duplicate-key error, not an overwrite.
- IDs are values, not relationships. ObjectId, strings, UUIDs and other BSON-compatible IDs are supported only when the chosen BSON Serde representation and the model's ID type agree. No implicit string/ObjectId coercion is performed.

Serde remains responsible for nested embedded values, arrays, optional fields, defaults, skipped fields and custom serializers. Plain Option<T> generally maps both an absent field and explicit BSON null to None; it does not preserve the distinction by itself. When an application must distinguish missing, null, defaulted and present values, it must use an explicit tri-state/custom Serde type or inspect raw BSON. Projections should deserialize into dedicated DTOs whose required fields match the projection; they must not be deserialized into an incomplete full model by assumption.

### 4. Repository and query contract

DocumentRepository<M> uses the collection declared by M and the already configured MongoDatabase. Its intended operations are:

- Read by ID, one result by filter, many results, count and existence.
- Insert and insert-many, replace, update-one/update-many, delete-one/delete-many.
- Return the database-generated ID on inserted models and explicit matched/modified/deleted/upserted result data for writes.
- Preserve driver write errors, including ordered/unordered insert-many partial failure details. A failed bulk write may have applied some writes; no all-or-nothing guarantee is implied.
- Offer typed projections into caller-supplied DTO types and typed aggregation outputs while keeping pipeline stages native MongoDB documents.

Filters and updates are MongoDB-native, not a port of SeaORM or SQL. The composable helper validates field paths and converts values through BSON/Serde; it does not interpolate request strings into query source. It covers ordinary equality/comparison, boolean composition, existence/null, membership, array predicates, nested dotted paths, sorting, skip/limit, projection and common update operators. An explicit native BSON escape hatch remains available for MongoDB operators not covered by the helper. Use it only with trusted, deliberately constructed BSON; callers remain responsible for its semantics and authorization scope.

No implicit relation or lazy-loading API is provided. Embedded values use nested BSON documents/arrays. References are explicit IDs (or application-defined DBRefs); applications issue explicit reads or aggregation pipelines such as $lookup when appropriate. A reference does not cause an automatic query when accessed, and no ORM-style cascade or N+1 behavior is inferred.

### 5. Index and collection lifecycle

Continue to use DocumentBootstrap as an explicit, repeatable collection/index declaration. A declared compound index is a native ordered BSON key document. Verification must compare the declared key document and every option RustFlow claims to manage (at least name and uniqueness), not just the name. `_id` is immutable: ODM update helpers reject updates targeting `_id` or its nested paths. Existing incompatible resources must cause a clear conflict/error; bootstrap must never silently drop or replace an index. TTL, sparse, partial, collation, wildcard and other index options may only be advertised when represented, compared and integration-tested. No BSON schema migration/versioning system is implied.

### 6. Sessions and transaction semantics

Expose a session from the same MongoDatabase client and require an explicit &mut ClientSession parameter for repository operations that participate in a transaction. Pass that session to every driver action through the official driver's session builder. Parallel operations within one session transaction are unsupported.

A MongoDB standalone server does not support multi-document transactions. A replica set or sharded cluster is required. The CI integration service must therefore start MongoDB 7.0.45 as a single-node replica set and prove both commit and abort/rollback against that real server before RustFlow advertises transaction coverage. See the [MongoDB server transaction requirements](https://www.mongodb.com/docs/manual/core/transactions-production-consideration/) and [Rust driver transactions guide](https://www.mongodb.com/docs/drivers/rust/current/crud/transactions/).

Use the driver's session/transaction state machine; do not emulate transactions using sequential writes. Preserve driver error labels. In particular, UnknownTransactionCommitResult means the application cannot infer whether the commit satisfied the requested write concern. The driver documents that the commit may be retried while that label remains present; RustFlow must not blindly rerun a non-idempotent transaction callback. Retry behavior and idempotency remain explicit application responsibilities unless a future wrapper deliberately adopts and tests the driver's convenient transaction API.

Transactions provide deployment-dependent multi-document atomicity, not a blanket guarantee for standalone servers. A single-document write's atomicity is distinct from a multi-document transaction. Transaction read/write concern, causal consistency, topology, session lifetime, retry limits and commit uncertainty must be documented.

### 7. Errors, timeouts and safety

Keep MongoError as the shared adapter/ODM error envelope and preserve the official driver error as its source. Distinguish invalid model/query/update inputs, not-found results, duplicate keys, incompatible indexes, connection/selection failures, operation failures, timeouts and task failures without exposing connection URIs, credentials, document bodies or raw driver messages in Debug or HTTP responses. HTTP 409 is reserved for duplicate-key errors that are positively identified. Map connection/timeout unavailability to 503 where appropriate.

A timeout does not prove that the server did not execute a write. The driver documentation also warns that dropping an in-flight driver future can leave driver internals inconsistent; owned ODM actions should therefore be driven to completion in a spawned task when enforcing a caller timeout, and a timeout response must explain that the write outcome may be unknown and should be reconciled by the application. Session operations that borrow a mutable session need a separately documented lifetime/cancellation policy.

### 8. Out of scope

This ADR does not add SQL-style joins or relationships, implicit lazy loading, SQL migrations, a second Mongo client/pool, mongodb+srv://, schema migrations, automatic cross-document cascades, an identity map, a global repository singleton, or global Phase 6.7 completion. Advanced generated CRUD applications and the remaining global quality gate stay in later subphases.

## Implementation and acceptance record (2026-10-31)

The trait-based ODM described in this ADR is implemented in `crates/rustflow-server/src/document_odm.rs` and verified by the integration suite in `crates/rustflow-server/tests/mongodb_adapter.rs`. The exact final PR #60 head `659f1750d611e57315263216167d43375da23574` passed Rust 1.85 and stable, formatting, Clippy, workspace tests, four-engine SQL regressions, MongoDB CRUD/bulk/query/index/ID/nested-document tests, replica-set commit/abort, standalone transaction rejection, Rustdoc, CLI and benchmark smoke, WebAssembly, Browser E2E/bundle, Documentation site, Governance and Dependency review. PR #60 merged into `main` on 2026-10-31 as `7ffb89b5ca36017a428cf3f828b68f590f8d404b`; this merge does not imply global Phase 6.7 completion.

## Consequences

The chosen architecture keeps RustFlow's dependency graph and MSRV stable, reuses the single existing driver/client, preserves the document model's native BSON semantics and provides an explicit repository/query layer without importing SQL ORM concepts. The trade-off is that field-name correctness is validated at runtime by the composable helper rather than generated from arbitrary Rust struct field identifiers; compile-time macro diagnostics are not part of this trait-based design.
