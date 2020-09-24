// use bollard::container::ListContainersOptions;
use futures::TryFutureExt;
use juniper::{GraphQLInputObject, GraphQLObject};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use sqlx::Connection;
// use std::collections::HashMap;
use std::convert::TryFrom;
// use std::default::Default;

use crate::api::gql::Context;
use crate::api::model::*;
use crate::db::model::ProvideData;
use crate::db::Db;
use crate::error;

/// The response body for single container
/// It is optional, since we may be looking for a user which
/// does not match the query criteria.
#[derive(Debug, Deserialize, Serialize, GraphQLObject)]
#[serde(rename_all = "camelCase")]
pub struct SingleContainerResponseBody {
    pub container: Option<Container>,
}

impl From<Container> for SingleContainerResponseBody {
    fn from(container: Container) -> Self {
        Self {
            container: Some(container),
        }
    }
}

/// The response body for multiple containers
#[derive(Debug, Deserialize, Serialize, GraphQLObject)]
#[serde(rename_all = "camelCase")]
pub struct MultiContainersResponseBody {
    pub containers: Vec<Container>,
    pub containers_count: i32,
}

impl From<Vec<Container>> for MultiContainersResponseBody {
    fn from(containers: Vec<Container>) -> Self {
        let containers_count = i32::try_from(containers.len()).unwrap();
        Self {
            containers,
            containers_count,
        }
    }
}

/// The query body for creating a new container
#[derive(Debug, Serialize, Deserialize, GraphQLInputObject)]
pub struct ContainerRequestBody {
    pub name: String,
    pub image: String,
}

/// Retrieve all containers
pub async fn list_containers(
    context: &Context,
) -> Result<MultiContainersResponseBody, error::Error> {
    async move {
        let pool = &context.state.pool;

        let mut tx = pool
            .conn()
            .and_then(Connection::begin)
            .await
            .context(error::DBError {
                msg: "could not initiate transaction",
            })?;

        let entities = tx
            .get_all_containers()
            .await
            .context(error::DBProvideError {
                msg: "Could not get all them containers",
            })?;

        let containers = entities
            .into_iter()
            .map(Container::from)
            .collect::<Vec<_>>();

        tx.commit().await.context(error::DBError {
            msg: "could not commit transaction",
        })?;

        Ok(MultiContainersResponseBody::from(containers))
    }
    .await
}

/// Create a new container
pub async fn create_container(
    container_request: ContainerRequestBody,
    context: &Context,
) -> Result<SingleContainerResponseBody, error::Error> {
    async move {
        let ContainerRequestBody { name, image } = container_request;

        let pool = &context.state.pool;

        let mut tx = pool
            .conn()
            .and_then(Connection::begin)
            .await
            .context(error::DBError {
                msg: "could not initiate transaction",
            })?;

        let entity =
            ProvideData::create_container(&mut tx as &mut sqlx::PgConnection, &name, &image)
                .await
                .context(error::DBProvideError {
                    msg: "Could not create container",
                })?;

        let container = Container::from(entity);

        tx.commit().await.context(error::DBError {
            msg: "could not commit container creation transaction",
        })?;

        Ok(SingleContainerResponseBody::from(container))
    }
    .await
}
