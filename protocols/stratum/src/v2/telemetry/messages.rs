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

use async_trait::async_trait;
use packed_struct_codegen::PrimitiveEnum_u8;
use serde;
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "v2json"))]
use crate::v2::serialization;
use crate::{
    error::{Error, Result},
    v2::{extensions, framing, types::*, Protocol},
    AnyPayload,
};

/// Generates conversion for telemetry protocol messages (extension 1)
macro_rules! impl_telemetry_message_conversion {
    ($message:tt, $is_channel_msg:expr, $handler_fn:tt) => {
        impl_message_conversion!(
            extensions::TELEMETRY,
            $message,
            $is_channel_msg,
            $handler_fn
        );
    };
}

/// All message recognized by the protocol
#[derive(PrimitiveEnum_u8, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageType {
    OpenTelemetryChannel = 0x00,
    OpenTelemetryChannelSuccess = 0x01,
    OpenTelemetryChannelError = 0x02,
    SubmitTelemetryData = 0x03,
    SubmitTelemetryDataSuccess = 0x04,
    SubmitTelemetryDataError = 0x05,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenTelemetryChannel {
    pub req_id: u32,
    pub dev_id: Str0_255,
    // TODO: consider adding this vendor field that would allow verifying that the device and
    //  upstream node accepting the telemetry data will exchange compatible telemetry data
    // pub telemetry_type: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenTelemetryChannelSuccess {
    pub req_id: u32,
    pub channel_id: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenTelemetryChannelError {
    pub req_id: u32,
    pub code: Str0_32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitTelemetryData {
    pub channel_id: u32,
    pub seq_num: u32,
    pub telemetry_payload: Bytes0_64k,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitTelemetryDataSuccess {
    pub channel_id: u32,
    pub last_seq_num: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SubmitTelemetryDataError {
    pub channel_id: u32,
    pub seq_num: u32,
    pub code: Str0_32,
}

impl_telemetry_message_conversion!(OpenTelemetryChannel, false, visit_open_telemetry_channel);
impl_telemetry_message_conversion!(
    OpenTelemetryChannelSuccess,
    false,
    visit_open_telemetry_channel_success
);
impl_telemetry_message_conversion!(
    OpenTelemetryChannelError,
    false,
    visit_open_telemetry_channel_error
);
impl_telemetry_message_conversion!(SubmitTelemetryData, false, visit_submit_telemetry_data);
impl_telemetry_message_conversion!(
    SubmitTelemetryDataSuccess,
    false,
    visit_submit_telemetry_data_success
);
impl_telemetry_message_conversion!(
    SubmitTelemetryDataError,
    false,
    visit_submit_telemetry_data_error
);
