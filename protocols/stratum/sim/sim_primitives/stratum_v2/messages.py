# Copyright (C) 2019  Braiins Systems s.r.o.
#
# This file is part of Braiins Open-Source Initiative (BOSI).
#
# BOSI is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
#
# Please, keep in mind that we may also license BOSI or any part thereof
# under a proprietary license. For more information on the terms and conditions
# of such proprietary license or if you have any other questions, please
# contact us at opensource@braiins.com.

"""Stratum V2 messages."""
from ..protocol import Message
from .types import (
    MiningChannelType,
    DeviceInfo,
    PubKey,
    Signature,
    Hash,
    MerklePath,
    CoinBasePrefix,
    CoinBaseSuffix,
    DownstreamConnectionFlags,
    UpstreamConnectionFlags,
)


class SetupConnection(Message):
    def __init__(
        self,
        max_version: int,
        min_version: int,
        flags: list,
        expected_pubkey: PubKey,
        endpoint_host,
        endpoint_port: int,
        device_info: DeviceInfo,
    ):
        self.max_version = max_version
        self.min_version = min_version
        self.flags = set(flags)
        self.expected_pubkey = expected_pubkey
        self.endpoint_host = endpoint_host
        self.endpoint_port = endpoint_port
        self.device_info = device_info
        super().__init__()


class SetupConnectionSuccess(Message):
    def __init__(self, used_version, flags: list, pubkey: PubKey):
        self.used_version = used_version
        self.flags = set(flags)
        self.pubkey = pubkey
        super().__init__()


class SetupConnectionError(Message):
    def __init__(self, error_code: str):
        self.error_code = error_code
        super().__init__()


class OpenStandardMiningChannel(Message):
    def __init__(self, req_id, user: str, nominal_hashrate, max_target: int):
        """

        :param req_id: request ID for pairing with response
        :param user: user credentials for accounting shares submitted on this channel
        :param device_info: Details about the connected device
        :param nominal_hashrate: Hash rate of the device in h/s (floating point)
        :param max_target: Maximum target that the device is capable of working on
        """
        self.user = user
        self.nominal_hashrate = nominal_hashrate
        self.max_target = max_target
        super().__init__(req_id)


class ChannelMessage(Message):
    """Message specific for a channel identified by its channel_id"""

    def __init__(self, channel_id: int, *args, **kwargs):
        self.channel_id = channel_id
        super().__init__(*args, **kwargs)


class OpenStandardMiningChannelSuccess(ChannelMessage):
    def __init__(self, req_id, channel_id: int, target: int, group_channel_id: int):
        self.init_target = target
        self.group_channel_id = group_channel_id
        super().__init__(channel_id=channel_id, req_id=req_id)


class OpenStandardMiningChannelError(Message):
    def __init__(self, req_id, error_code: str):
        self.req_id = req_id
        self.error_code = error_code
        super().__init__(req_id)


class SubmitShares(ChannelMessage):
    def __init__(
        self,
        channel_id: int,
        seq_num: int,
        job_id: int,
        nonce: int,
        ntime: int,
        version: int,
    ):
        self.seq_num = seq_num
        self.job_id = job_id
        self.nonce = nonce
        self.ntime = ntime
        self.version = version
        super().__init__(channel_id)

    def __str__(self):
        return self._format(
            'channel_id={}, job_id={}'.format(self.channel_id, self.job_id)
        )


class SubmitSharesExtended(SubmitShares):
    def __init__(self, extranonce2, *args, **kwargs):
        self.extranonce2 = extranonce2
        super().__init__(*args, **kwargs)


class SubmitSharesSuccess(ChannelMessage):
    def __init__(
        self,
        channel_id: int,
        last_seq_num: int,
        new_submits_accepted_count: int,
        new_shares_sum: int,
    ):
        self.last_seq_num = last_seq_num
        self.new_submits_accepted_count = new_submits_accepted_count
        self.new_shares_sum = new_shares_sum
        super().__init__(channel_id)

    def __str__(self):
        return self._format(
            'channel_id={}, last_seq_num={}, accepted_submits={}, accepted_shares={}'.format(
                self.channel_id,
                self.last_seq_num,
                self.new_submits_accepted_count,
                self.new_shares_sum,
            )
        )


class SubmitSharesError(ChannelMessage):
    def __init__(self, channel_id: int, seq_num: int, error_code: str):
        self.seq_num = seq_num
        self.error_code = error_code
        super().__init__(channel_id)


class NewMiningJob(ChannelMessage):
    def __init__(
        self,
        channel_id: int,
        job_id: int,
        future_job: bool,
        merkle_root: Hash,
        version: int,
    ):
        self.job_id = job_id
        self.future_job = future_job
        self.merkle_root = merkle_root
        self.version = version
        super().__init__(channel_id)

    def __str__(self):
        return self._format(
            'channel_id={}, job_id={}, future_job={}'.format(
                self.channel_id, self.job_id, self.future_job
            )
        )


class NewExtendedMiningJob(Message):
    def __init__(
        self,
        channel_id,
        job_id,
        future_job: bool,
        merkle_path: MerklePath,
        custom_id,
        cb_prefix: CoinBasePrefix,
        cb_suffix: CoinBaseSuffix,
    ):
        self.job_id = job_id
        self.future_job = future_job
        self.merkle_path = merkle_path
        self.custom_id = custom_id
        self.cb_prefix = cb_prefix
        self.cb_suffix = cb_suffix
        # TODO-DOC: version not in spec?
        # self.version = version
        super().__init__(channel_id)


class SetNewPrevHash(ChannelMessage):
    def __init__(
        self,
        channel_id,
        prev_hash,
        min_ntime,
        max_ntime_offset,
        nbits,
        job_id,
        signature: Signature,
    ):
        self.prev_hash = prev_hash
        self.min_ntime = min_ntime
        self.max_ntime_offset = max_ntime_offset
        self.nbits = nbits
        self.job_id = job_id
        self.signature = signature
        super().__init__(channel_id)

    def __str__(self):
        return self._format(
            'channel_id={}, job_id={}'.format(self.channel_id, self.job_id)
        )


class SetCustomMiningJob(ChannelMessage):
    pass


class SetCustomMiningJobSuccess(ChannelMessage):
    pass


class Reconnect(Message):
    def __init__(self, new_url, signature: Signature):
        self.new_url = new_url
        self.signature = signature
        super().__init__()


class SetTarget(ChannelMessage):
    def __init__(self, channel_id, max_target):
        self.max_target = max_target
        super().__init__(channel_id)
