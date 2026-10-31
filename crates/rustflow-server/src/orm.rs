//! Typed Data Mapper helpers built on SeaORM.
//!
//! Repositories borrow an existing connection or transaction. They never create pools
//! and do not perform lazy database I/O when relation fields are read.

use std::{
    any::Any,
    collections::{HashMap, HashSet},
    future::Future,
    hash::Hash,
    marker::PhantomData,
    pin::Pin,
};

use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ColumnTrait, ConnectionTrait, DeleteMany, DeleteResult,
    EntityTrait, FromQueryResult, InsertResult, IntoActiveModel, LoaderTrait, ModelTrait,
    PrimaryKeyTrait, QueryFilter, Related, Select, UpdateMany, Value,
    query::EntityOrSelect,
};

use crate::database::{DatabaseError, DatabaseTransaction};

/// A typed repository for one SeaORM entity and an explicit database connection.
///
/// Construct it through DatabasePool::entity or DatabaseTransaction::entity so the
/// repository always uses RustFlow's existing shared SQLx/SeaORM connection.
pub struct EntityRepository<'db, E, C>
where
    E: EntityTrait,
    C: ConnectionTrait,
{
    connection: &'db C,
    entity: PhantomData<fn() -> E>,
}

impl<'db, E, C> EntityRepository<'db, E, C>
where
    E: EntityTrait,
    C: ConnectionTrait,
{
    /// Bind the repository to an existing connection or transaction.
    pub fn new(connection: &'db C) -> Self {
        Self {
            connection,
            entity: PhantomData,
        }
    }

    /// Return the borrowed connection for advanced SeaORM operations.
    pub fn connection(&self) -> &'db C {
        self.connection
    }

    /// Start a composable typed SELECT query.
    ///
    /// Use SeaORM's QueryFilter, QueryOrder and QuerySelect traits to add nested
    /// predicates, projections, aliases, joins, grouping, aggregates, HAVING and
    /// pagination. Subqueries and expression composition remain native SeaORM APIs.
    pub fn select(&self) -> Select<E> {
        E::find()
    }

    /// Execute a SELECT builder and decode complete entity models.
    pub async fn all(&self, query: Select<E>) -> Result<Vec<E::Model>, DatabaseError> {
        query
            .all(self.connection)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Execute a SELECT builder and decode at most one complete entity model.
    pub async fn one(&self, query: Select<E>) -> Result<Option<E::Model>, DatabaseError> {
        query
            .one(self.connection)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Find one model by its complete primary key.
    pub async fn find_by_id(
        &self,
        id: <E::PrimaryKey as PrimaryKeyTrait>::ValueType,
    ) -> Result<Option<E::Model>, DatabaseError>
    where
        E::PrimaryKey: PrimaryKeyTrait,
    {
        E::find_by_id(id)
            .one(self.connection)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Start a native SeaORM insert builder for explicit dialect-specific behavior.
    ///
    /// Use this to compose OnConflict/upsert statements or select a RETURNING strategy.
    /// Check DatabasePool::capabilities before requesting a returned model: the shared
    /// MySQL/MariaDB backend does not advertise INSERT RETURNING.
    pub fn insert_builder(&self, active_model: E::ActiveModel) -> sea_orm::Insert<E::ActiveModel>
    where
        E::ActiveModel: ActiveModelTrait<Entity = E>,
    {
        E::insert(active_model)
    }

    /// Insert an ActiveModel and return the database-observed Model.
    pub async fn insert(&self, active_model: E::ActiveModel) -> Result<E::Model, DatabaseError>
    where
        E::ActiveModel: ActiveModelBehavior + Send,
        E::Model: IntoActiveModel<E::ActiveModel>,
    {
        active_model
            .insert(self.connection)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Update one ActiveModel by its complete primary key.
    pub async fn update(&self, active_model: E::ActiveModel) -> Result<E::Model, DatabaseError>
    where
        E::ActiveModel: ActiveModelTrait<Entity = E> + ActiveModelBehavior + Send,
        E::Model: IntoActiveModel<E::ActiveModel>,
    {
        if active_model.get_primary_key_value().is_none() {
            return Err(DatabaseError::InvalidModelState(
                "an update requires every primary-key field to be set",
            ));
        }
        active_model
            .update(self.connection)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Insert when the generated primary key is NotSet; otherwise update by that key.
    pub async fn save(&self, active_model: E::ActiveModel) -> Result<E::ActiveModel, DatabaseError>
    where
        E::ActiveModel: ActiveModelBehavior + Send,
        E::Model: IntoActiveModel<E::ActiveModel>,
    {
        active_model
            .save(self.connection)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Delete one ActiveModel by its complete primary key.
    pub async fn delete(&self, active_model: E::ActiveModel) -> Result<DeleteResult, DatabaseError>
    where
        E::ActiveModel: ActiveModelTrait<Entity = E> + ActiveModelBehavior + Send,
    {
        if active_model.get_primary_key_value().is_none() {
            return Err(DatabaseError::InvalidModelState(
                "a delete requires every primary-key field to be set",
            ));
        }
        active_model
            .delete(self.connection)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Delete one row by its complete primary key.
    pub async fn delete_by_id(
        &self,
        id: <E::PrimaryKey as PrimaryKeyTrait>::ValueType,
    ) -> Result<DeleteResult, DatabaseError>
    where
        E::PrimaryKey: PrimaryKeyTrait,
    {
        E::delete_by_id(id)
            .exec(self.connection)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Insert several ActiveModels with SeaORM's backend-aware bulk insert builder.
    pub async fn insert_many<I>(
        &self,
        models: I,
    ) -> Result<InsertResult<E::ActiveModel>, DatabaseError>
    where
        I: IntoIterator<Item = E::ActiveModel>,
        E::ActiveModel: ActiveModelBehavior + Send,
        E::Model: IntoActiveModel<E::ActiveModel>,
    {
        E::insert_many(models)
            .exec(self.connection)
            .await
            .map_err(DatabaseError::OrmQuery)
    }

    /// Start an UPDATE builder; add a SET expression and filter before execution.
    pub fn update_many(&self) -> UpdateMany<E> {
        E::update_many()
    }

    /// Start a DELETE builder; add a filter before execution unless deleting all rows is intended.
    pub fn delete_many(&self) -> DeleteMany<E> {
        E::delete_many()
    }
}

/// Batch-load a has-one or belongs-to relation for a slice of parent models.
pub async fn load_one<M, R, S, C>(
    parents: &[M],
    related: S,
    connection: &C,
) -> Result<Vec<Option<R::Model>>, DatabaseError>
where
    M: ModelTrait + Send + Sync,
    M::Entity: Related<R>,
    R: EntityTrait + Send,
    R::Model: Send + Sync,
    S: EntityOrSelect<R>,
    C: ConnectionTrait,
{
    parents
        .load_one(related, connection)
        .await
        .map_err(DatabaseError::OrmQuery)
}

/// Batch-load a has-many relation with one relation query per hop, avoiding N+1.
pub async fn load_many<M, R, S, C>(
    parents: &[M],
    related: S,
    connection: &C,
) -> Result<Vec<Vec<R::Model>>, DatabaseError>
where
    M: ModelTrait + Send + Sync,
    M::Entity: Related<R>,
    R: EntityTrait + Send,
    R::Model: Send + Sync,
    S: EntityOrSelect<R>,
    C: ConnectionTrait,
{
    parents
        .load_many(related, connection)
        .await
        .map_err(DatabaseError::OrmQuery)
}

/// Batch-load a many-to-many relation through its explicit junction entity.
pub async fn load_many_to_many<M, R, S, V, C>(
    parents: &[M],
    related: S,
    via: V,
    connection: &C,
) -> Result<Vec<Vec<R::Model>>, DatabaseError>
where
    M: ModelTrait + Send + Sync,
    M::Entity: Related<R>,
    R: EntityTrait + Send,
    R::Model: Send + Sync,
    S: EntityOrSelect<R>,
    V: EntityTrait,
    V::Model: Send + Sync,
    C: ConnectionTrait,
{
    parents
        .load_many_to_many(related, via, connection)
        .await
        .map_err(DatabaseError::OrmQuery)
}


/// Options for one explicit relationship load.
///
/// The wrapped SeaORM Select is executed as part of the batch loader, so filters,
/// ordering, limits and other query clauses are applied to the related table without
/// issuing one query per parent. Use modify to compose the native SeaORM query builder.
pub struct RelationLoadOptions<E>
where
    E: EntityTrait + Send,
{
    query: Select<E>,
}

impl<E> RelationLoadOptions<E>
where
    E: EntityTrait + Send,
{
    /// Start with every row in the related entity; add constraints with modify.
    pub fn new() -> Self {
        Self { query: E::find() }
    }

    /// Wrap an already composed SeaORM query.
    pub fn from_query(query: Select<E>) -> Self {
        Self { query }
    }

    /// Apply filters, ordering or other SeaORM query-builder clauses.
    ///
    /// Values supplied to SeaORM's column expressions are bound parameters. Prefer a
    /// deterministic tiebreaker (usually the related entity's primary key) when ordering.
    pub fn modify<F>(mut self, modify: F) -> Self
    where
        F: FnOnce(Select<E>) -> Select<E>,
    {
        self.query = modify(self.query);
        self
    }

    /// Consume the options and return the underlying typed query.
    pub fn into_query(self) -> Select<E> {
        self.query
    }
}

impl<E> Default for RelationLoadOptions<E>
where
    E: EntityTrait + Send,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E> EntityOrSelect<E> for RelationLoadOptions<E>
where
    E: EntityTrait + Send,
{
    fn select(self) -> Select<E> {
        self.query
    }
}

/// Batch-load a has-many relation into a caller-defined projection.
///
/// foreign_key identifies the related-table column that points to the parent key.
/// Both key extractors are required: related_key must read that same foreign-key
/// column from the projection type D. This makes omission of the association key a
/// compile-time error rather than silently mis-grouping projected records.
///
/// The projection is filtered with a parameterized IN expression and fetched in one
/// query for the complete parent slice. The database's result order is retained inside
/// each parent's group. D must be cloneable because duplicate parent keys receive the
/// same related projection values.
pub async fn load_many_projected<M, R, D, K, C, ParentKey, RelatedKey>(
    parents: &[M],
    foreign_key: R::Column,
    options: RelationLoadOptions<R>,
    parent_key: ParentKey,
    related_key: RelatedKey,
    connection: &C,
) -> Result<Vec<Vec<D>>, DatabaseError>
where
    M: ModelTrait + Send + Sync,
    R: EntityTrait + Send,
    R::Column: ColumnTrait,
    D: FromQueryResult + Clone + Send + Sync,
    K: Clone + Eq + Hash + Into<Value> + Send + Sync,
    ParentKey: Fn(&M) -> K,
    RelatedKey: Fn(&D) -> K,
    C: ConnectionTrait,
{
    if parents.is_empty() {
        return Ok(Vec::new());
    }

    let keys: Vec<K> = parents.iter().map(parent_key).collect();
    let query = options
        .into_query()
        .filter(foreign_key.is_in(keys.clone()));

    let projected = query
        .into_model::<D>()
        .all(connection)
        .await
        .map_err(DatabaseError::OrmQuery)?;

    let mut grouped: HashMap<K, Vec<D>> = HashMap::new();
    for record in projected {
        grouped.entry(related_key(&record)).or_default().push(record);
    }

    Ok(parents
        .iter()
        .map(|parent| grouped.get(&parent_key(parent)).cloned().unwrap_or_default())
        .collect())
}

/// Maximum supported nesting depth for an explicit relation graph.
pub const MAX_RELATION_GRAPH_DEPTH: usize = 16;

type RelationGraphFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'a>>;

trait RelationGraphNode<P, C>: Send
where
    P: ModelTrait + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    fn load<'a>(
        self: Box<Self>,
        parents: &'a [P],
        connection: &'a C,
        results: &'a mut RelationGraphResults,
        prefix: String,
        depth: usize,
        max_depth: usize,
    ) -> RelationGraphFuture<'a>
    where
        P: 'a,
        C: 'a;
}

/// Results from an explicitly executed RelationGraph.
///
/// many and one retrieve a relation by its graph path (for example "posts" or
/// "posts.comments"). For a nested path, the returned groups correspond to the
/// flattened related models from the preceding path, in the same order produced by
/// that preceding batch load.
#[derive(Default)]
pub struct RelationGraphResults {
    values: HashMap<String, Box<dyn Any + Send + Sync>>,
}

impl RelationGraphResults {
    /// Return the grouped models produced by a with_many or with_many_to_many node.
    pub fn many<M>(&self, path: &str) -> Option<&[Vec<M>]>
    where
        M: 'static,
    {
        self.values
            .get(path)?
            .downcast_ref::<Vec<Vec<M>>>()
            .map(Vec::as_slice)
    }

    /// Return the optional models produced by a with_one node.
    pub fn one<M>(&self, path: &str) -> Option<&[Option<M>]>
    where
        M: 'static,
    {
        self.values
            .get(path)?
            .downcast_ref::<Vec<Option<M>>>()
            .map(Vec::as_slice)
    }
}

/// An explicit, typed graph of relationship batch loads.
///
/// Each node is executed once for its entire parent slice. Nested nodes receive the
/// flattened result of their parent node, so query counts scale with graph edges rather
/// than with the number of parent records. No relationship is loaded when this graph is
/// not explicitly executed.
pub struct RelationGraph<P, C>
where
    P: ModelTrait + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    nodes: Vec<Box<dyn RelationGraphNode<P, C>>>,
    names: HashSet<String>,
    max_depth: usize,
}

impl<P, C> RelationGraph<P, C>
where
    P: ModelTrait + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    /// Create an empty relation graph.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            names: HashSet::new(),
            max_depth: 8,
        }
    }

    /// Set a graph-wide maximum nesting depth (between 1 and 16).
    pub fn with_max_depth(mut self, max_depth: usize) -> Result<Self, DatabaseError> {
        if !(1..=MAX_RELATION_GRAPH_DEPTH).contains(&max_depth) {
            return Err(DatabaseError::InvalidConfiguration(
                "relation graph depth must be between 1 and MAX_RELATION_GRAPH_DEPTH",
            ));
        }
        self.max_depth = max_depth;
        Ok(self)
    }

    /// Add a batch-loaded has-many or belongs-to traversal.
    ///
    /// children describes additional relations to load from the related models. Use
    /// RelationLoadOptions::modify to filter and order this edge independently.
    pub fn with_many<R>(
        mut self,
        name: impl Into<String>,
        options: RelationLoadOptions<R>,
        children: RelationGraph<R::Model, C>,
    ) -> Result<Self, DatabaseError>
    where
        R: EntityTrait + Send + 'static,
        R::Model: ModelTrait + Send + Sync + 'static,
        P::Entity: Related<R>,
    {
        let name = self.insert_name(name.into())?;
        self.nodes.push(Box::new(ManyRelationNode {
            name,
            options,
            children,
            _parent: PhantomData,
        }));
        Ok(self)
    }

    /// Add a batch-loaded has-one or belongs-to traversal.
    pub fn with_one<R>(
        mut self,
        name: impl Into<String>,
        options: RelationLoadOptions<R>,
        children: RelationGraph<R::Model, C>,
    ) -> Result<Self, DatabaseError>
    where
        R: EntityTrait + Send + 'static,
        R::Model: ModelTrait + Send + Sync + 'static,
        P::Entity: Related<R>,
    {
        let name = self.insert_name(name.into())?;
        self.nodes.push(Box::new(OneRelationNode {
            name,
            options,
            children,
            _parent: PhantomData,
        }));
        Ok(self)
    }

    /// Add a batch-loaded many-to-many traversal through an explicit junction entity.
    pub fn with_many_to_many<R, V>(
        mut self,
        name: impl Into<String>,
        options: RelationLoadOptions<R>,
        via: V,
        children: RelationGraph<R::Model, C>,
    ) -> Result<Self, DatabaseError>
    where
        R: EntityTrait + Send + 'static,
        R::Model: ModelTrait + Send + Sync + 'static,
        V: EntityTrait + Send + 'static,
        V::Model: Send + Sync,
        P::Entity: Related<R>,
    {
        let name = self.insert_name(name.into())?;
        self.nodes.push(Box::new(ManyToManyRelationNode {
            name,
            options,
            via,
            children,
            _parent: PhantomData,
        }));
        Ok(self)
    }


    /// Add a projected has-many branch using a DTO with its association key.
    ///
    /// The DTO must expose the related foreign-key value through related_key. This edge
    /// can be combined with filtered and ordered RelationLoadOptions and executes as one
    /// parameterized batch query. Projected DTOs do not automatically become entity
    /// models for further nested traversal.
    pub fn with_many_projected<R, D, K, ParentKey, RelatedKey>(
        mut self,
        name: impl Into<String>,
        foreign_key: R::Column,
        options: RelationLoadOptions<R>,
        parent_key: ParentKey,
        related_key: RelatedKey,
    ) -> Result<Self, DatabaseError>
    where
        R: EntityTrait + Send + 'static,
        R::Column: ColumnTrait,
        R::Model: Send + Sync,
        P::Entity: Related<R>,
        D: FromQueryResult + Clone + Send + Sync + 'static,
        K: Clone + Eq + Hash + Into<Value> + Send + Sync + 'static,
        ParentKey: Fn(&P) -> K + Send + Sync + 'static,
        RelatedKey: Fn(&D) -> K + Send + Sync + 'static,
    {
        let name = self.insert_name(name.into())?;
        self.nodes.push(Box::new(ProjectedManyRelationNode {
            name,
            foreign_key,
            options,
            parent_key,
            related_key,
            _types: PhantomData,
        }));
        Ok(self)
    }

    /// Execute the complete graph explicitly against the supplied connection/transaction.
    pub async fn load(
        self,
        parents: &[P],
        connection: &C,
    ) -> Result<RelationGraphResults, DatabaseError> {
        let max_depth = self.max_depth;
        let mut results = RelationGraphResults::default();
        self.load_at(parents, connection, &mut results, String::new(), 0, max_depth)
            .await?;
        Ok(results)
    }

    fn insert_name(&mut self, name: String) -> Result<String, DatabaseError> {
        if name.trim().is_empty() || name.contains('.') || name.chars().any(char::is_control) {
            return Err(DatabaseError::InvalidModelState(
                "relation graph node names must be non-empty path segments without dots or control characters",
            ));
        }
        if !self.names.insert(name.clone()) {
            return Err(DatabaseError::InvalidModelState(
                "relation graph contains a duplicate sibling node name",
            ));
        }
        Ok(name)
    }

    async fn load_at(
        self,
        parents: &[P],
        connection: &C,
        results: &mut RelationGraphResults,
        prefix: String,
        depth: usize,
        max_depth: usize,
    ) -> Result<(), DatabaseError> {
        if depth > max_depth {
            return Err(DatabaseError::InvalidModelState(
                "relation graph exceeds its configured maximum depth",
            ));
        }
        for node in self.nodes {
            node.load(
                parents,
                connection,
                results,
                prefix.clone(),
                depth,
                max_depth,
            )
            .await?;
        }
        Ok(())
    }
}

impl<P, C> Default for RelationGraph<P, C>
where
    P: ModelTrait + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

struct ManyRelationNode<P, R, C>
where
    P: ModelTrait + Send + Sync + 'static,
    R: EntityTrait + Send,
    R::Model: ModelTrait + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    name: String,
    options: RelationLoadOptions<R>,
    children: RelationGraph<R::Model, C>,
    _parent: PhantomData<fn() -> P>,
}

