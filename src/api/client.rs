use futures::future::TryFutureExt;
use snafu::futures::try_future::TryFutureExt as SnafuTryFutureExt;
use snafu::ResultExt;

use super::containers::MultiContainersResponseBody;
use crate::error;
use crate::utils::{construct_headers, get_service_url};

// Request a list of containers.
// TODO We rely on a helper function `get_service_url` to identify the target service
// but this is probably not the best solution. Maybe the service's url needs to be
// passed as another function argument.
pub async fn list_containers() -> Result<MultiContainersResponseBody, error::Error> {
    let data = get_graphql_str_list_containers();
    let url = get_service_url();
    let client = reqwest::Client::new();
    client
        .post(&url)
        .headers(construct_headers())
        .body(data)
        .send()
        .context(error::ReqwestError {
            msg: String::from("Could not query containers"),
        })
        .and_then(|resp| {
            resp.json::<serde_json::Value>()
                .context(error::ReqwestError {
                    msg: String::from("Could not deserialize MultiContainersResponseBody"),
                })
        })
        .and_then(|json| {
            // This json object can be either { data: { containers: { } } } if the call was successful,
            // or { data: null, error: [ ] } if the call was not successful,
            // FIXME Lots of unwrap in the following code, also I don't extract the 'message' part
            // of the error.
            async move {
                let data = &json["data"];
                if data.is_null() {
                    if let Some(errors) = json.get("errors") {
                        let errors = errors.clone();
                        let errors = errors.as_array().expect("errors is an array");
                        let error = &errors.first().expect("at least one error");
                        let msg = error
                            .get("extensions")
                            .expect("error to have an extension field")
                            .get("internal_error")
                            .expect("extension to have an internal_error field");
                        return Err(error::Error::MiscError {
                            msg: format!("Error while requesting containers: {}", msg),
                        });
                    } else {
                        return Err(error::Error::MiscError {
                            msg: String::from("Data is null, and there are no errors."),
                        });
                    }
                } else {
                    if let Some(containers) = data.get("containers") {
                        let containers = containers.clone();
                        serde_json::from_value(containers).context(error::JSONError {
                            msg: String::from("Could not deserialize containers"),
                        })
                    } else {
                        Err(error::Error::MiscError {
                            msg: String::from("Data is not null, and there are no containers."),
                        })
                    }
                }
            }
        })
        .await
}

// This is a helper function which generates the GraphQL query for listing containers
pub fn get_graphql_str_list_containers() -> String {
    String::from("{ \"query\": \"{ containers { containers { name }, containersCount } }\" }")
}

pub mod blocking {
    use crate::api::containers::MultiContainersResponseBody;
    use crate::error;
    pub fn list_containers() -> Result<MultiContainersResponseBody, error::Error> {
        // We use the Client API, which is async, so we need to wrap it around some
        // tokio machinery to spin the async code in a thread, and wait for the result.
        let handle = tokio::runtime::Handle::current();
        let th = std::thread::spawn(move || {
            match handle.block_on(async { super::list_containers().await }) {
                Ok(m) => Ok(m),
                Err(err) => Err(err),
            }
        });
        th.join().unwrap()
    }
}
