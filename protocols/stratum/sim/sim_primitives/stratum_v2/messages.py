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
        endpoint_hostname,
        endpoint_port: int,
    ):
        self.max_version = max_version
        self.min_version = min_version
        self.flags = set(flags)
        self.expected_pubkey = expected_pubkey
        self.endpoint_hostname = endpoint_hostname
        self.endpoint_port = endpoint_port
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


class OpenMiningChannel(Message):
    def __init__(
        self,
        req_id,
        user: str,
        channel_type: MiningChannelType,
        device_info: DeviceInfo,
        nominal_hashrate,
        max_target: int,
        min_extranonce_size: int,
        agg_device_count: int,
    ):
        """

        :param req_id: request ID for pairing with response
        :param user: user credentials for accounting shares submitted on this channel
        :param channel_type: ChannelType
        :param device_info: Details about the connected device
        :param nominal_hashrate: Hash rate of the device in h/s (floating point)
        :param max_target: Maximum target that the device is capable of working on
        :param min_extranonce_size: Minimum extra nonce 2 size
        :param agg_device_count: aggregated downstream device count (e.g. the device is an aggregating proxy)
        """
        self.user = user
        self.channel_type = channel_type
        self.device_info = device_info
        self.nominal_hashrate = nominal_hashrate
        self.max_target = max_target
        self.min_extranonce_size = min_extranonce_size
        self.agg_device_count = agg_device_count
        super().__init__(req_id)


class ChannelMessage(Message):
    """Message specific for a channel identified by its channel_id"""

    def __init__(self, channel_id: int, *args, **kwargs):
        self.channel_id = channel_id
        super().__init__(*args, **kwargs)


class OpenMiningChannelSuccess(ChannelMessage):
    def __init__(
        self,
        req_id,
        channel_id: int,
        init_target: int,
        group_channel_id: int,
        extranonce2_size: int,
    ):
        self.init_target = init_target
        self.group_channel_id = group_channel_id
        self.extranonce2_size = extranonce2_size
        super().__init__(channel_id=channel_id, req_id=req_id)


class OpenMiningChannelError(Message):
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
