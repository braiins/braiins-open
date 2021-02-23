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

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{App, Arg};
use futures::TryFutureExt;
use ii_async_utils::HaltHandle;
use ii_logging::macros::*;
use ii_noise_proxy::{NoiseProxy, SecurityContext};
use ii_wire::proxy;

use tokio::prelude::*;

#[derive(serde::Deserialize, Debug)]
struct Configuration {
    listen: String,
    upstream: String,
    certificate: PathBuf,
    server_key: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _grd = ii_logging::setup_for_app(100);
    let matches = App::new("Stratum V1-to-V1 noise proxy")
        .arg(
            Arg::with_name("config")
                .long("conf")
                .short("c")
                .takes_value(true)
                .help("Configuration file path"),
        )
        .get_matches();

    let config_file = matches
        .value_of("config")
        .context("Missing configuration path")?;
    let cfg_string = tokio::fs::File::open(config_file)
        .and_then(|mut file_handle| async move {
            let mut cfg_str = String::new();
            file_handle
                .read_to_string(&mut cfg_str)
                .await
                .map(|_| cfg_str)
        })
        .await?;

    let config = toml::from_str::<Configuration>(&cfg_string)?;

    info!("Running V1 noise proxy: {:#?}", config);

    let cert_path = Path::new(&config.certificate);
    let key_path = Path::new(&config.server_key);
    let ctx = SecurityContext::read_from_file(cert_path, key_path).await?;
    let halt_handle = HaltHandle::arc();
    let noise_proxy = NoiseProxy::new(
        config.listen,
        config.upstream,
        std::sync::Arc::new(ctx),
        proxy::ProtocolConfig::new(
            false,
            vec![proxy::ProtocolVersion::V1, proxy::ProtocolVersion::V2],
        ),
        None,
        None,
    )
    .await?;
    halt_handle.spawn_object(noise_proxy);
    halt_handle.ready();
    halt_handle.clone().halt_on_signal();
    halt_handle
        .join(Some(std::time::Duration::from_secs(3)))
        .await
        .map_err(Into::into)
}
