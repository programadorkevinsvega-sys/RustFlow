# ORM model metadata and SQL type contract (Phase 6.7.1)

**Status: architecture contract for review; runtime ORM mapping is not yet implemented.** The following table specifies intended representations and required checks. It is not a claim that current RustFlow can persist these types. Phase 6.7.1 publishes model metadata only.

## Model identity and metadata

- A model is a Rust struct with named fields. By default, its table name is the struct name converted to snake_case and each column name is the field identifier. The table/column attributes override those defaults.
- Table and column identifiers must match the ASCII grammar <code>[A-Za-z_][A-Za-z0-9_]*</code>. They are identifiers, not SQL fragments. Query rendering must quote them for the selected dialect; values must never go through identifier interpolation.
- One or more fields may declare a primary key, allowing composite keys. A key field cannot be nullable. A keyless view must explicitly declare read-only and is never writable.
- <code>Option&lt;T&gt;</code> is the sole nullable marker. Nested Option and reference fields are unsupported. SQL NULL decoded into a non-nullable field is an error, not a default value.
- A generated field is database-owned and its actual value must be retrieved after a successful write when supported. A default field declares a database default. Those markers are mutually exclusive in this contract. A normal model value does not track “not set”; the future insert payload must distinguish set, omit and use-default states.
- Models must have named fields and no generic parameters initially. Duplicate/unknown attributes, duplicate column names, invalid identifiers, unsupported types, missing primary keys on writable models, or nullable keys are compile-time errors.
- Indexes, unique constraints, foreign keys and check constraints stay with schema/migrations in 6.7.1. The derive does not create DDL.
- Metadata is a compile-time static description. It does not query the live schema, generate migrations or guarantee that an existing table matches the model.

## Rust logical-type matrix

Every cell is an **intended contract**, not tested runtime support. Checked means the ORM must reject out-of-range or invalid conversions rather than wrap, truncate or panic.

| Rust type/category | SQLite 3.46.0 | PostgreSQL 16.15 | MySQL 8.4.13 LTS | MariaDB 11.4.13 LTS | Contract / limitations |
|---|---|---|---|---|---|
| Signed integers i8/i16/i32/i64 | INTEGER, signed 64-bit storage | SMALLINT / INTEGER / BIGINT by declared width | TINYINT / SMALLINT / INT / BIGINT | TINYINT / SMALLINT / INT / BIGINT | Checked narrowing on decode; overflow behavior is not assumed identical. |
| Unsigned integers u8/u16/u32 | INTEGER, checked range | Signed integer storage, checked range | Unsigned type where supported | Unsigned type where supported | Values beyond target signed range are rejected on SQLite/PostgreSQL. |
| u64 | Excluded from portable numeric persistence | Excluded when exceeding signed BIGINT range | UNSIGNED BIGINT can represent full range | UNSIGNED BIGINT can represent full range | Metadata can classify it, but no portable representation exists across all four engines. Runtime mapping must reject unsupported values; never unwrap or cast with loss. |
| Floating point f32/f64 | REAL / floating-point semantics | REAL / DOUBLE PRECISION | FLOAT / DOUBLE | FLOAT / DOUBLE | Not exact decimal arithmetic; Decimal must never silently map to f64. |
| Boolean | INTEGER 0/1 convention; CHECK is explicit DDL | BOOLEAN | BOOLEAN / TINYINT(1) representation | BOOLEAN alias / TINYINT representation | Decoder accepts only documented representations; arbitrary nonzero is not automatically true. |
| String | TEXT | TEXT / VARCHAR | TEXT / VARCHAR | TEXT / VARCHAR | Encoding and collation depend on schema/engine. |
| Bytes Vec&lt;u8&gt; | BLOB | BYTEA | BLOB family, sized to schema | BLOB family, sized to schema | Size limits remain engine-specific. |
| Decimal / BigDecimal | No portable numeric implementation in the first codec; storing text alone does not preserve numeric SQL semantics | NUMERIC exact decimal | DECIMAL exact fixed-precision decimal | DECIMAL exact fixed-precision decimal | Portable query arithmetic excluded until SQLite strategy is tested; never pass through f64. |
| UUID | Canonical lowercase hyphenated TEXT | Native UUID | CHAR(36) canonical representation | CHAR(36) canonical representation | No binary/text reinterpretation; invalid values fail decoding. Runtime encoding pending. |
| Date | ISO-8601 YYYY-MM-DD TEXT | DATE | DATE | DATE | Strict calendar validation. |
| Time | Canonical time TEXT | TIME | TIME | TIME | Fractional precision is engine-specific and must be declared; no silent precision loss. |
| Timestamp without timezone | ISO-8601 TEXT with no-zone convention | TIMESTAMP WITHOUT TIME ZONE | DATETIME(6) | DATETIME(6) | Wall-clock time, not an instant. Local timezone types are excluded. |
| Timestamp with timezone / instant | ISO-8601 UTC TEXT ending in Z | TIMESTAMPTZ, normalized to UTC at API boundary | DATETIME(6) stored/returned under explicit UTC convention | DATETIME(6) stored/returned under explicit UTC convention | MySQL-family DATETIME does not preserve zone; normalize explicitly and test timezone/session settings. |
| Enum | TEXT label | TEXT by default; native PG enum excluded unless explicit dialect capability | VARCHAR/TEXT label | VARCHAR/TEXT label | Stable explicit labels, not Rust discriminants or Debug output. Unknown labels fail decoding. |
| JSON | TEXT; validity enforcement is schema-specific | JSONB by default | JSON | JSON alias/LONGTEXT behavior requires MariaDB-specific validation | JSON semantics/number representation/operators are not promised identical. |
| Nullable Option&lt;T&gt; | SQL NULL | SQL NULL | SQL NULL | SQL NULL | None maps to NULL only for nullable columns; NULL into T is a decode error. |
| Arrays/ranges/spatial/vendor-specific | Excluded from baseline | Excluded unless explicit feature/tests | Excluded unless explicit feature/tests | Excluded unless explicit feature/tests | Require explicit extension/type mapping, never automatic inference. |

