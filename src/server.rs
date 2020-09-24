use clap::ArgMatches;
use environments::api::gql;
use environments::error;
use environments::settings::Settings;
use environments::state::State;
use slog::{info, Logger};
use snafu::ResultExt;
use std::net::ToSocketAddrs;
use warp::{self, http, Filter};

#[allow(clippy::needless_lifetimes)]
pub async fn run<'a>(matches: &ArgMatches<'a>, logger: Logger) -> Result<(), error::Error> {
    let settings = Settings::new(matches)?;
    let state = State::new(&settings, &logger).await?;
    run_server(settings, state).await
}

pub async fn run_server(settings: Settings, state: State) -> Result<(), error::Error> {
    // We keep a copy of the logger before the context takes ownership of it.
    let logger = state.logger.clone();

    let state = warp::any().map(move || state.clone());

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST"])
        .allow_headers(vec!["content-type", "authorization"])
        .allow_any_origin()
        .build();

    let log = warp::log("foo");

    let auth = warp::header::<String>("authorization")
        .map(|bearer: String| match bearer.strip_prefix("Bearer ") {
            Some(token) => Some(String::from(token)),
            None => None,
        })
        .or(warp::any().map(|| None))
        .unify();

    let context = warp::any()
        .and(state.clone())
        .and(auth)
        .map(move |state, token| gql::Context { state, token });

    let playground = warp::get()
        .and(warp::path("playground"))
        .and(playground_filter("/graphql", Some("/subscriptions")));

    let graphql_filter = juniper_warp::make_graphql_filter(gql::schema(), context.boxed());

    let graphql = warp::post().and(warp::path("graphql")).and(graphql_filter);

    let routes = playground.or(graphql).with(cors).with(log);

    let host = settings.service.host;
    let port = settings.service.port;
    let addr = (host.as_str(), port);
    let addr = addr
        .to_socket_addrs()
        .context(error::IOError {
            msg: String::from("To Sock Addr"),
        })?
        .next()
        .ok_or(error::Error::MiscError {
            msg: String::from("Cannot resolve addr"),
        })?;

    info!(logger, "Serving Environments");
    warp::serve(routes).run(addr).await;

    Ok(())
}

/// Create a filter that replies with an HTML page containing GraphQL Playground.
/// This does not handle routing, so you can mount it on any endpoint.
pub fn playground_filter(
    graphql_endpoint_url: &'static str,
    subscriptions_endpoint_url: Option<&'static str>,
) -> warp::filters::BoxedFilter<(http::Response<Vec<u8>>,)> {
    warp::any()
        .map(move || playground_response(graphql_endpoint_url, subscriptions_endpoint_url))
        .boxed()
}

fn playground_response(
    graphql_endpoint_url: &'static str,
    subscriptions_endpoint_url: Option<&'static str>,
) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .header("content-type", "text/html;charset=utf-8")
        .body(
            juniper::http::playground::playground_source(
                graphql_endpoint_url,
                subscriptions_endpoint_url,
            )
            .into_bytes(),
        )
        .expect("response is valid")
}