impl<P, R, C> RelationGraphNode<P, C> for ManyRelationNode<P, R, C>
where
    P: ModelTrait + Send + Sync + 'static,
    P::Entity: Related<R>,
    R: EntityTrait + Send + 'static,
    R::Model: ModelTrait + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    fn load<'a>(
        self: Box<Self>,
        parents: &'a [P],
        connection: &'a C,
        results: &'a mut RelationGraphResults,
        prefix: String,
        depth: usize,
        max_depth: usize,
    ) -> RelationGraphFuture<'a>
    where
        P: 'a,
        C: 'a,
    {
        Box::pin(async move {
            let path = relation_graph_path(&prefix, &self.name);
            let loaded = parents
                .load_many(self.options, connection)
                .await
                .map_err(DatabaseError::OrmQuery)?;
            let nested: Vec<R::Model> = loaded.iter().flatten().cloned().collect();
            results.values.insert(path.clone(), Box::new(loaded));
            if !nested.is_empty() {
                self.children
                    .load_at(&nested, connection, results, path, depth + 1, max_depth)
                    .await?;
            }
            Ok(())
        })
    }
}

struct OneRelationNode<P, R, C>
where
    P: ModelTrait + Send + Sync + 'static,
    R: EntityTrait + Send,
    R::Model: ModelTrait + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    name: String,
    options: RelationLoadOptions<R>,
    children: RelationGraph<R::Model, C>,
    _parent: PhantomData<fn() -> P>,
}

