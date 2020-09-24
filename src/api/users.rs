use futures::TryFutureExt;
use juniper::{GraphQLInputObject, GraphQLObject};
use serde::{Deserialize, Serialize};
use slog::info;
use snafu::ResultExt;
use sqlx::Connection;
use std::convert::TryFrom;

use crate::api::gql::Context;
use crate::api::model::*;
use crate::auth;
use crate::db::model::ProvideAuthn;
use crate::db::model::ProvideData;
use crate::db::Db;
use crate::error;
// use crate::state::{argon, jwt};
// use crate::fsm;

/// The response body for single user
/// It is optional, since we may be looking for a user which
/// does not match the query criteria.
#[derive(Debug, Deserialize, Serialize, GraphQLObject)]
#[serde(rename_all = "camelCase")]
pub struct SingleUserResponseBody {
    pub user: Option<User>,
}

impl From<User> for SingleUserResponseBody {
    fn from(user: User) -> Self {
        Self { user: Some(user) }
    }
}

/// The response body for a user login
#[derive(Debug, Deserialize, Serialize, GraphQLObject)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatedUserResponseBody {
    pub user: User,
    pub token: String,
}

impl From<(User, String)> for AuthenticatedUserResponseBody {
    fn from(auth: (User, String)) -> Self {
        Self {
            user: auth.0,
            token: auth.1,
        }
    }
}

/// The response body for multiple users
#[derive(Debug, Deserialize, Serialize, GraphQLObject)]
#[serde(rename_all = "camelCase")]
pub struct MultiUsersResponseBody {
    pub users: Vec<User>,
    pub users_count: i32,
}

impl From<Vec<User>> for MultiUsersResponseBody {
    fn from(users: Vec<User>) -> Self {
        let users_count = i32::try_from(users.len()).unwrap();
        Self { users, users_count }
    }
}

