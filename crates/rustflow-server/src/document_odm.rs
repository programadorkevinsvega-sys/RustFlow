//! Typed MongoDB document mapping over RustFlow's existing Mongo client.
//! This module exposes document semantics, not SQL entities or relations.

use std::{collections::HashSet, marker::PhantomData};

use crate::{
    MongoDatabase, MongoError,
    mongodb_driver::{
        ClientSession, Cursor,
        bson::{self, Bson, Document, doc},
    },
    to_bson_document,
};
use serde::{Serialize, de::DeserializeOwned};

/// Maximum number of documents accepted by one typed insert-many call.
pub const MAX_DOCUMENT_BULK_SIZE: usize = 1_000;

/// A Serde-backed document model whose identifier field serializes as BSON _id.
pub trait DocumentModel: Serialize + DeserializeOwned + Clone + Send + Sync + 'static {
    /// BSON-serializable identifier type.
    type Id: Serialize + DeserializeOwned + Clone + Send + Sync + 'static;
    /// Collection used by this model.
    const COLLECTION: &'static str;
    /// Return the current identifier, if present.
    fn id(&self) -> Option<Self::Id>;
    /// Store an identifier returned by MongoDB after insertion.
    fn set_id(&mut self, id: Self::Id);
}

/// A typed static MongoDB field path.
///
/// Declare field constants next to a model to couple the path to its Rust value type:
///
/// ```
/// use rustflow_server::DocumentField;
///
/// const EMAIL: DocumentField<String> = DocumentField::new("email");
/// const AGE: DocumentField<i32> = DocumentField::new("age");
/// ```
///
/// The type parameter prevents passing a value of the wrong Rust type to typed filter
/// and update helpers. It does not inspect arbitrary Serde rename attributes at runtime.
#[derive(Debug)]
pub struct DocumentField<T> {
    path: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> Copy for DocumentField<T> {}

impl<T> Clone for DocumentField<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> DocumentField<T> {
    /// Declare a field path associated with a Rust/BSON value type.
    pub const fn new(path: &'static str) -> Self {
        Self {
            path,
            marker: PhantomData,
        }
    }

    /// Return the declared field path.
    pub const fn path(self) -> &'static str {
        self.path
    }
}

/// A MongoDB filter composed from BSON values and native query operators.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentFilter {
    document: Document,
}

impl DocumentFilter {
    /// Create an empty filter, matching every document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a trusted native filter. This does not validate authorization scope or operators.
    pub fn native(document: Document) -> Self {
        Self { document }
    }

    /// Borrow the underlying MongoDB filter document.
    pub fn as_document(&self) -> &Document {
        &self.document
    }

    /// Consume the filter and return its BSON document.
    pub fn into_document(self) -> Document {
        self.document
    }