impl<P, R, C> RelationGraphNode<P, C> for OneRelationNode<P, R, C>
where
    P: ModelTrait + Send + Sync + 'static,
    P::Entity: Related<R>,
    R: EntityTrait + Send + 'static,
    R::Model: ModelTrait + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    fn load<'a>(
        self: Box<Self>,
        parents: &'a [P],
        connection: &'a C,
        results: &'a mut RelationGraphResults,
        prefix: String,
        depth: usize,
        max_depth: usize,
    ) -> RelationGraphFuture<'a>
    where
        P: 'a,
        C: 'a,
    {
        Box::pin(async move {
            let path = relation_graph_path(&prefix, &self.name);
            let loaded = parents
                .load_one(self.options, connection)
                .await
                .map_err(DatabaseError::OrmQuery)?;
            let nested: Vec<R::Model> = loaded.iter().flatten().cloned().collect();
            results.values.insert(path.clone(), Box::new(loaded));
            if !nested.is_empty() {
                self.children
                    .load_at(&nested, connection, results, path, depth + 1, max_depth)
                    .await?;
            }
            Ok(())
        })
    }
}

struct ManyToManyRelationNode<P, R, V, C>
where
    P: ModelTrait + Send + Sync + 'static,
    R: EntityTrait + Send,
    V: EntityTrait + Send,
    R::Model: ModelTrait + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    name: String,
    options: RelationLoadOptions<R>,
    via: V,
    children: RelationGraph<R::Model, C>,
    _parent: PhantomData<fn() -> P>,
}