/// The query body for creating (registering) a user
#[derive(Debug, Serialize, Deserialize, GraphQLInputObject)]
pub struct UserRequestBody {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// The query body for login a user
#[derive(Debug, Serialize, Deserialize, GraphQLInputObject)]
pub struct CredentialsRequestBody {
    pub username: String,
    pub password: String,
}

/// Retrieve all users
pub async fn list_users(context: &Context) -> Result<MultiUsersResponseBody, error::Error> {
    async move {
        let pool = &context.state.pool;

        let mut tx = pool
            .conn()
            .and_then(Connection::begin)
            .await
            .context(error::DBError {
                msg: "could not initiate transaction",
            })?;

        let entities = tx.get_all_users().await.context(error::DBProvideError {
            msg: "Could not get all them users",
        })?;

        let users = entities.into_iter().map(User::from).collect::<Vec<_>>();

        tx.commit().await.context(error::DBError {
            msg: "could not commit transaction",
        })?;

        Ok(MultiUsersResponseBody::from(users))
    }
    .await
}

/// Create a new user.
pub async fn add_user(
    user_request: UserRequestBody,
    context: &Context,
) -> Result<SingleUserResponseBody, error::Error> {
    async move {
        let UserRequestBody {
            username,
            email,
            password,
        } = user_request;

        let pool = &context.state.pool;

        let mut tx = pool
            .conn()
            .and_then(Connection::begin)
            .await
            .context(error::DBError {
                msg: "could not initiate transaction",
            })?;

        let entity = ProvideData::create_user(
            &mut tx as &mut sqlx::PgConnection,
            &username,
            &email,
            &password,
        )
        .await
        .context(error::DBProvideError {
            msg: "Could not create user",
        })?;

        let user = User::from(entity);

        tx.commit().await.context(error::DBError {
            msg: "could not retrieve indexes",
        })?;

        Ok(SingleUserResponseBody::from(user))
    }
    .await
}

/// Register a new user.
/// This is really the same thing as new user...
pub async fn register_user(
    user_request: UserRequestBody,
    context: &Context,
) -> Result<SingleUserResponseBody, error::Error> {
    async move {
        let UserRequestBody {
            username,
            email,
            password,
        } = user_request;

        let password = context
            .state
            .argon
            .hasher()
            .with_password(password)
            .hash()
            .map_err(|err| error::Error::HasherError {
                msg: format!("could not hash password: {}", err),
            })?;

        let pool = &context.state.pool;

        let mut tx = pool
            .conn()
            .and_then(Connection::begin)
            .await
            .context(error::DBError {
                msg: "could not initiate register user transaction",
            })?;

        let entity = ProvideAuthn::create_user(
            &mut tx as &mut sqlx::PgConnection,
            &username,
            &email,
            &password,
        )
        .await
        .context(error::DBProvideError {
            msg: "Could not create user",
        })?;

        let user = User::from(entity);

        tx.commit().await.context(error::DBError {
            msg: "could not commit register user transaction",
        })?;

        Ok(SingleUserResponseBody::from(user))
    }
    .await
}

/// Retrieve a single user given its username
pub async fn find_user_by_username(
    context: &Context,
    username: &str,
) -> Result<SingleUserResponseBody, error::Error> {
    async move {
        let pool = &context.state.pool;

        let mut tx = pool
            .conn()
            .and_then(Connection::begin)
            .await
            .context(error::DBError {
                msg: "could not initiate transaction",
            })?;

        let entity = tx
            .get_user_by_username(username)
            .await
            .context(error::DBProvideError {
                msg: "Could not get user by username",
            });

        match entity {
            Err(err) => {
                info!(context.state.logger, "DB Provide Error: {:?}", err);
                Err(err)
            }
            Ok(entity) => {
                tx.commit().await.context(error::DBError {
                    msg: "could not commit transaction",
                })?;
                match entity {
                    None => Ok(SingleUserResponseBody { user: None }),
                    Some(entity) => {
                        let user = User::from(entity);
                        Ok(SingleUserResponseBody::from(user))
                    }
                }
            }
        }
    }
    .await
}

/// user login
pub async fn login_user(
    credentials: CredentialsRequestBody,
    context: &Context,
) -> Result<AuthenticatedUserResponseBody, error::Error> {
    async move {
        // First we lookup an account based on the username
        // If there is no such account, return Ok(None)
        // 2. Compare using password hasher
        //
        // I am not reusing the find_user_by_username function because it
        // doesn't return enough information.
        let pool = &context.state.pool;

        let mut tx = pool
            .conn()
            .and_then(Connection::begin)
            .await
            .context(error::DBError {
                msg: "could not initiate transaction",
            })?;

        let entity = tx
            .get_user_by_username(&credentials.username)
            .await
            .context(error::DBProvideError {
                msg: "Could not get user by username",
            })?;

        if entity.is_none() {
            info!(context.state.logger, "Cannot find user");
            return Err(error::Error::MiscError {
                msg: String::from("Unknown user"),
            });
        }

        let entity = entity.unwrap();
        tx.commit().await.context(error::DBError {
            msg: "could not commit transaction",
        })?;

        let is_valid = context
            .state
            .argon
            .verifier()
            .with_hash(&entity.password)
            .with_password(credentials.password)
            .verify()
            .map_err(|err| error::Error::HasherError {
                msg: format!("could not verify password: {}", err),
            })?;

        if !is_valid {
            return Err(error::Error::MiscError {
                msg: String::from("Invalid credentials"),
            });
        }

        // User is authenticated, so build the jwt token
        let claims = auth::PrivateClaims {
            roles: entity
                .roles
                .iter()
                .map(|role| String::from(role))
                .collect::<Vec<String>>(),
        };

        let user = User::from(entity);
        let token = context.state.jwt.encode(claims)?;

        Ok(AuthenticatedUserResponseBody::from((user, token)))
    }
    .await
}
