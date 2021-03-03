// Copyright (C) 2021  Braiins Systems s.r.o.
//
// This file is part of Braiins Open-Source Initiative (BOSI).
//
// BOSI is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// Please, keep in mind that we may also license BOSI or any part thereof
// under a proprietary license. For more information on the terms and conditions
// of such proprietary license or if you have any other questions, please
// contact us at opensource@braiins.com.

use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use structopt::StructOpt;

use ii_noise_proxy::SecurityContext;
use ii_scm::global::Version;
use ii_wire::Address;

use crate::error::{Error, Result};
use crate::server::ProxyProtocolConfig;

#[derive(Debug, StructOpt)]
#[structopt(name = Version::signature().as_str(), version = Version::full().as_str())]
pub struct Args {
    #[structopt(short = "c", long = "conf", help("Path to configuration file"))]
    pub config_file: PathBuf,
}

// TODO: Write Deserizlize manually in order to report errors and validate config more properly
// if only one of the certificate or server-private-key config line is present, serde treates it as
// if none were present and doesn't return error
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub listen_address: Address,
    pub upstream_address: Address,
    #[serde(default)] // Default for bool is "false"
    pub insecure: bool,
    #[serde(flatten)]
    pub key_and_cert_files: Option<KeyAndCertFiles>,
    pub proxy_protocol_config: Option<ProxyProtocolConfig>,
}

#[derive(Debug, Deserialize)]
pub struct KeyAndCertFiles {
    certificate_file: PathBuf,
    secret_key_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_address: Address("0.0.0.0".to_owned(), 3336),
            upstream_address: Address("stratum.slushpool.com".to_owned(), 3333),
            insecure: true,
            key_and_cert_files: None,
            proxy_protocol_config: None,
        }
    }
}

impl Config {
    /// Read certificates for current configuration and return:
    ///  - `None` if file path configurations are missing and/or `insecure == true` option
    ///  - SecurityContext `Some(SecurityContext)` if files are valid and `insecure == false`
    ///  - `Error` otherwise
    pub async fn read_security_context(&self) -> Result<Option<Arc<SecurityContext>>> {
        if self.insecure {
            Ok(None)
        } else if let Some(key_and_cert_files) = self.key_and_cert_files.as_ref() {
            let ctx_result = SecurityContext::read_from_file(
                key_and_cert_files.certificate_file.as_path(),
                key_and_cert_files.secret_key_file.as_path(),
            )
            .await
            .map_err(|e| Error::InvalidFile(format!("Failed to read certificate and key: {}", e)))
            .map(Arc::new);
            if let Ok(ctx) = ctx_result.as_ref() {
                ctx.validate_by_time(std::time::SystemTime::now)?;
            }
            ctx_result.map(Some)
        } else {
            Err(Error::InvalidFile(
                "Certificate and key files are missing".to_owned(),
            ))
        }
    }
}
