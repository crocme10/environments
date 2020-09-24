use async_trait::async_trait;
use chrono::{DateTime, Utc};
use slog::{debug, info, o, Logger};
use snafu::ResultExt;
use sqlx::error::DatabaseError;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgError, PgQueryAs, PgRow};
use sqlx::row::{FromRow, Row};
use sqlx::{PgConnection, PgPool};
use std::convert::TryFrom;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::model;
use super::Db;
use crate::error;

/// A user registered with the application (Postgres version)
pub struct ContainerEntity {
    pub id: String,
    pub name: String,
    pub image: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl<'c> FromRow<'c, PgRow<'c>> for ContainerEntity {
    fn from_row(row: &PgRow<'c>) -> Result<Self, sqlx::Error> {
        Ok(ContainerEntity {
            id: row.get(0),
            name: row.get(1),
            image: row.get(2),
            created_at: row.get(3),
            updated_at: row.get(4),
        })
    }
}

impl From<ContainerEntity> for model::ContainerEntity {
    fn from(pg: ContainerEntity) -> Self {
        let ContainerEntity {
            id,
            name,
            image,
            created_at,
            updated_at,
        } = pg;

        model::ContainerEntity {
            id,
            name,
            image,
            created_at,
            updated_at,
        }
    }
}

/// Open a connection to a database
pub async fn connect(db_url: &str) -> sqlx::Result<PgPool> {
    let pool = PgPool::new(db_url).await?;
    Ok(pool)
}

impl TryFrom<&PgError> for model::ProvideError {
    type Error = ();

    /// Attempt to convert a Postgres error into a generic ProvideError
    ///
    /// Unexpected cases will be bounced back to the caller for handling
    ///
    /// * [Postgres Error Codes](https://www.postgresql.org/docs/current/errcodes-appendix.html)
    fn try_from(pg_err: &PgError) -> Result<Self, Self::Error> {
        let provider_err = match pg_err.code().unwrap() {
            "23505" => model::ProvideError::UniqueViolation {
                details: pg_err.details().unwrap().to_owned(),
            },
            code if code.starts_with("23") => model::ProvideError::ModelViolation {
                details: pg_err.message().to_owned(),
            },
            _ => return Err(()),
        };

        Ok(provider_err)
    }
}

#[async_trait]
impl Db for PgPool {
    type Conn = PoolConnection<PgConnection>;

    async fn conn(&self) -> Result<Self::Conn, sqlx::Error> {
        self.acquire().await
    }
}

#[async_trait]
impl model::ProvideData for PgConnection {
    async fn create_container(
        &mut self,
        id: &str,
        name: &str,
        image: &str,
    ) -> model::ProvideResult<model::ContainerEntity> {
        let container: ContainerEntity = sqlx::query_as(
            r#"
INSERT INTO main.containers ( id, name, image )
VALUES ( $1, $2, $3 )
RETURNING *
        "#,
        )
        .bind(id)
        .bind(name)
        .bind(image)
        .fetch_one(self)
        .await?;

        Ok(container.into())
    }

    async fn get_all_containers(&mut self) -> model::ProvideResult<Vec<model::ContainerEntity>> {
        let containers: Vec<ContainerEntity> = sqlx::query_as(
            r#"
SELECT *
FROM main.containers
ORDER BY created_at
            "#,
        )
        .fetch_all(self)
        .await?;

        let containers = containers
            .into_iter()
            .map(model::ContainerEntity::from)
            .collect::<Vec<_>>();

        Ok(containers)
    }

    async fn get_container_by_name(
        &mut self,
        name: &str,
    ) -> model::ProvideResult<Option<model::ContainerEntity>> {
        let container: Option<ContainerEntity> = sqlx::query_as(
            r#"
SELECT *
FROM main.containers
WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(self)
        .await?;

        match container {
            None => Ok(None),
            Some(container) => {
                let container = model::ContainerEntity::from(container);
                Ok(Some(container))
            }
        }
    }
}

/// A user registered with the application (Postgres version)
pub struct UserEntity {
    pub id: model::EntityId,
    pub username: String,
    pub email: String,
    pub password: String,
    pub roles: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl<'c> FromRow<'c, PgRow<'c>> for UserEntity {
    fn from_row(row: &PgRow<'c>) -> Result<Self, sqlx::Error> {
        Ok(UserEntity {
            id: row.get(0),
            username: row.get(1),
            email: row.get(2),
            password: row.get(3),
            roles: row.get(4),
            active: row.get(5),
            created_at: row.get(6),
            updated_at: row.get(7),
        })
    }
}

impl From<UserEntity> for model::UserEntity {
    fn from(pg: UserEntity) -> Self {
        let UserEntity {
            id,
            username,
            email,
            password,
            roles,
            active,
            created_at,
            updated_at,
        } = pg;

        model::UserEntity {
            id,
            username,
            email,
            password,
            roles,
            active,
            created_at,
            updated_at,
        }
    }
}

#[async_trait]
impl model::ProvideAuthn for PgConnection {
    async fn create_user(
        &mut self,
        username: &str,
        email: &str,
        password: &str,
    ) -> model::ProvideResult<model::UserEntity> {
        let user: UserEntity = sqlx::query_as(
            r#"
INSERT INTO main.users ( username, email, password )
VALUES ( $1, $2, $3 )
RETURNING *
        "#,
        )
        .bind(username)
        .bind(email)
        .bind(password)
        .fetch_one(self)
        .await?;

        Ok(user.into())
    }