impl<P, R, V, C> RelationGraphNode<P, C> for ManyToManyRelationNode<P, R, V, C>
where
    P: ModelTrait + Send + Sync + 'static,
    P::Entity: Related<R>,
    R: EntityTrait + Send + 'static,
    R::Model: ModelTrait + Send + Sync + 'static,
    V: EntityTrait + Send + 'static,
    V::Model: Send + Sync,
    C: ConnectionTrait + Send + Sync + 'static,
{
    fn load<'a>(
        self: Box<Self>,
        parents: &'a [P],
        connection: &'a C,
        results: &'a mut RelationGraphResults,
        prefix: String,
        depth: usize,
        max_depth: usize,
    ) -> RelationGraphFuture<'a>
    where
        P: 'a,
        C: 'a,
    {
        Box::pin(async move {
            let path = relation_graph_path(&prefix, &self.name);
            let loaded = parents
                .load_many_to_many(self.options, self.via, connection)
                .await
                .map_err(DatabaseError::OrmQuery)?;
            let nested: Vec<R::Model> = loaded.iter().flatten().cloned().collect();
            results.values.insert(path.clone(), Box::new(loaded));
            if !nested.is_empty() {
                self.children
                    .load_at(&nested, connection, results, path, depth + 1, max_depth)
                    .await?;
            }
            Ok(())
        })
    }
}


