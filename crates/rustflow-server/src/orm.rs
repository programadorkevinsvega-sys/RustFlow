//! Typed Data Mapper helpers built on SeaORM.
//!
//! Repositories borrow an existing connection or transaction. They never create pools
//! and do not perform lazy database I/O when relation fields are read.

use std::marker::PhantomData;

use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ConnectionTrait, DeleteMany, DeleteResult, EntityTrait,
    InsertResult, IntoActiveModel, LoaderTrait, ModelTrait, PrimaryKeyTrait, Related, Select,
    UpdateMany,
};

use crate::database::DatabaseError;

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

    /// Insert if the generated primary key is NotSet; otherwise update the model.
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

/// Batch-load a has-one relation for a slice of parent models.
pub async fn load_one<M, R, C>(
    parents: &[M],
    related: R,
    connection: &C,
) -> Result<Vec<Option<R::Model>>, DatabaseError>
where
    M: ModelTrait + Send + Sync,
    M::Entity: Related<R>,
    R: EntityTrait,
    R::Model: Send + Sync,
    C: ConnectionTrait,
{
    parents
        .load_one(related, connection)
        .await
        .map_err(DatabaseError::OrmQuery)
}

/// Batch-load a has-many relation with one relation query per hop, avoiding N+1.
pub async fn load_many<M, R, C>(
    parents: &[M],
    related: R,
    connection: &C,
) -> Result<Vec<Vec<R::Model>>, DatabaseError>
where
    M: ModelTrait + Send + Sync,
    M::Entity: Related<R>,
    R: EntityTrait,
    R::Model: Send + Sync,
    C: ConnectionTrait,
{
    parents
        .load_many(related, connection)
        .await
        .map_err(DatabaseError::OrmQuery)
}

/// Batch-load a many-to-many relation through its explicit junction entity.
pub async fn load_many_to_many<M, R, V, C>(
    parents: &[M],
    related: R,
    via: V,
    connection: &C,
) -> Result<Vec<Vec<R::Model>>, DatabaseError>
where
    M: ModelTrait + Send + Sync,
    M::Entity: Related<R>,
    R: EntityTrait,
    R::Model: Send + Sync,
    V: EntityTrait,
    V::Model: Send + Sync,
    C: ConnectionTrait,
{
    parents
        .load_many_to_many(related, via, connection)
        .await
        .map_err(DatabaseError::OrmQuery)
}
