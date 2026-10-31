use rustflow::ModelDiagnostics;
use rustflow_server::sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, ModelDiagnostics, DeriveEntityModel)]
#[sea_orm(table_name = "products")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub sku: String,
    pub description: Option<String>,
    pub price_cents: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