struct ProjectedManyRelationNode<P, R, D, K, ParentKey, RelatedKey, C>
where
    P: ModelTrait + Send + Sync + 'static,
    R: EntityTrait + Send,
    C: ConnectionTrait + Send + Sync + 'static,
{
    name: String,
    foreign_key: R::Column,
    options: RelationLoadOptions<R>,
    parent_key: ParentKey,
    related_key: RelatedKey,
    _types: PhantomData<fn(&P) -> (D, K)>,
}

impl<P, R, D, K, ParentKey, RelatedKey, C> RelationGraphNode<P, C>
    for ProjectedManyRelationNode<P, R, D, K, ParentKey, RelatedKey, C>
where
    P: ModelTrait + Send + Sync + 'static,
    P::Entity: Related<R>,
    R: EntityTrait + Send + 'static,
    R::Column: ColumnTrait,
    R::Model: Send + Sync,
    D: FromQueryResult + Clone + Send + Sync + 'static,
    K: Clone + Eq + Hash + Into<Value> + Send + Sync + 'static,
    ParentKey: Fn(&P) -> K + Send + Sync + 'static,
    RelatedKey: Fn(&D) -> K + Send + Sync + 'static,
    C: ConnectionTrait + Send + Sync + 'static,
{
    fn load<'a>(
        self: Box<Self>,
        parents: &'a [P],
        connection: &'a C,
        results: &'a mut RelationGraphResults,
        prefix: String,
        _depth: usize,
        _max_depth: usize,
    ) -> RelationGraphFuture<'a>
    where
        P: 'a,
        C: 'a,
    {
        Box::pin(async move {
            let path = relation_graph_path(&prefix, &self.name);
            let loaded = load_many_projected(
                parents,
                self.foreign_key,
                self.options,
                &self.parent_key,
                &self.related_key,
                connection,
            )
            .await?;
            results.values.insert(path, Box::new(loaded));
            Ok(())
        })
    }
}

