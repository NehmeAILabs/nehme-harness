use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AcpServerConfig {
    #[serde(rename = "tcp")]
    Tcp {
        host: String,
        port: u16,
        #[serde(default)]
        api_key: Option<String>,
    },
    #[serde(rename = "stdio")]
    Stdio,
}

impl AcpServerConfig {
    #[allow(dead_code)]
    pub fn transport_type(&self) -> &str {
        match self {
            AcpServerConfig::Tcp { .. } => "tcp",
            AcpServerConfig::Stdio => "stdio",
        }
    }
}
