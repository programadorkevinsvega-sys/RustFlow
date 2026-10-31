use std::{
    env,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use http::StatusCode;
use rustflow_server::mongodb_driver::bson::{Document, doc, oid::ObjectId};
use rustflow_server::{
    DocumentBootstrap, DocumentIndex, MongoConfig, MongoDatabase, MongoError, from_bson_document,
    to_bson_document,
};
use serde::{Deserialize, Serialize};
use rustflow_server::{
    DocumentDeleteOutcome, DocumentFilter, DocumentModel, DocumentQuery, DocumentRepository,
    DocumentUpdate,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct OdmEntry {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<ObjectId>,
    title: String,
    score: i32,
    tags: Vec<String>,
}

impl DocumentModel for OdmEntry {
    type Id = ObjectId;
    const COLLECTION: &'static str = "odm_entries";

    fn id(&self) -> Option<Self::Id> {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = Some(id);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct NamedOdmEntry {
    #[serde(rename = "_id")]
    id: String,
    title: String,
}

impl DocumentModel for NamedOdmEntry {
    type Id = String;
    const COLLECTION: &'static str = "named_odm_entries";

    fn id(&self) -> Option<Self::Id> {
        Some(self.id.clone())
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id;
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct OdmEntrySummary {
    #[serde(rename = "_id")]
    id: ObjectId,
    title: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct OdmScoreGroup {
    #[serde(rename = "_id")]
    score: i32,
    entries: i32,
}

fn odm_entry(title: &str, score: i32, tags: &[&str]) -> OdmEntry {
    OdmEntry {
        id: None,
        title: title.to_owned(),
        score,
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Address {
    city: String,
    coordinates: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Profile {
    #[serde(rename = "_id")]
    id: ObjectId,
    email: String,
    address: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    nickname: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct NumericDocument {
    count: i32,
}

fn test_uri() -> Option<String> {
    match env::var("RUSTFLOW_TEST_MONGODB_URI") {
        Ok(uri) if !uri.trim().is_empty() => Some(uri),
        _ if env::var("CI").is_ok_and(|value| value == "true") => {
            panic!("RUSTFLOW_TEST_MONGODB_URI must be set in CI");
        }
        _ => None,
    }
}

fn standalone_test_uri() -> Option<String> {
    match env::var("RUSTFLOW_TEST_MONGODB_STANDALONE_URI") {
        Ok(uri) if !uri.trim().is_empty() => Some(uri),
        _ if env::var("CI").is_ok_and(|value| value == "true") => {
            panic!("RUSTFLOW_TEST_MONGODB_STANDALONE_URI must be set in CI");
        }
        _ => None,
    }
}

fn database_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after UNIX epoch")
        .as_nanos();
    format!("rustflow_phase662_{}_{}", std::process::id(), nanos)
}

fn invalid_password_uri(uri: &str) -> Option<String> {
    let (scheme, remainder) = uri.split_once("://")?;
    let (credentials, host_and_suffix) = remainder.split_once('@')?;
    let (username, _) = credentials.split_once(':')?;
    Some(format!(
        "{scheme}://{username}:invalid-phase662-password@{host_and_suffix}"
    ))
}

#[tokio::test]
async fn mongodb_7045_real_server_contract() {
    let Some(uri) = test_uri() else {
        eprintln!(
            "Skipping MongoDB integration test outside CI: RUSTFLOW_TEST_MONGODB_URI is unset"
        );
        return;
    };

    let database_name = database_name();
    let config = MongoConfig::new(uri.clone(), database_name.clone())
        .expect("valid MongoDB integration URI")
        .with_connect_timeout(Duration::from_secs(3))
        .expect("valid connect timeout")
        .with_server_selection_timeout(Duration::from_secs(3))
        .expect("valid server-selection timeout")
        .with_operation_timeout(Duration::from_secs(5))
        .expect("valid operation timeout");

    let debug = format!("{config:?}");
    assert!(
        !debug.contains(&uri),
        "MongoConfig Debug must redact the URI"
    );
    assert!(!debug.contains("root:root"), "credentials must be redacted");

    let database = MongoDatabase::connect(&config)
        .await
        .expect("connect to real MongoDB service");
    assert_eq!(database.database_name(), database_name);

    let build_info = database
        .run_with_timeout(database.database().run_command(doc! { "buildInfo": 1 }))
        .await
        .expect("read MongoDB server build info");
    let version = build_info
        .get_str("version")
        .expect("buildInfo contains a server version");
    assert!(
        version == "7.0.45",
        "CI contract expects MongoDB 7.0.45, got {version}"
    );

    let collection_name = "profiles";
    let bootstrap = DocumentBootstrap::new(collection_name)
        .expect("valid collection name")
        .with_index(
            DocumentIndex::new("email_unique", doc! { "email": 1 }, true)
                .expect("valid unique index"),
        )
        .expect("unique index declaration");
    database
        .bootstrap(&bootstrap)
        .await
        .expect("first bootstrap");
    database
        .bootstrap(&bootstrap)
        .await
        .expect("repeat bootstrap is safe");
    let incompatible_index = DocumentBootstrap::new(collection_name)
        .expect("valid collection")
        .with_index(
            DocumentIndex::new("email_unique", doc! { "email": 1 }, false)
                .expect("valid but incompatible index declaration"),
        )
        .expect("valid index declaration");
    let index_conflict = database
        .bootstrap(&incompatible_index)
        .await
        .expect_err("bootstrap must reject an index with incompatible uniqueness");
    assert!(matches!(index_conflict, MongoError::Bootstrap(_)));

    let collection = database.collection::<Profile>(collection_name);
    let profile = Profile {
        id: ObjectId::new(),
        email: "ada@example.test".to_owned(),
        address: Address {
            city: "Medellin".to_owned(),
            coordinates: vec![-75.56, 6.25],
        },
        nickname: None,
    };

    let bson_document = to_bson_document(&profile).expect("serialize typed document");
    assert!(!bson_document.contains_key("nickname"));
    let round_trip: Profile =
        from_bson_document(bson_document).expect("deserialize typed document");
    assert_eq!(round_trip, profile);

    database
        .run_with_timeout(collection.insert_one(profile.clone()))
        .await
        .expect("insert typed document");

    let fetched = database
        .run_with_timeout(collection.find_one(doc! { "_id": profile.id }))
        .await
        .expect("find by ObjectId")
        .expect("inserted document exists");
    assert_eq!(fetched.email, profile.email);
    assert_eq!(fetched.address.city, "Medellin");
    assert_eq!(fetched.address.coordinates.len(), 2);
    assert_eq!(fetched.nickname, None);

    let filtered = database
        .run_with_timeout(collection.find_one(doc! { "address.city": "Medellin" }))
        .await
        .expect("query with native MongoDB filter")
        .expect("nested filter matched the document");
    assert_eq!(filtered.id, profile.id);

    database
        .run_with_timeout(collection.update_one(
            doc! { "_id": profile.id },
            doc! { "$set": { "email": "updated@example.test" } },
        ))
        .await
        .expect("update document");

    let updated = database
        .run_with_timeout(collection.find_one(doc! { "_id": profile.id }))
        .await
        .expect("read updated document")
        .expect("updated document exists");
    assert_eq!(updated.email, "updated@example.test");

    let duplicate = Profile {
        id: ObjectId::new(),
        email: "updated@example.test".to_owned(),
        address: profile.address.clone(),
        nickname: Some("duplicate".to_owned()),
    };
    let duplicate_error = database
        .run_with_timeout(collection.insert_one(duplicate))
        .await
        .expect_err("unique email index rejects duplicate values");
    assert_eq!(
        duplicate_error.to_http_error().status_code(),
        StatusCode::CONFLICT
    );
    assert!(
        std::error::Error::source(&duplicate_error).is_some(),
        "the driver error remains available as a source"
    );
    assert!(!format!("{duplicate_error:?}").contains(&uri));

    let index_names = database
        .run_with_timeout(collection.list_index_names())
        .await
        .expect("list collection indexes");
    assert!(index_names.iter().any(|name| name == "email_unique"));

    let count = database
        .run_with_timeout(collection.count_documents(doc! { "_id": profile.id }))
        .await
        .expect("count inserted document");
    assert_eq!(count, 1);

    database
        .run_with_timeout(collection.delete_one(doc! { "_id": profile.id }))
        .await
        .expect("delete document");
    let deleted_count = database
        .run_with_timeout(collection.count_documents(doc! { "_id": profile.id }))
        .await
        .expect("verify delete");
    assert_eq!(deleted_count, 0);

    let type_error = from_bson_document::<NumericDocument>(doc! { "count": "not-an-integer" })
        .expect_err("invalid BSON field type is rejected");
    assert!(matches!(type_error, MongoError::Deserialization(_)));
    assert!(std::error::Error::source(&type_error).is_some());

    if let Some(bad_uri) = invalid_password_uri(&uri) {
        let bad_config = MongoConfig::new(bad_uri, database_name.clone())
            .expect("invalid credentials URI is syntactically valid")
            .with_server_selection_timeout(Duration::from_secs(3))
            .expect("valid timeout");
        let bad_credentials =
            tokio::time::timeout(Duration::from_secs(6), MongoDatabase::connect(&bad_config))
                .await
                .expect("invalid credentials check is bounded")
                .expect_err("invalid credentials must not connect");
        assert!(!format!("{bad_credentials}").contains("invalid-phase662-password"));
        assert!(!format!("{bad_credentials:?}").contains("invalid-phase662-password"));
    }

    database
        .run_with_timeout(database.database().drop())
        .await
        .expect("clean up isolated test database");
}

#[tokio::test]
async fn mongodb_typed_odm_crud_bulk_filters_updates_projection_and_aggregation() {
    let Some(uri) = test_uri() else {
        eprintln!("Skipping typed ODM integration test outside CI: MongoDB URI is unset");
        return;
    };
    let database_name = database_name();
    let config = MongoConfig::new(uri, database_name.clone())
        .expect("valid MongoDB integration URI")
        .with_operation_timeout(Duration::from_secs(5))
        .expect("valid operation timeout");
    let database = MongoDatabase::connect(&config)
        .await
        .expect("connect to MongoDB replica set");
    let repository = database.repository::<OdmEntry>();
    assert_eq!(repository.collection_name(), "odm_entries");

    let inserted = repository
        .insert(odm_entry("alpha", 10, &["rust", "database"]))
        .await
        .expect("insert typed document");
    let alpha_id = inserted.id.expect("server generated ObjectId");
    assert_eq!(
        repository.find_by_id(alpha_id).await.expect("find by id"),
        Some(inserted.clone())
    );

    let named_repository = database.repository::<NamedOdmEntry>();
    let named = named_repository
        .insert(NamedOdmEntry {
            id: "stable-user-key".to_owned(),
            title: "custom id".to_owned(),
        })
        .await
        .expect("insert custom string ID");
    assert_eq!(
        named_repository
            .find_by_id("stable-user-key".to_owned())
            .await
            .expect("find string ID"),
        Some(named),
    );

    let bulk = repository
        .insert_many(
            vec![
                odm_entry("beta", 20, &["rust"]),
                odm_entry("gamma", 30, &["wasm"]),
            ],
            true,
        )
        .await
        .expect("ordered bulk insert");
    assert!(bulk.iter().all(|entry| entry.id.is_some()));

    let bulk_update = repository
        .update_many(
            DocumentFilter::new()
                .in_values("title", ["beta", "gamma"])
                .expect("bulk update filter"),
            DocumentUpdate::new()
                .set("batch_mark", true)
                .expect("bulk update"),
            false,
        )
        .await
        .expect("update many documents");
    assert_eq!(bulk_update.matched_count, 2);
    assert_eq!(bulk_update.modified_count, 2);

    let duplicate_id = ObjectId::new();
    let partial_error = repository
        .insert_many(
            vec![
                OdmEntry {
                    id: Some(duplicate_id),
                    title: "bulk-first".to_owned(),
                    score: 1,
                    tags: vec![],
                },
                OdmEntry {
                    id: Some(duplicate_id),
                    title: "bulk-conflict".to_owned(),
                    score: 2,
                    tags: vec![],
                },
            ],
            true,
        )
        .await
        .expect_err("ordered bulk stops at a duplicate _id");
    assert!(std::error::Error::source(&partial_error).is_some());
    assert_eq!(
        partial_error.to_http_error().status_code(),
        StatusCode::CONFLICT,
        "a positively identified duplicate in insert_many maps to HTTP 409",
    );
    assert!(
        repository
            .find_by_id(duplicate_id)
            .await
            .expect("read partial insert")
            .is_some()
    );
    repository
        .delete_one(
            DocumentFilter::new()
                .eq("_id", duplicate_id)
                .expect("delete partial insert"),
        )
        .await
        .expect("clean partial insert fixture");

    let filter = DocumentFilter::and([
        DocumentFilter::new()
            .gte("score", 10)
            .expect("gte filter")
            .lt("score", 30)
            .expect("lt filter"),
        DocumentFilter::or([
            DocumentFilter::new()
                .eq("title", "alpha")
                .expect("title filter"),
            DocumentFilter::new()
                .eq("title", "beta")
                .expect("title filter"),
        ])
        .expect("or composition"),
        DocumentFilter::new()
            .exists("tags", true)
            .expect("field existence"),
    ])
    .expect("and composition");
    let page = repository
        .find_many(
            DocumentQuery::new(filter)
                .sort_desc("score")
                .expect("sort")
                .limit(10)
                .expect("page size"),
        )
        .await
        .expect("query page");
    assert_eq!(
        page.iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        vec!["beta", "alpha"]
    );

    let summaries = repository
        .find_many_as::<OdmEntrySummary>(
            DocumentQuery::new(
                DocumentFilter::new()
                    .in_values("title", ["alpha", "beta"])
                    .expect("in filter"),
            )
            .include("title")
            .expect("projection")
            .sort_asc("title")
            .expect("sort"),
        )
        .await
        .expect("typed projection");
    assert_eq!(
        summaries
            .iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );

    let updated = repository
        .update_one(
            DocumentFilter::new()
                .eq("_id", inserted.id.expect("id"))
                .expect("filter"),
            DocumentUpdate::new()
                .inc("score", 5)
                .expect("increment")
                .push("tags", "odm")
                .expect("array push"),
            false,
        )
        .await
        .expect("update typed document");
    assert_eq!(updated.matched_count, 1);
    assert_eq!(updated.modified_count, 1);
    let after_update = repository
        .find_by_id(inserted.id.expect("id"))
        .await
        .expect("read after update")
        .expect("document remains");
    assert_eq!(after_update.score, 15);
    assert!(after_update.tags.contains(&"odm".to_owned()));

    let pulled = repository
        .update_one(
            DocumentFilter::new()
                .eq("_id", inserted.id.expect("id"))
                .expect("filter"),
            DocumentUpdate::new()
                .pull("tags", "rust")
                .expect("pull array item"),
            false,
        )
        .await
        .expect("pull an array element");
    assert_eq!(pulled.modified_count, 1);
    let after_pull = repository
        .find_by_id(inserted.id.expect("id"))
        .await
        .expect("read after pull")
        .expect("document remains");
    assert!(!after_pull.tags.contains(&"rust".to_owned()));

    let replaced = repository
        .replace_one(
            DocumentFilter::new()
                .eq("_id", inserted.id.expect("id"))
                .expect("filter"),
            &OdmEntry {
                score: 16,
                ..after_update.clone()
            },
            false,
        )
        .await
        .expect("replace typed document");
    assert_eq!(replaced.matched_count, 1);
    assert_eq!(
        repository
            .find_by_id(inserted.id.expect("id"))
            .await
            .expect("read replacement")
            .expect("replacement exists")
            .score,
        16
    );

    let groups = repository
        .aggregate_as::<OdmScoreGroup>(vec![
            doc! { "$match": { "title": { "$in": ["alpha", "beta", "gamma"] } } },
            doc! { "$group": { "_id": "$score", "entries": { "$sum": 1 } } },
            doc! { "$sort": { "_id": 1 } },
        ])
        .await
        .expect("typed aggregation");
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].score, 16);
    assert_eq!(groups[0].entries, 1);

    assert_eq!(
        repository
            .count(DocumentFilter::new())
            .await
            .expect("count"),
        3
    );
    assert!(
        repository
            .exists(DocumentFilter::new().eq("title", "gamma").expect("filter"))
            .await
            .expect("exists query")
    );
    let upsert = repository
        .update_one(
            DocumentFilter::new()
                .eq("title", "created-by-upsert")
                .expect("filter"),
            DocumentUpdate::new()
                .set("score", 40)
                .expect("set score")
                .set("tags", Vec::<String>::new())
                .expect("set tags"),
            true,
        )
        .await
        .expect("upsert document");
    assert!(upsert.upserted_id.is_some());
    assert!(
        repository
            .find_one(
                DocumentFilter::new()
                    .eq("title", "created-by-upsert")
                    .expect("filter")
            )
            .await
            .expect("read upsert")
            .is_some()
    );

    let deleted: DocumentDeleteOutcome = repository
        .delete_many(
            DocumentFilter::new()
                .in_values("title", ["alpha", "beta", "gamma", "created-by-upsert"])
                .expect("delete filter"),
        )
        .await
        .expect("delete fixtures");
    assert_eq!(deleted.deleted_count, 4);
    assert_eq!(
        repository
            .count(DocumentFilter::new())
            .await
            .expect("empty collection"),
        0
    );

    database
        .run_with_timeout(database.database().drop())
        .await
        .expect("drop isolated test database");
}

#[tokio::test]
async fn mongodb_typed_odm_rejects_multi_document_transactions_on_standalone() {
    let Some(uri) = standalone_test_uri() else {
        eprintln!("Skipping standalone transaction restriction test outside CI: URI is unset");
        return;
    };
    let database_name = database_name();
    let config = MongoConfig::new(uri, database_name.clone())
        .expect("valid MongoDB standalone URI")
        .with_operation_timeout(Duration::from_secs(5))
        .expect("valid operation timeout");
    let database = MongoDatabase::connect(&config)
        .await
        .expect("connect to MongoDB standalone");
    let repository = database.repository::<OdmEntry>();

    let mut session = database
        .start_session()
        .await
        .expect("start standalone session");
    session
        .start_transaction()
        .await
        .expect("transaction state is local until operation");
    let error = repository
        .insert_with_session(
            odm_entry("standalone-transaction-must-fail", 1, &[]),
            &mut session,
        )
        .await
        .expect_err("standalone MongoDB must reject multi-document transaction writes");
    assert!(
        std::error::Error::source(&error).is_some(),
        "the server's transaction topology error must remain available as a source",
    );

    database
        .run_with_timeout(database.database().drop())
        .await
        .expect("drop isolated standalone test database");
}

#[tokio::test]
async fn mongodb_typed_odm_uses_real_replica_set_transactions_for_commit_and_abort() {
    let Some(uri) = test_uri() else {
        eprintln!("Skipping ODM transaction test outside CI: MongoDB URI is unset");
        return;
    };
    let database_name = database_name();
    let config = MongoConfig::new(uri, database_name.clone())
        .expect("valid MongoDB replica-set URI")
        .with_operation_timeout(Duration::from_secs(5))
        .expect("valid operation timeout");
    let database = MongoDatabase::connect(&config)
        .await
        .expect("connect to replica set");
    let repository: DocumentRepository<OdmEntry> = database.repository();

    let mut committed_session = database
        .start_session()
        .await
        .expect("start client session");
    committed_session
        .start_transaction()
        .await
        .expect("start replica-set transaction");
    let committed = repository
        .insert_with_session(
            odm_entry("committed", 1, &["transaction"]),
            &mut committed_session,
        )
        .await
        .expect("insert within transaction");
    let committed_id = committed.id.expect("generated ID");
    committed_session
        .commit_transaction()
        .await
        .expect("commit transaction");
    assert!(
        repository
            .find_by_id(committed_id)
            .await
            .expect("read committed doc")
            .is_some()
    );

    let mut aborted_session = database
        .start_session()
        .await
        .expect("start rollback session");
    aborted_session
        .start_transaction()
        .await
        .expect("start rollback transaction");
    let aborted = repository
        .insert_with_session(
            odm_entry("aborted", 2, &["transaction"]),
            &mut aborted_session,
        )
        .await
        .expect("insert before abort");
    let aborted_id = aborted.id.expect("generated ID");
    aborted_session
        .abort_transaction()
        .await
        .expect("abort transaction");
    assert!(
        repository
            .find_by_id(aborted_id)
            .await
            .expect("read aborted doc")
            .is_none()
    );

    database
        .run_with_timeout(database.database().drop())
        .await
        .expect("drop isolated transaction database");
}

#[tokio::test]
async fn mongodb_invalid_uri_and_unreachable_server_fail_with_bounded_timeout() {
    assert!(MongoConfig::new("postgres://127.0.0.1/test", "test").is_err());
    assert!(MongoConfig::new("mongodb+srv://cluster.example.test", "test").is_err());

    let config = MongoConfig::new(
        "mongodb://127.0.0.1:1/rustflow_test",
        "rustflow_phase662_unreachable",
    )
    .expect("well-shaped MongoDB URI")
    .with_connect_timeout(Duration::from_millis(500))
    .expect("valid connect timeout")
    .with_server_selection_timeout(Duration::from_millis(500))
    .expect("valid server-selection timeout")
    .with_operation_timeout(Duration::from_secs(1))
    .expect("valid operation timeout");

    let error = tokio::time::timeout(Duration::from_secs(4), MongoDatabase::connect(&config))
        .await
        .expect("unreachable server check is bounded")
        .expect_err("unreachable server must fail");
    assert_eq!(
        error.to_http_error().status_code(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(!format!("{error:?}").contains("127.0.0.1:1"));
    assert!(!format!("{error}").contains("mongodb://"));
}

#[test]
fn integration_bson_helpers_accept_native_documents() {
    let source = doc! { "name": "Ada", "metadata": { "active": true } };
    let copied: Document =
        from_bson_document(to_bson_document(&source).expect("serialize")).expect("deserialize");
    assert_eq!(copied, source);
}