fn relation_graph_path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

/// Reassign an existing related model to a parent using its typed foreign-key column.
///
/// This does not create or delete a model. It performs one UPDATE and respects the
/// database's foreign-key and uniqueness constraints. Pass an explicit transaction when
/// reparenting is one step in a larger atomic operation.
pub async fn attach_one_to_many<M, C>(
    model: M,
    foreign_key: <M::Entity as EntityTrait>::Column,
    parent_key: Value,
    connection: &C,
) -> Result<M, DatabaseError>
where
    M: ModelTrait + IntoActiveModel<<M::Entity as EntityTrait>::ActiveModel>,
    M::Entity: EntityTrait<Model = M>,
    <M::Entity as EntityTrait>::ActiveModel:
        ActiveModelTrait<Entity = M::Entity> + ActiveModelBehavior + Send,
    C: ConnectionTrait,
{
    update_relation_key(model, foreign_key, parent_key, connection).await
}

/// Detach an existing has-one/has-many related model by setting its foreign key to NULL.
///
/// The foreign-key column must be nullable. A NOT NULL or other integrity violation is
/// returned as a database error; this helper never deletes the related row or cascades
/// to other records.
pub async fn detach_one_to_many<M, C>(
    model: M,
    foreign_key: <M::Entity as EntityTrait>::Column,
    connection: &C,
) -> Result<M, DatabaseError>
where
    M: ModelTrait + IntoActiveModel<<M::Entity as EntityTrait>::ActiveModel>,
    M::Entity: EntityTrait<Model = M>,
    <M::Entity as EntityTrait>::ActiveModel:
        ActiveModelTrait<Entity = M::Entity> + ActiveModelBehavior + Send,
    C: ConnectionTrait,
{
    let null_value = model.get(foreign_key).as_null();
    update_relation_key(model, foreign_key, null_value, connection).await
}

