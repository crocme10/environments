use juniper::{EmptySubscription, FieldResult, IntoFieldError, RootNode};
use slog::info;

use super::containers;
use crate::state::State;

#[derive(Debug, Clone)]
pub struct Context {
    pub state: State,
    pub token: Option<String>,
}

impl juniper::Context for Context {}

pub struct Query;

#[juniper::graphql_object(
    Context = Context
)]
impl Query {
    /// Returns a list of containers
    async fn containers(
        &self,
        context: &Context,
    ) -> FieldResult<containers::MultiContainersResponseBody> {
        if let Some(token) = &context.token {
            info!(context.state.logger, "auth token: {}", token);
        }
        containers::list_containers(context)
            .await
            .map_err(IntoFieldError::into_field_error)
            .into()
    }

    // /// Find a container by name
    // async fn findContainerByName(
    //     &self,
    //     name: String,
    //     context: &Context,
    // ) -> FieldResult<containers::SingleContainerResponseBody> {
    //     containers::find_container_by_name(context, &name)
    //         .await
    //         .map_err(IntoFieldError::into_field_error)
    // }
}

pub struct Mutation;

#[juniper::graphql_object(
    Context = Context
)]
impl Mutation {
    async fn create_container(
        &self,
        container: containers::ContainerRequestBody,
        context: &Context,
    ) -> FieldResult<containers::SingleContainerResponseBody> {
        containers::create_container(container, context)
            .await
            .map_err(IntoFieldError::into_field_error)
    }
}

type Schema = RootNode<'static, Query, Mutation, EmptySubscription<Context>>;

pub fn schema() -> Schema {
    Schema::new(Query, Mutation, EmptySubscription::new())
}
