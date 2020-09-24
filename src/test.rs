use clap::ArgMatches;
// use cucumber::{
//     after, before, steps, CucumberBuilder, DefaultOutput, OutputVisitor, Scenario, Steps,
// };
// use futures::future::TryFutureExt;
use slog::{info, Logger};
use slog::{o, Drain};
// use snafu::futures::try_future::TryFutureExt as SnafuTryFutureExt;
// use std::path::Path;
use std::thread;

use super::server::run_server;
// use environments::api::client::blocking::list_containers;
// use environments::api::containers::MultiContainersResponseBody;
// use environments::db::pg;
use environments::error;
use environments::settings::Settings;
use environments::state::State;
// use environments::utils::{construct_headers, get_database_url, get_service_url};

#[allow(clippy::needless_lifetimes)]
pub async fn test<'a>(matches: &ArgMatches<'a>, logger: Logger) -> Result<(), error::Error> {
    let settings = Settings::new(matches)?;

    // FIXME There is work that should be done here to terminate the service
    // when we are done with testing.
    if settings.testing {
        info!(logger, "Launching testing service");
        let handle = tokio::runtime::Handle::current();
        thread::spawn(move || {
            handle.spawn(async {
                let decorator = slog_term::TermDecorator::new().build();
                let drain = slog_term::FullFormat::new(decorator).build().fuse();
                let drain = slog_async::Async::new(drain).build().fuse();
                let logger = slog::Logger::root(drain, o!());
                let state = State::new(&settings, &logger)
                    .await
                    .expect("state creation");
                let _ = run_server(settings, state).await;
            });
        });
        //th.join().expect("Waiting for test execution");
    }

    test_environments();
    Ok(())
}

pub fn test_environments() {}