/// Replace a one-to-one related row inside an explicit transaction.
///
/// The previous row, when present, is detached first; the replacement row is then
/// associated with parent_key. The foreign key must be nullable and protected by a
/// UNIQUE constraint for true one-to-one semantics. The caller owns commit/rollback and
/// must roll back the transaction if this function returns an error.
pub async fn replace_one_to_one<M>(
    previous: Option<M>,
    replacement: M,
    foreign_key: <M::Entity as EntityTrait>::Column,
    parent_key: Value,
    transaction: &DatabaseTransaction,
) -> Result<M, DatabaseError>
where
    M: ModelTrait + IntoActiveModel<<M::Entity as EntityTrait>::ActiveModel>,
    M::Entity: EntityTrait<Model = M>,
    <M::Entity as EntityTrait>::ActiveModel:
        ActiveModelTrait<Entity = M::Entity> + ActiveModelBehavior + Send,
{
    let connection = transaction.connection();
    if let Some(previous) = previous {
        detach_one_to_many(previous, foreign_key, connection).await?;
    }
    attach_one_to_many(replacement, foreign_key, parent_key, connection).await
}

async fn update_relation_key<M, C>(
    model: M,
    column: <M::Entity as EntityTrait>::Column,
    value: Value,
    connection: &C,
) -> Result<M, DatabaseError>
where
    M: ModelTrait + IntoActiveModel<<M::Entity as EntityTrait>::ActiveModel>,
    M::Entity: EntityTrait<Model = M>,
    <M::Entity as EntityTrait>::ActiveModel:
        ActiveModelTrait<Entity = M::Entity> + ActiveModelBehavior + Send,
    C: ConnectionTrait,
{
    let mut active_model = model.into_active_model();
    if active_model.get_primary_key_value().is_none() {
        return Err(DatabaseError::InvalidModelState(
            "a relation mutation requires a complete primary key",
        ));
    }
    active_model
        .try_set(column, value)
        .map_err(DatabaseError::OrmQuery)?;
    active_model
        .update(connection)
        .await
        .map_err(DatabaseError::OrmQuery)
}
