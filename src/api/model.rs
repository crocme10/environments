use chrono::{DateTime, Utc};
use juniper::GraphQLObject;
use serde::{Deserialize, Serialize};

use crate::db::model::*;

/// A container
#[derive(Debug, Deserialize, Serialize, GraphQLObject)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ContainerEntity> for Container {
    fn from(entity: ContainerEntity) -> Self {
        let ContainerEntity {
            id,
            name,
            image,
            created_at,
            updated_at,
            ..
        } = entity;

        Container {
            id,
            name,
            image,
            created_at,
            updated_at,
        }
    }
}
