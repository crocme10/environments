use argon::Argon;
use bollard::Docker;
use jwt::Jwt;
use slog::{info, o, Logger};
use snafu::ResultExt;
use sqlx::postgres::PgPool;
use sqlx::prelude::PgQueryAs;

use crate::error;
use crate::settings::Settings;

pub mod argon;
pub mod jwt;

#[derive(Clone, Debug)]
pub struct State {
    pub pool: PgPool,
    pub logger: Logger,
    pub argon: Argon,
    pub jwt: Jwt,
    pub docker: Docker,
}

impl State {
    pub async fn new(settings: &Settings, logger: &Logger) -> Result<Self, error::Error> {
        let pool = PgPool::builder()
            .max_size(5)
            .build(&settings.database.url)
            .await
            .context(error::DBError {
                msg: String::from("foo"),
            })?;
        // FIXME ping the pool to know quickly if we have a db connection

        let row: (String,) = sqlx::query_as("SELECT version()")
            .fetch_one(&pool)
            .await
            .context(error::DBError {
                msg: format!(
                    "Could not test database version for {}",
                    &settings.database.url,
                ),
            })?;

        info!(logger, "db version: {:?}", row.0);

        let logger = logger.new(
            o!("host" => String::from(&settings.service.host), "port" => settings.service.port, "database" => String::from(&settings.database.url)),
        );
        let argon = Argon::new(&settings);
        let jwt = Jwt::new(&settings);

        let docker = Docker::connect_with_unix_defaults().context(error::BollardError {
            msg: String::from("Could not establish connection with docker engine"),
        })?;

        let version = docker.version().await.context(error::BollardError {
            msg: String::from("Could not get docker version"),
        })?;

        info!(logger, "docker version: {:?}", version);
        Ok(Self {
            pool,
            logger,
            argon,
            jwt,
            docker,
        })
    }
}