    /// Match an exact serialized field value.
    pub fn eq<T: Serialize>(mut self, field: &str, value: T) -> Result<Self, MongoError> {
        self.insert_operator(field, "$eq", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match a typed field against an equal value.
    ///
    /// ```compile_fail
    /// use rustflow_server::{DocumentField, DocumentFilter};
    /// const EMAIL: DocumentField<String> = DocumentField::new("email");
    /// let _ = DocumentFilter::new().eq_field(EMAIL, 42_i32);
    /// ```
    pub fn eq_field<T: Serialize>(
        mut self,
        field: DocumentField<T>,
        value: T,
    ) -> Result<Self, MongoError> {
        self.insert_operator(field.path(), "$eq", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match a typed field greater than a value of its declared Rust type.
    pub fn gt_field<T: Serialize>(
        mut self,
        field: DocumentField<T>,
        value: T,
    ) -> Result<Self, MongoError> {
        self.insert_operator(field.path(), "$gt", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match a typed field greater than or equal to a value of its declared Rust type.
    pub fn gte_field<T: Serialize>(
        mut self,
        field: DocumentField<T>,
        value: T,
    ) -> Result<Self, MongoError> {
        self.insert_operator(field.path(), "$gte", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match a typed field less than a value of its declared Rust type.
    pub fn lt_field<T: Serialize>(
        mut self,
        field: DocumentField<T>,
        value: T,
    ) -> Result<Self, MongoError> {
        self.insert_operator(field.path(), "$lt", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match a field not equal to a serialized value.
    pub fn ne<T: Serialize>(mut self, field: &str, value: T) -> Result<Self, MongoError> {
        self.insert_operator(field, "$ne", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match a field greater than a serialized value.
    pub fn gt<T: Serialize>(mut self, field: &str, value: T) -> Result<Self, MongoError> {
        self.insert_operator(field, "$gt", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match a field greater than or equal to a serialized value.
    pub fn gte<T: Serialize>(mut self, field: &str, value: T) -> Result<Self, MongoError> {
        self.insert_operator(field, "$gte", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match a field less than a serialized value.
    pub fn lt<T: Serialize>(mut self, field: &str, value: T) -> Result<Self, MongoError> {
        self.insert_operator(field, "$lt", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match a field less than or equal to a serialized value.
    pub fn lte<T: Serialize>(mut self, field: &str, value: T) -> Result<Self, MongoError> {
        self.insert_operator(field, "$lte", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match documents where a field exists or is absent.
    pub fn exists(mut self, field: &str, exists: bool) -> Result<Self, MongoError> {
        self.insert_operator(field, "$exists", Bson::Boolean(exists))?;
        Ok(self)
    }

    /// Match BSON null; MongoDB also matches documents where the field is absent.
    pub fn is_null(mut self, field: &str) -> Result<Self, MongoError> {
        self.insert_operator(field, "$eq", Bson::Null)?;
        Ok(self)
    }

    /// Match a field whose array contains the serialized value.
    pub fn array_contains<T: Serialize>(
        mut self,
        field: &str,
        value: T,
    ) -> Result<Self, MongoError> {
        self.insert_operator(field, "$eq", bson_value(&value)?)?;
        Ok(self)
    }

    /// Match an array that contains all serialized values.
    pub fn all<T: Serialize>(
        mut self,
        field: &str,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Self, MongoError> {
        let values = values
            .into_iter()
            .map(|v| bson_value(&v))
            .collect::<Result<Vec<_>, _>>()?;
        self.insert_operator(field, "$all", Bson::Array(values))?;
        Ok(self)
    }

    /// Match an array element against a nested filter document.
    pub fn elem_match(mut self, field: &str, filter: DocumentFilter) -> Result<Self, MongoError> {
        self.insert_operator(field, "$elemMatch", Bson::Document(filter.into_document()))?;
        Ok(self)
    }

    /// Match a field against one of the provided values.
    pub fn in_values<T: Serialize>(
        mut self,
        field: &str,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Self, MongoError> {
        let values = values
            .into_iter()
            .map(|v| bson_value(&v))
            .collect::<Result<Vec<_>, _>>()?;
        self.insert_operator(field, "$in", Bson::Array(values))?;
        Ok(self)
    }

    /// Match a field against none of the provided values.
    pub fn not_in<T: Serialize>(
        mut self,
        field: &str,
        values: impl IntoIterator<Item = T>,
    ) -> Result<Self, MongoError> {
        let values = values
            .into_iter()
            .map(|v| bson_value(&v))
            .collect::<Result<Vec<_>, _>>()?;
        self.insert_operator(field, "$nin", Bson::Array(values))?;
        Ok(self)
    }

    /// Combine filters with MongoDB AND.
    pub fn and(filters: impl IntoIterator<Item = Self>) -> Result<Self, MongoError> {
        Self::combine("$and", filters)
    }

    /// Combine filters with MongoDB OR.
    pub fn or(filters: impl IntoIterator<Item = Self>) -> Result<Self, MongoError> {
        Self::combine("$or", filters)
    }

    fn combine(
        operator: &'static str,
        filters: impl IntoIterator<Item = Self>,
    ) -> Result<Self, MongoError> {
        let filters = filters
            .into_iter()
            .map(Self::into_document)
            .collect::<Vec<_>>();
        if filters.is_empty() {
            return Err(MongoError::InvalidDocumentQuery(
                "boolean filters require a condition",
            ));
        }
        let mut document = Document::new();
        document.insert(
            operator,
            Bson::Array(filters.into_iter().map(Bson::Document).collect()),
        );
        Ok(Self { document })
    }

    fn insert_operator(
        &mut self,
        field: &str,
        operator: &'static str,
        value: Bson,
    ) -> Result<(), MongoError> {
        validate_field_path(field)?;
        if !self.document.contains_key(field) {
            self.document.insert(field, Document::new());
        }
        match self.document.get_mut(field) {
            Some(Bson::Document(operators)) => {
                if operators.contains_key(operator) {
                    return Err(MongoError::InvalidDocumentQuery(
                        "filter repeats a field operator",
                    ));
                }
                operators.insert(operator, value);
                Ok(())
            }
            _ => Err(MongoError::InvalidDocumentQuery(
                "filter mixes equality and comparison on one field",
            )),
        }
    }
}

/// MongoDB query options with a default page size of 100 and a maximum of 1,000.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentQuery {
    filter: DocumentFilter,
    sort: Document,
    projection: Document,
    skip: Option<u64>,
    limit: i64,
}

impl Default for DocumentQuery {
    fn default() -> Self {
        Self {
            filter: DocumentFilter::new(),
            sort: Document::new(),
            projection: Document::new(),
            skip: None,
            limit: 100,
        }
    }
}

impl DocumentQuery {
    /// Start a query with a filter.
    pub fn new(filter: DocumentFilter) -> Self {
        Self {
            filter,
            ..Self::default()
        }
    }
    /// Replace the query filter.
    pub fn filter(mut self, filter: DocumentFilter) -> Self {
        self.filter = filter;
        self
    }
    /// Sort a field ascending.
    pub fn sort_asc(mut self, field: &str) -> Result<Self, MongoError> {
        self.set_sort(field, 1)?;
        Ok(self)
    }
    /// Sort a field descending.
    pub fn sort_desc(mut self, field: &str) -> Result<Self, MongoError> {
        self.set_sort(field, -1)?;
        Ok(self)
    }
    /// Skip matching documents.
    pub fn skip(mut self, count: u64) -> Self {
        self.skip = Some(count);
        self
    }
    /// Set page size in the inclusive range 1..=1000.
    pub fn limit(mut self, count: u64) -> Result<Self, MongoError> {
        if !(1..=1_000).contains(&count) {
            return Err(MongoError::InvalidDocumentQuery(
                "query page size must be between 1 and 1000",
            ));
        }
        self.limit = count as i64;
        Ok(self)
    }
    /// Include a field in the projection.
    pub fn include(mut self, field: &str) -> Result<Self, MongoError> {
        validate_field_path(field)?;
        if field != "_id"
            && self
                .projection
                .iter()
                .any(|(_, value)| matches!(value, Bson::Int32(0)))
        {
            return Err(MongoError::InvalidDocumentQuery(
                "inclusion and exclusion projections cannot be mixed",
            ));
        }
        self.projection.insert(field, 1);
        Ok(self)
    }
    /// Exclude a field from the projection.
    pub fn exclude(mut self, field: &str) -> Result<Self, MongoError> {
        validate_field_path(field)?;
        if field != "_id"
            && self
                .projection
                .iter()
                .any(|(_, value)| matches!(value, Bson::Int32(1)))
        {
            return Err(MongoError::InvalidDocumentQuery(
                "inclusion and exclusion projections cannot be mixed",
            ));
        }
        self.projection.insert(field, 0);
        Ok(self)
    }
    fn set_sort(&mut self, field: &str, direction: i32) -> Result<(), MongoError> {
        validate_field_path(field)?;
        self.sort.insert(field, direction);
        Ok(())
    }
}

/// MongoDB update document composed from native update operators.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentUpdate {
    document: Document,
    paths: HashSet<String>,
}

impl DocumentUpdate {
    /// Create an empty update.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set a field to a serialized value.
    pub fn set<T: Serialize>(mut self, field: &str, value: T) -> Result<Self, MongoError> {
        self.insert("$set", field, bson_value(&value)?)?;
        Ok(self)
    }
    /// Set a typed field using the value type associated with its declaration.
    pub fn set_field<T: Serialize>(
        mut self,
        field: DocumentField<T>,
        value: T,
    ) -> Result<Self, MongoError> {
        self.insert("$set", field.path(), bson_value(&value)?)?;
        Ok(self)
    }

    /// Increment a typed numeric field.
    pub fn inc_field<T: Serialize>(
        mut self,
        field: DocumentField<T>,
        amount: T,
    ) -> Result<Self, MongoError> {
        self.insert("$inc", field.path(), bson_value(&amount)?)?;
        Ok(self)
    }

    /// Remove a field.
    pub fn unset(mut self, field: &str) -> Result<Self, MongoError> {
        self.insert("$unset", field, Bson::Int32(1))?;
        Ok(self)
    }
    /// Increment a numeric field.
    pub fn inc<T: Serialize>(mut self, field: &str, amount: T) -> Result<Self, MongoError> {
        self.insert("$inc", field, bson_value(&amount)?)?;
        Ok(self)
    }
    /// Push a value into an array.
    pub fn push<T: Serialize>(mut self, field: &str, value: T) -> Result<Self, MongoError> {
        self.insert("$push", field, bson_value(&value)?)?;
        Ok(self)
    }
    /// Pull matching values from an array.
    pub fn pull<T: Serialize>(mut self, field: &str, value: T) -> Result<Self, MongoError> {
        self.insert("$pull", field, bson_value(&value)?)?;
        Ok(self)
    }
    /// Borrow the native update document.
    pub fn as_document(&self) -> &Document {
        &self.document
    }
    /// Consume the update and return the native update document.
    pub fn into_document(self) -> Result<Document, MongoError> {
        if self.document.is_empty() {
            return Err(MongoError::InvalidDocumentUpdate(
                "update must contain an operator",
            ));
        }
        Ok(self.document)
    }
    fn insert(
        &mut self,
        operator: &'static str,
        field: &str,
        value: Bson,
    ) -> Result<(), MongoError> {
        validate_field_path(field)?;
        if self.paths.iter().any(|existing| {
            existing == field
                || existing
                    .strip_prefix(field)
                    .is_some_and(|suffix| suffix.starts_with('.'))
                || field
                    .strip_prefix(existing)
                    .is_some_and(|suffix| suffix.starts_with('.'))
        }) {
            return Err(MongoError::InvalidDocumentUpdate(
                "an update cannot target the same or overlapping field paths",
            ));
        }
        self.paths.insert(field.to_owned());
        if !self.document.contains_key(operator) {
            self.document.insert(operator, Document::new());
        }
        match self.document.get_mut(operator) {
            Some(Bson::Document(fields)) => {
                fields.insert(field, value);
                Ok(())
            }
            _ => Err(MongoError::InvalidDocumentUpdate(
                "update operator document is invalid",
            )),
        }
    }
}

/// Counts returned from update_one, update_many, or replace_one.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentUpdateOutcome {
    /// Number of matching documents.
    pub matched_count: u64,
    /// Number of documents actually modified.
    pub modified_count: u64,
    /// Identifier created by an upsert, if any.
    pub upserted_id: Option<Bson>,
}

/// Count returned from a delete operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDeleteOutcome {
    /// Number of documents deleted.
    pub deleted_count: u64,
}

/// Repository bound to the collection declared by its model.
#[derive(Debug, Clone)]
pub struct DocumentRepository<M> {
    database: MongoDatabase,
    collection_name: &'static str,
    marker: PhantomData<fn() -> M>,
}

impl<M: DocumentModel> DocumentRepository<M> {
    fn new(database: MongoDatabase) -> Self {
        Self {
            database,
            collection_name: M::COLLECTION,
            marker: PhantomData,
        }
    }
    /// Return the collection name bound by the model.
    pub fn collection_name(&self) -> &'static str {
        self.collection_name
    }

    /// Insert one model and return it with its database-generated or supplied ID.
    pub async fn insert(&self, mut model: M) -> Result<M, MongoError> {
        if model.id().is_none() {
            validate_generated_id_type::<M>()?;
        }
        let document = serialize_document_model(&model)?;
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = self
            .database
            .run_owned_with_timeout(async move { collection.insert_one(document).await })
            .await?;
        model.set_id(bson::from_bson(result.inserted_id).map_err(MongoError::Deserialization)?);
        Ok(model)
    }

    /// Insert multiple documents. Partial write failures are returned as driver errors;
    /// ordered controls whether the server stops after the first write error.
    pub async fn insert_many(
        &self,
        mut models: Vec<M>,
        ordered: bool,
    ) -> Result<Vec<M>, MongoError> {
        validate_bulk_size(models.len())?;
        let mut documents = Vec::with_capacity(models.len());
        for model in &models {
            if model.id().is_none() {
                validate_generated_id_type::<M>()?;
            }
            documents.push(serialize_document_model(model)?);
        }
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = self
            .database
            .run_owned_with_timeout(async move {
                collection.insert_many(documents).ordered(ordered).await
            })
            .await?;
        for (index, id) in result.inserted_ids {
            if let Some(model) = models.get_mut(index) {
                model.set_id(bson::from_bson(id).map_err(MongoError::Deserialization)?);
            }
        }
        Ok(models)
    }

    /// Find by the BSON _id field.
    pub async fn find_by_id(&self, id: M::Id) -> Result<Option<M>, MongoError> {
        let filter = doc! { "_id": bson_value(&id)? };
        self.find_one(DocumentFilter::native(filter)).await
    }
    /// Find one document matching a filter.
    pub async fn find_one(&self, filter: DocumentFilter) -> Result<Option<M>, MongoError> {
        let collection = self.database.collection::<M>(self.collection_name);
        self.database
            .run_owned_with_timeout(
                async move { collection.find_one(filter.into_document()).await },
            )
            .await
    }
    /// Find up to the query page size as full models.
    pub async fn find_many(&self, query: DocumentQuery) -> Result<Vec<M>, MongoError> {
        self.find_many_as::<M>(query).await
    }
    /// Find a projection and deserialize it into a dedicated DTO type.
    pub async fn find_many_as<P>(&self, query: DocumentQuery) -> Result<Vec<P>, MongoError>
    where
        P: DeserializeOwned + Send + Sync + 'static,
    {
        let collection = self.database.collection::<P>(self.collection_name);
        let filter = query.filter.into_document();
        let sort = query.sort;
        let projection = query.projection;
        let skip = query.skip;
        let limit = query.limit;
        let cursor = self
            .database
            .run_owned_with_timeout(async move {
                let mut action = collection.find(filter);
                if !sort.is_empty() {
                    action = action.sort(sort);
                }
                if let Some(skip) = skip {
                    action = action.skip(skip);
                }
                action = action.limit(limit);
                if !projection.is_empty() {
                    action = action.projection(projection);
                }
                action.await
            })
            .await?;
        collect_cursor(&self.database, cursor).await
    }
    /// Count documents matching a filter.
    pub async fn count(&self, filter: DocumentFilter) -> Result<u64, MongoError> {
        let collection = self.database.collection::<Document>(self.collection_name);
        self.database
            .run_owned_with_timeout(async move {
                collection.count_documents(filter.into_document()).await
            })
            .await
    }
    /// Check whether any document matches a filter.
    pub async fn exists(&self, filter: DocumentFilter) -> Result<bool, MongoError> {
        let collection = self.database.collection::<Document>(self.collection_name);
        let count = self
            .database
            .run_owned_with_timeout(async move {
                collection
                    .count_documents(filter.into_document())
                    .limit(1)
                    .await
            })
            .await?;
        Ok(count > 0)
    }
    /// Replace one document. Upsert inserts when no document matches.
    pub async fn replace_one(
        &self,
        filter: DocumentFilter,
        replacement: &M,
        upsert: bool,
    ) -> Result<DocumentUpdateOutcome, MongoError> {
        if replacement.id().is_none() {
            return Err(MongoError::InvalidDocumentModel(
                "replace_one requires an explicit model ID",
            ));
        }
        let replacement = serialize_document_model(replacement)?;
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = self
            .database
            .run_owned_with_timeout(async move {
                collection
                    .replace_one(filter.into_document(), replacement)
                    .upsert(upsert)
                    .await
            })
            .await?;
        Ok(DocumentUpdateOutcome {
            matched_count: result.matched_count,
            modified_count: result.modified_count,
            upserted_id: result.upserted_id,
        })
    }
    /// Update one matching document.
    pub async fn update_one(
        &self,
        filter: DocumentFilter,
        update: DocumentUpdate,
        upsert: bool,
    ) -> Result<DocumentUpdateOutcome, MongoError> {
        let update = update.into_document()?;
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = self
            .database
            .run_owned_with_timeout(async move {
                collection
                    .update_one(filter.into_document(), update)
                    .upsert(upsert)
                    .await
            })
            .await?;
        Ok(DocumentUpdateOutcome {
            matched_count: result.matched_count,
            modified_count: result.modified_count,
            upserted_id: result.upserted_id,
        })
    }
    /// Update all matching documents.
    pub async fn update_many(
        &self,
        filter: DocumentFilter,
        update: DocumentUpdate,
        upsert: bool,
    ) -> Result<DocumentUpdateOutcome, MongoError> {
        let update = update.into_document()?;
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = self
            .database
            .run_owned_with_timeout(async move {
                collection
                    .update_many(filter.into_document(), update)
                    .upsert(upsert)
                    .await
            })
            .await?;
        Ok(DocumentUpdateOutcome {
            matched_count: result.matched_count,
            modified_count: result.modified_count,
            upserted_id: result.upserted_id,
        })
    }
    /// Delete one matching document.
    pub async fn delete_one(
        &self,
        filter: DocumentFilter,
    ) -> Result<DocumentDeleteOutcome, MongoError> {
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = self
            .database
            .run_owned_with_timeout(
                async move { collection.delete_one(filter.into_document()).await },
            )
            .await?;
        Ok(DocumentDeleteOutcome {
            deleted_count: result.deleted_count,
        })
    }
    /// Delete all matching documents.
    pub async fn delete_many(
        &self,
        filter: DocumentFilter,
    ) -> Result<DocumentDeleteOutcome, MongoError> {
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = self
            .database
            .run_owned_with_timeout(
                async move { collection.delete_many(filter.into_document()).await },
            )
            .await?;
        Ok(DocumentDeleteOutcome {
            deleted_count: result.deleted_count,
        })
    }
    /// Run a native aggregation pipeline and decode results into the requested type.
    pub async fn aggregate_as<R>(&self, pipeline: Vec<Document>) -> Result<Vec<R>, MongoError>
    where
        R: DeserializeOwned + Send + Sync + 'static,
    {
        if pipeline.is_empty() {
            return Err(MongoError::InvalidDocumentQuery(
                "aggregation pipeline cannot be empty",
            ));
        }
        // The official driver returns aggregation stages as BSON Documents even when
        // the source collection has a typed model. Decode each result into the caller's DTO.
        let collection = self.database.collection::<Document>(self.collection_name);
        let cursor = self
            .database
            .run_owned_with_timeout(async move { collection.aggregate(pipeline).await })
            .await?;
        collect_cursor(&self.database, cursor)
            .await?
            .into_iter()
            .map(|document| bson::from_document(document).map_err(MongoError::Deserialization))
            .collect()
    }

    /// Insert one model using a session created from this repository's MongoDatabase client.
    /// The borrowed session future is not wrapped in a cancellable timeout.
    pub async fn insert_with_session(
        &self,
        mut model: M,
        session: &mut ClientSession,
    ) -> Result<M, MongoError> {
        if model.id().is_none() {
            validate_generated_id_type::<M>()?;
        }
        let document = serialize_document_model(&model)?;
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = collection
            .insert_one(document)
            .session(&mut *session)
            .await
            .map_err(MongoError::Operation)?;
        model.set_id(bson::from_bson(result.inserted_id).map_err(MongoError::Deserialization)?);
        Ok(model)
    }
    /// Find by ID in an explicitly supplied session.
    pub async fn find_by_id_with_session(
        &self,
        id: M::Id,
        session: &mut ClientSession,
    ) -> Result<Option<M>, MongoError> {
        let filter = doc! { "_id": bson_value(&id)? };
        let collection = self.database.collection::<M>(self.collection_name);
        collection
            .find_one(filter)
            .session(&mut *session)
            .await
            .map_err(MongoError::Operation)
    }
    /// Update one document in an explicitly supplied session.
    pub async fn update_one_with_session(
        &self,
        filter: DocumentFilter,
        update: DocumentUpdate,
        session: &mut ClientSession,
    ) -> Result<DocumentUpdateOutcome, MongoError> {
        let update = update.into_document()?;
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = collection
            .update_one(filter.into_document(), update)
            .session(&mut *session)
            .await
            .map_err(MongoError::Operation)?;
        Ok(DocumentUpdateOutcome {
            matched_count: result.matched_count,
            modified_count: result.modified_count,
            upserted_id: result.upserted_id,
        })
    }
    /// Delete one document in an explicitly supplied session.
    pub async fn delete_one_with_session(
        &self,
        filter: DocumentFilter,
        session: &mut ClientSession,
    ) -> Result<DocumentDeleteOutcome, MongoError> {
        let collection = self.database.collection::<Document>(self.collection_name);
        let result = collection
            .delete_one(filter.into_document())
            .session(&mut *session)
            .await
            .map_err(MongoError::Operation)?;
        Ok(DocumentDeleteOutcome {
            deleted_count: result.deleted_count,
        })
    }
}

impl MongoDatabase {
    /// Obtain a repository for the collection declared by M.
    pub fn repository<M: DocumentModel>(&self) -> DocumentRepository<M> {
        DocumentRepository::new(self.clone())
    }
    /// Start a session using this database's existing client.
    pub async fn start_session(&self) -> Result<ClientSession, MongoError> {
        let client = self.client().clone();
        self.run_owned_with_timeout(async move { client.start_session().await })
            .await
    }
}

async fn collect_cursor<P>(
    database: &MongoDatabase,
    mut cursor: Cursor<P>,
) -> Result<Vec<P>, MongoError>
where
    P: DeserializeOwned + Send + Sync + 'static,
{
    let mut documents = Vec::new();
    loop {
        let (available, next_cursor) = database
            .run_owned_with_timeout(async move {
                let mut cursor = cursor;
                let available = cursor.advance().await?;
                Ok((available, cursor))
            })
            .await?;
        cursor = next_cursor;
        if !available {
            break;
        }
        documents.push(
            cursor
                .deserialize_current()
                .map_err(MongoError::Operation)?,
        );
    }
    Ok(documents)
}

fn validate_generated_id_type<M: DocumentModel>() -> Result<(), MongoError> {
    bson::from_bson::<M::Id>(Bson::ObjectId(bson::oid::ObjectId::new()))
        .map(|_| ())
        .map_err(|_| {
            MongoError::InvalidDocumentModel(
                "MongoDB-generated IDs are ObjectId values; provide an explicit ID for this model type",
            )
        })
}

fn serialize_document_model<M: DocumentModel>(model: &M) -> Result<Document, MongoError> {
    let mut document = to_bson_document(model)?;
    match model.id() {
        Some(id) => {
            let expected = bson_value(&id)?;
            match document.get("_id") {
                Some(serialized) if serialized == &expected => {}
                Some(_) => {
                    return Err(MongoError::InvalidDocumentModel(
                        "serialized BSON _id does not match DocumentModel::id",
                    ));
                }
                None => {
                    return Err(MongoError::InvalidDocumentModel(
                        "DocumentModel identifier must serialize as BSON _id",
                    ));
                }
            }
        }
        None => {
            // Omit an absent ID so MongoDB can generate an ObjectId on insert.
            document.remove("_id");
        }
    }
    Ok(document)
}

fn validate_bulk_size(size: usize) -> Result<(), MongoError> {
    if size == 0 {
        return Err(MongoError::InvalidDocumentModel(
            "insert_many requires at least one document",
        ));
    }
    if size > MAX_DOCUMENT_BULK_SIZE {
        return Err(MongoError::InvalidDocumentModel(
            "insert_many exceeds the maximum batch size of 1000 documents",
        ));
    }
    Ok(())
}

fn bson_value<T: Serialize>(value: &T) -> Result<Bson, MongoError> {
    bson::to_bson(value).map_err(MongoError::Serialization)
}

fn validate_field_path(field: &str) -> Result<(), MongoError> {
    if field.is_empty()
        || field.split('.').any(|part| {
            part.is_empty()
                || part.starts_with('$')
                || part.chars().any(|character| {
                    character == '\0' || character.is_control() || character == '$'
                })
        })
    {
        return Err(MongoError::InvalidDocumentQuery(
            "field path is empty or contains an unsupported segment",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mongodb_driver::bson::oid::ObjectId;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Serialize, Deserialize)]
    struct MissingIdRename {
        id: ObjectId,
    }

    impl DocumentModel for MissingIdRename {
        type Id = ObjectId;
        const COLLECTION: &'static str = "missing_id_rename";

        fn id(&self) -> Option<Self::Id> {
            Some(self.id)
        }
        fn set_id(&mut self, id: Self::Id) {
            self.id = id;
        }
    }

    #[test]
    fn document_model_requires_identifier_to_serialize_as_bson_id() {
        let model = MissingIdRename {
            id: bson::oid::ObjectId::new(),
        };
        assert!(serialize_document_model(&model).is_err());
    }

    #[derive(Clone, Serialize, Deserialize)]
    struct StringIdWithoutValue {
        #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    }

    impl DocumentModel for StringIdWithoutValue {
        type Id = String;
        const COLLECTION: &'static str = "string_ids";

        fn id(&self) -> Option<Self::Id> {
            self.id.clone()
        }

        fn set_id(&mut self, id: Self::Id) {
            self.id = Some(id);
        }
    }

    #[test]
    fn generated_object_ids_are_rejected_for_non_object_id_models_before_writes() {
        assert!(validate_generated_id_type::<StringIdWithoutValue>().is_err());
        assert!(validate_generated_id_type::<MissingIdRename>().is_ok());
    }

    #[test]
    fn filters_compose_native_nested_operators() {
        let filter = DocumentFilter::and([
            DocumentFilter::new()
                .gte("age", 18)
                .expect("gte")
                .lt("age", 65)
                .expect("lt merges"),
            DocumentFilter::or([
                DocumentFilter::new().eq("status", "active").expect("eq"),
                DocumentFilter::new().eq("status", "trial").expect("eq"),
            ])
            .expect("or"),
        ])
        .expect("and");
        assert_eq!(
            filter.into_document(),
            doc! {
                "$and": [
                    { "age": { "$gte": 18, "$lt": 65 } },
                    { "$or": [
                        { "status": { "$eq": "active" } },
                        { "status": { "$eq": "trial" } }
                    ] }
                ]
            }
        );
    }

    #[test]
    fn typed_field_helpers_use_declared_paths() {
        const AGE: DocumentField<i32> = DocumentField::new("age");
        const EMAIL: DocumentField<String> = DocumentField::new("email");
        let filter = DocumentFilter::new()
            .gte_field(AGE, 21)
            .expect("typed age filter")
            .eq_field(EMAIL, "ada@example.test".to_owned())
            .expect("typed email filter");
        assert_eq!(
            filter.into_document(),
            doc! {
                "age": { "$gte": 21 },
                "email": { "$eq": "ada@example.test" }
            }
        );
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct SerdeFieldSemantics {
        optional: Option<String>,
        #[serde(default = "default_score")]
        score: i32,
    }

    fn default_score() -> i32 {
        7
    }

    #[test]
    fn serde_missing_null_default_and_present_values_are_explicit() {
        let missing: SerdeFieldSemantics =
            bson::from_document(Document::new()).expect("missing optional/defaulted fields");
        assert_eq!(
            missing,
            SerdeFieldSemantics {
                optional: None,
                score: 7
            }
        );

        let explicit_null: SerdeFieldSemantics =
            bson::from_document(doc! { "optional": Bson::Null }).expect("null optional field");
        assert_eq!(
            explicit_null,
            SerdeFieldSemantics {
                optional: None,
                score: 7
            }
        );

        let present: SerdeFieldSemantics =
            bson::from_document(doc! { "optional": "value", "score": 12 }).expect("present values");
        assert_eq!(
            present,
            SerdeFieldSemantics {
                optional: Some("value".to_owned()),
                score: 12
            }
        );
    }

    #[test]
    fn array_and_nested_filter_operators_are_native_bson() {
        let filter = DocumentFilter::new()
            .all("tags", ["rust", "mongodb"])
            .expect("all values")
            .elem_match(
                "comments",
                DocumentFilter::new()
                    .gte("votes", 3)
                    .expect("nested votes filter"),
            )
            .expect("element match");
        assert_eq!(
            filter.into_document(),
            doc! {
                "tags": { "$all": ["rust", "mongodb"] },
                "comments": { "$elemMatch": { "votes": { "$gte": 3 } } }
            }
        );
        assert!(DocumentFilter::new().in_values("tags", ["rust"]).is_ok());
        assert!(DocumentFilter::new().not_in("tags", ["unsafe"]).is_ok());
    }

    #[test]
    fn query_sort_projection_and_pagination_are_bounded() {
        let query = DocumentQuery::new(
            DocumentFilter::new()
                .eq("profile.city", "Medellin")
                .expect("nested field"),
        )
        .sort_desc("created_at")
        .expect("sort")
        .skip(20)
        .limit(50)
        .expect("limit")
        .include("email")
        .expect("projection")
        .include("display_name")
        .expect("projection");
        assert_eq!(query.skip, Some(20));
        assert_eq!(query.limit, 50);
        assert_eq!(query.sort, doc! { "created_at": -1 });
        assert_eq!(query.projection, doc! { "email": 1, "display_name": 1 });
        assert!(DocumentQuery::default().limit(1_001).is_err());
        assert!(
            DocumentQuery::default()
                .include("email")
                .expect("include")
                .exclude("secret")
                .is_err()
        );
    }

    #[test]
    fn updates_use_mongo_operators_and_reject_repeated_fields() {
        let update = DocumentUpdate::new()
            .set("profile.city", "Medellin")
            .expect("set")
            .inc("visits", 1)
            .expect("inc")
            .push("tags", "rust")
            .expect("push")
            .unset("legacy")
            .expect("unset");
        assert_eq!(
            update.into_document().expect("document"),
            doc! {
                "$set": { "profile.city": "Medellin" },
                "$inc": { "visits": 1 },
                "$push": { "tags": "rust" },
                "$unset": { "legacy": 1 }
            }
        );
        let update = DocumentUpdate::new().set("counter", 1).expect("first");
        assert!(update.inc("counter", 1).is_err());
        assert!(DocumentUpdate::new().into_document().is_err());
        let overlap = DocumentUpdate::new()
            .set("profile", doc! { "city": "Medellin" })
            .expect("parent path");
        assert!(overlap.set("profile.city", "Bogota").is_err());
    }

    #[test]
    fn bulk_insert_size_limit_is_enforced() {
        assert_eq!(MAX_DOCUMENT_BULK_SIZE, 1_000);
        assert!(validate_bulk_size(0).is_err());
        assert!(validate_bulk_size(1).is_ok());
        assert!(validate_bulk_size(MAX_DOCUMENT_BULK_SIZE).is_ok());
        assert!(validate_bulk_size(MAX_DOCUMENT_BULK_SIZE + 1).is_err());
    }

    #[test]
    fn untrusted_field_paths_are_rejected() {
        assert!(DocumentFilter::new().eq("$where", "return true").is_err());
        assert!(
            DocumentFilter::new()
                .eq("profile.$where", "return true")
                .is_err()
        );
        assert!(DocumentQuery::default().sort_asc("").is_err());
        assert!(DocumentUpdate::new().set("name\0hidden", "x").is_err());
    }
}