## CRUD, generated values and mutation state

The selected Data Mapper design keeps model data separate from persistence.

- **Insert:** receives explicit insert values, including per-column set/omit/use-default state. Database-generated fields are omitted from submitted values. Success returns a new model with values actually returned by the database or safely identified by the driver. It cannot fabricate generated timestamps/defaults that it did not read.
- **Update:** targets the full primary key or an explicitly built predicate. It returns affected-row information and, when supported/requested, a new model snapshot. The caller's model is never mutated in place. Missing/stale records are not silently treated as success.
- **Delete:** targets the full primary key or an explicit safe predicate and returns affected-row information. No rows affected is distinct from query/connection failure.
- **Read one/many:** decoding validates nullability and range. Unknown columns may be ignored only for an explicitly projected partial model; full-model decode rejects missing required fields or incompatible data.
- **Defaults/generated fields:** defaults are not guessed in Rust. RETURNING differs (PostgreSQL, SQLite 3.46, MySQL and MariaDB); model operations negotiate per-engine behavior and may do a documented follow-up read when safe. Failure returns an error, not a success-shaped model.
- **State/concurrency:** no Active Record dirty state, identity map, lifecycle hook or Unit of Work. Ordinary updates are last-write-wins unless a later opt-in version-column contract is implemented and tested.
- **Transactions:** multi-step operations take an explicit transaction context. Caller chooses commit/rollback; operation failure propagates without implicit commit. Connection loss during commit can leave outcome unknown and must be reported as such.
- **Errors:** distinguish invalid metadata, value encode/decode, not found, constraint (unique/foreign key/check), pool/connection, query, concurrency and transaction failures. Public messages redact credentials and URLs and do not expose raw SQL/parameter values.
- **SQL escape hatch:** permit parameterized raw SQL and explicitly named engine-specific operations. Dynamic identifiers and user values may not be concatenated into SQL.
- **Schema/migrations:** retain SQLx migration management. Metadata does not auto-create/alter the schema, generate migrations or promise full rollback for MySQL/MariaDB DDL.

## Derive attributes added in 6.7.1

Example:

~~~
#[derive(rustflow::Model)]
#[model(table = "user_profiles")]
struct UserProfile {
    #[model(primary_key, generated)]
    id: i64,
    #[model(column = "display_name")]
    name: String,
    #[model(default)]
    nickname: Option<String>,
}
~~~

Supported model attributes: <code>table = "identifier"</code> and <code>read_only</code>. Supported field attributes: <code>column = "identifier"</code>, <code>primary_key</code>, <code>generated</code>, <code>default</code>, and <code>sql_type = "enum"</code> for a custom enum field. Unknown keys are errors. Attribute strings cannot contain SQL expressions.

## Deliberately not implemented in 6.7.1

- CRUD, row decoding/encoding, value binding, query execution and automatic DDL/migrations.
- Relation definitions, eager/batch loaders and query-count guarantees.
- Runtime codecs for UUID, Decimal, JSON, temporal values or optional Rust crates.
- Optimistic locking, lifecycle hooks, Unit of Work and hidden dirty tracking.
- MongoDB document derives/ODM.
- Production-ready ORM status or a claim of identical behavior across SQL engines.

These remain pending until their subphases have executable tests against the pinned versions. A logical type recognized by the metadata derive is not thereby a verified runtime capability.
