use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, StartContainerOptions,
};
use bollard::image::CreateImageOptions;
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::{future, TryFutureExt, TryStreamExt};
use juniper::{GraphQLInputObject, GraphQLObject};
use serde::{Deserialize, Serialize};
use slog::{info, warn};
use snafu::ResultExt;
use sqlx::Connection;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::default::Default;

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
/// This function will work in 4 steps.
/// 1. Get a list of containers from the database
/// 2. Query the docker engine to get up-to-date information
/// 3. Update the database
/// 4. Formulate the response.
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

        let ids: Vec<&str> = entities
            .iter()
            .map(|entity| entity.id.as_ref())
            .collect::<Vec<_>>();

        tx.commit().await.context(error::DBError {
            msg: "could not commit transaction",
        })?;

        let mut filters = HashMap::new();
        filters.insert("id", ids);

        let options = Some(ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        });

        let container_summaries = context
            .state
            .docker
            .list_containers(options)
            .await
            .context(error::BollardError {
                msg: "Could not list containers",
            })?;

        let containers = container_summaries
            .into_iter()
            .map(|summary| Container {
                id: summary.id.unwrap_or(String::from("NA")),
                name: String::from("NA"),
                image: summary.image.unwrap_or(String::from("NA")),
                created_at: DateTime::<Utc>::from_utc(
                    NaiveDateTime::from_timestamp(summary.created.unwrap_or(0), 0),
                    Utc,
                ),
                updated_at: DateTime::<Utc>::from_utc(
                    NaiveDateTime::from_timestamp(summary.created.unwrap_or(0), 0),
                    Utc,
                ),
            })
            .collect::<Vec<_>>();

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

        info!(context.state.logger, "Creating image {}", &image);

        let options = Some(CreateImageOptions {
            from_image: image.clone(),
            ..Default::default()
        });

        context
            .state
            .docker
            .create_image(options, None, None)
            .try_for_each(|info| {
                info!(context.state.logger, "image: {:?}", info);
                future::ready(Ok(()))
            })
            .await
            .context(error::BollardError {
                msg: "Could not create image",
            })?;

        let options = Some(CreateContainerOptions { name: name.clone() });

        let config = Config {
            image: Some(image.clone()),
            //cmd: Some(vec!["/hello"]),
            ..Default::default()
        };

        let resp = context
            .state
            .docker
            .create_container(options, config)
            .await
            .context(error::BollardError {
                msg: "Could not create container",
            })?;

        resp.warnings
            .iter()
            .for_each(|warning| warn!(context.state.logger, "hey! {}", warning));

        context
            .state
            .docker
            .start_container(&name, None::<StartContainerOptions<String>>)
            .await
            .context(error::BollardError {
                msg: "Could not start container",
            })?;

        let pool = &context.state.pool;

        let mut tx = pool
            .conn()
            .and_then(Connection::begin)
            .await
            .context(error::DBError {
                msg: "could not initiate transaction",
            })?;

        let entity = ProvideData::create_container(
            &mut tx as &mut sqlx::PgConnection,
            &resp.id,
            &name,
            &image,
        )
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
