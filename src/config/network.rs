use serde::{Deserialize, Serialize};

fn def_callback_port() -> u16 {
    // must be between 40000 and 65535 to avoid conflicts with well-known ports
    53400
}

fn def_max_port_retries() -> u16 {
    100
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Network {
    // this is the port on the local machine that receives the oauth callback
    // can be set to whatever as long as its not taken
    #[serde(default = "def_callback_port")]
    pub callback_port: u16,

    // if the port is taken and the binding fails it will retry with the next port:
    // (callback_port + 1). This determines how far that will go
    #[serde(default = "def_max_port_retries")]
    pub max_port_retries: u16,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            callback_port: def_callback_port(),
            max_port_retries: def_max_port_retries(),
        }
    }
}
