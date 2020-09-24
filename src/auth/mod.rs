use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrivateClaims {
    pub roles: Vec<String>,
}

impl PrivateClaims {
    pub fn roles(&self) -> Vec<String> {
        self.roles.to_owned()
    }
}
