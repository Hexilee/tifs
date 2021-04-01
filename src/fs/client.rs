use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tikv_client::Config;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct TlsConfig {
    pub ca_path: PathBuf,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl TlsConfig {
    fn exist_all(&self) -> bool {
        self.ca_path.exists() && self.cert_path.exists() && self.key_path.exists()
    }
}

impl From<TlsConfig> for Config {
    fn from(tls_cfg: TlsConfig) -> Self {
        let cfg = Config::default();
        if !tls_cfg.exist_all() {
            cfg
        } else {
            cfg.with_security(tls_cfg.ca_path, tls_cfg.cert_path, tls_cfg.key_path)
        }
    }
}
