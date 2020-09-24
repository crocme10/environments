use clap::ArgMatches;
use slog::{info, Logger};

use environments::db;
use environments::error;
use environments::settings::Settings;

#[allow(clippy::needless_lifetimes)]
pub async fn init<'a>(matches: &ArgMatches<'a>, logger: Logger) -> Result<(), error::Error> {
    info!(logger, "Initiazing application");
    let settings = Settings::new(matches)?;

    info!(logger, "Mode: {}", settings.mode);

    if settings.debug {
        info!(logger, "Database URL: {}", settings.database.url);
    }

    // FIXME Here I hardcode, in the form of the path to the module, that we're using
    // a postgres database...
    db::pg::init_db(&settings.database.url, logger).await
}