    async fn get_user_by_id(
        &mut self,
        user_id: model::EntityId,
    ) -> model::ProvideResult<Option<model::UserEntity>> {
        let user: Option<UserEntity> = sqlx::query_as(
            r#"
SELECT *
FROM main.users
WHERE id = $1
            "#,
        )
        .bind(user_id)
        .fetch_optional(self)
        .await?;

        match user {
            None => Ok(None),
            Some(user) => {
                let user = model::UserEntity::from(user);
                Ok(Some(user))
            }
        }
    }

    async fn get_user_by_email(
        &mut self,
        email: &str,
    ) -> model::ProvideResult<Option<model::UserEntity>> {
        let user: Option<UserEntity> = sqlx::query_as(
            r#"
SELECT *
FROM main.users
WHERE email = $1
            "#,
        )
        .bind(email)
        .fetch_optional(self)
        .await?;

        match user {
            None => Ok(None),
            Some(user) => {
                let user = model::UserEntity::from(user);
                Ok(Some(user))
            }
        }
    }

    async fn update_user(
        &mut self,
        updated: &model::UserEntity,
    ) -> model::ProvideResult<model::UserEntity> {
        let user: UserEntity = sqlx::query_as(
            r#"
UPDATE main.users
SET email = $1, username = $2, password = $3, updated_at = DEFAULT
WHERE id = $4
RETURNING *
            "#,
        )
        .bind(updated.email.clone())
        .bind(updated.username.clone())
        .bind(updated.password.clone())
        .bind(updated.id)
        .fetch_one(self)
        .await?;

        Ok(user.into())
    }
}

pub async fn init_db(conn_str: &str, logger: Logger) -> Result<(), error::Error> {
    info!(logger, "Initializing  DB @ {}", conn_str);
    migration_down(conn_str, &logger).await?;
    migration_up(conn_str, &logger).await?;
    Ok(())
}

pub async fn migration_up(conn_str: &str, logger: &Logger) -> Result<(), error::Error> {
    let clogger = logger.new(o!("database" => String::from(conn_str)));
    debug!(clogger, "Movine Up");
    // This is essentially running 'psql $DATABASE_URL < db/init.sql', and logging the
    // psql output.
    // FIXME This relies on a command psql, which is not desibable.
    // We could alternatively try to use sqlx...
    // There may be a tool for doing migrations.
    let mut cmd = Command::new("movine");
    cmd.env("DATABASE_URL", conn_str);
    cmd.arg("up");
    cmd.stdout(Stdio::piped());

    let mut child = cmd.spawn().context(error::TokioIOError {
        msg: String::from("Failed to execute movine"),
    })?;

    let stdout = child.stdout.take().ok_or(error::Error::MiscError {
        msg: String::from("child did not have a handle to stdout"),
    })?;

    let mut reader = BufReader::new(stdout).lines();

    // Ensure the child process is spawned in the runtime so it can
    // make progress on its own while we await for any output.
    tokio::spawn(async {
        // FIXME Need to do something about logging this and returning an error.
        let _status = child.await.expect("child process encountered an error");
        // println!("child status was: {}", status);
    });
    debug!(clogger, "Spawned migration up");

    while let Some(line) = reader.next_line().await.context(error::TokioIOError {
        msg: String::from("Could not read from piped output"),
    })? {
        debug!(clogger, "movine: {}", line);
    }

    Ok(())
}

pub async fn migration_down(conn_str: &str, logger: &Logger) -> Result<(), error::Error> {
    let clogger = logger.new(o!("database" => String::from(conn_str)));
    debug!(clogger, "Movine Down");
    // This is essentially running 'psql $DATABASE_URL < db/init.sql', and logging the
    // psql output.
    // FIXME This relies on a command psql, which is not desibable.
    // We could alternatively try to use sqlx...
    // There may be a tool for doing migrations.
    let mut cmd = Command::new("movine");
    cmd.env("DATABASE_URL", conn_str);
    cmd.arg("down");
    cmd.stdout(Stdio::piped());

    let mut child = cmd.spawn().context(error::TokioIOError {
        msg: String::from("Failed to execute movine"),
    })?;

    let stdout = child.stdout.take().ok_or(error::Error::MiscError {
        msg: String::from("child did not have a handle to stdout"),
    })?;

    let mut reader = BufReader::new(stdout).lines();

    // Ensure the child process is spawned in the runtime so it can
    // make progress on its own while we await for any output.
    tokio::spawn(async {
        // FIXME Need to do something about logging this and returning an error.
        let _status = child.await.expect("child process encountered an error");
        // println!("child status was: {}", status);
    });
    debug!(clogger, "Spawned migration down");

    while let Some(line) = reader.next_line().await.context(error::TokioIOError {
        msg: String::from("Could not read from piped output"),
    })? {
        debug!(clogger, "movine: {}", line);
    }

    Ok(())
}
