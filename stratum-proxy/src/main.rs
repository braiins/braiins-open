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

//! Simple proxy that translates V2 protocol from clients to V1 protocol and connects to a
//! requested pool

use anyhow::{Context, Result};
use structopt::StructOpt;

use ii_async_utils::HaltHandle;
use ii_logging::macros::*;
use ii_scm::global::Version;
use ii_stratum_proxy::{
    frontend::{Args, Config},
    server::{self, controller::LoggingController, ProxyProtocolConfig},
};

#[tokio::main]
async fn main() -> Result<()> {
    Version::set("StratumProxy", ii_scm::version_full!().as_str());
    ii_async_utils::setup_panic_handling();

    let _logging_controller = LoggingController::new(None);

    let args = Args::from_args();

    let config_file_string = tokio::fs::read_to_string(args.config_file)
        .await
        .context("Proxy configuration file couldn't be read.")?;
    let config = toml::from_str::<Config>(config_file_string.as_str())?;
    info!("Starting {}: {}", Version::signature(), Version::full(),);
    info!("Config: {:#?}", config);

    let server = server::ProxyServer::listen(
        config.listen_address.clone(),
        config.upstream_address.clone(),
        server::TranslationHandler::new(None),
        config.read_certificate_secret_key_pair().await?,
        config
            .proxy_protocol_config
            .unwrap_or_else(ProxyProtocolConfig::default),
        None,
    )
    .context("Cannot bind the server")?;

    let halt_handle = HaltHandle::arc();
    halt_handle.spawn_object(server);
    halt_handle.ready();
    halt_handle.halt_on_signal();
    halt_handle
        .join(Some(std::time::Duration::from_secs(5)))
        .await
        .map_err(Into::into)
}
