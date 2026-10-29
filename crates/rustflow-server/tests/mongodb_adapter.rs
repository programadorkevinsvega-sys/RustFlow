use std::{
    env,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use http::StatusCode;
use rustflow_server::mongodb_driver::bson::{doc, oid::ObjectId, Document};
use rustflow_server::{
    DocumentBootstrap, DocumentIndex, MongoConfig, MongoDatabase, MongoError,
    from_bson_document, to_bson_document,
};
use serde::{Deserialize, Serialize};

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
        eprintln!("Skipping MongoDB integration test outside CI: RUSTFLOW_TEST_MONGODB_URI is unset");
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
    assert!(!debug.contains(&uri), "MongoConfig Debug must redact the URI");
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
    database.bootstrap(&bootstrap).await.expect("first bootstrap");
    database
        .bootstrap(&bootstrap)
        .await
        .expect("repeat bootstrap is safe");

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
        let bad_credentials = tokio::time::timeout(
            Duration::from_secs(6),
            MongoDatabase::connect(&bad_config),
        )
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
async fn mongodb_invalid_uri_and_unreachable_server_fail_with_bounded_timeout() {
    assert!(MongoConfig::new("postgres://127.0.0.1/test", "test").is_err());

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
    assert_eq!(error.to_http_error().status_code(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(!format!("{error:?}").contains("127.0.0.1:1"));
    assert!(!format!("{error}").contains("mongodb://"));
}

#[test]
fn integration_bson_helpers_accept_native_documents() {
    let source = doc! { "name": "Ada", "metadata": { "active": true } };
    let copied: Document = from_bson_document(to_bson_document(&source).expect("serialize"))
        .expect("deserialize");
    assert_eq!(copied, source);
}
