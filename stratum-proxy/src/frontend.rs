// Copyright (C) 2020  Braiins Systems s.r.o.
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
use std::convert::TryFrom;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::{fs::File, io::AsyncReadExt};

use ii_stratum::v2::noise::auth::{Certificate, StaticSecretKeyFormat};
use ii_wire::Address;

use crate::error::{Error, Result};
use crate::server::ProxyProtocolConfig;

#[derive(Debug, StructOpt)]
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
    pub security_context: Option<SecurityContext>,
    pub proxy_protocol_config: Option<ProxyProtocolConfig>,
}

#[derive(Debug, Deserialize)]
pub struct SecurityContext {
    certificate_file: PathBuf,
    secret_key_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_address: Address("0.0.0.0".to_owned(), 3336),
            upstream_address: Address("stratum.slushpool.com".to_owned(), 3333),
            insecure: true,
            security_context: None,
            proxy_protocol_config: None,
        }
    }
}

impl Config {
    /// Read certificates for current configuation and return:
    ///  - `None` if file path configurations are missing and/or `insecure == true` option
    ///  - pair `Some((Certificate, StaticSecretKeyFormat))` if files are valid and `insecure == false`
    ///  - `Error` otherwise
    pub async fn read_certificate_secret_key_pair(
        &self,
    ) -> Result<Option<(Certificate, StaticSecretKeyFormat)>> {
        if self.insecure {
            Ok(None)
        } else {
            if let Some(ctx) = self.security_context.as_ref() {
                Ok(Some(ctx.read_from_file().await?))
            } else {
                Err(Error::InvalidFile(
                    "Certificate and key files are missing".to_owned(),
                ))
            }
        }
    }
}

impl SecurityContext {
    /// Reads certificate and secret key files
    /// return Error if files cannot be read:
    async fn read_from_file(&self) -> Result<(Certificate, StaticSecretKeyFormat)> {
        let mut cert_file = File::open(self.certificate_file.as_path())
            .await
            .map_err(|e| {
                Error::InvalidFile(format!(
                    "{}: {}",
                    e,
                    self.certificate_file.to_string_lossy()
                ))
            })?;
        let mut key_file = File::open(self.secret_key_file.as_path())
            .await
            .map_err(|e| {
                Error::InvalidFile(format!("{}: {}", e, self.secret_key_file.to_string_lossy()))
            })?;

        let mut cert_string = String::new();
        cert_file
            .read_to_string(&mut cert_string)
            .await
            .map_err(|e| {
                Error::InvalidFile(format!("Error: {} in file {:?}", e, self.certificate_file))
            })?;
        let mut key_string = String::new();
        key_file
            .read_to_string(&mut key_string)
            .await
            .map_err(|e| {
                Error::InvalidFile(format!("Error: {} in file {:?}", e, self.secret_key_file))
            })?;

        let cert = Certificate::try_from(cert_string).map_err(|e| {
            Error::InvalidFile(format!("Error: {} in file {:?}", e, self.certificate_file))
        })?;
        let key = StaticSecretKeyFormat::try_from(key_string).map_err(|e| {
            Error::InvalidFile(format!("Error: {} in file {:?}", e, self.secret_key_file))
        })?;
        Ok((cert, key))
    }
}
