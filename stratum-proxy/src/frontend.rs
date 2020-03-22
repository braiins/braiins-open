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

use structopt::StructOpt;

use ii_wire::Address;

#[derive(StructOpt, Debug)]
#[structopt(name = "stratum-proxy", about = "Stratum V2->V1 translating proxy.")]
pub struct Args {
    /// Listen address
    #[structopt(
        short = "l",
        long = "listen",
        default_value = "localhost:3336",
        help = "Address to listen on for incoming Stratum V2 connections"
    )]
    pub listen_address: Address,

    /// Remote V1 endpoint where to connect to
    #[structopt(
        short = "u",
        long = "v1-upstream",
        name = "HOSTNAME:PORT",
        help = "Address of the upstream Stratum V1 server that the proxy connects to"
    )]
    pub upstream_address: Address,
}
