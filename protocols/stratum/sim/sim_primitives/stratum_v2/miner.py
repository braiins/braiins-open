"""V2 header only miner"""

from ..miner import Miner
from ..protocol import DownstreamConnectionProcessor
from .messages import (
    SetupConnection,
    SetupConnectionSuccess,
    SetupConnectionError,
    OpenMiningChannel,
    OpenMiningChannelSuccess,
    OpenMiningChannelError,
    SetNewPrevHash,
    SetTarget,
    NewMiningJob,
    SubmitShares,
    SubmitSharesSuccess,
    SubmitSharesError,
)
from ..network import Connection
from .types import *
import sim_primitives.coins as coins

# TODO: Move MiningChannel and session from Pool
from .pool import PoolMiningChannel
from ..pool import MiningJob


class MinerV2(DownstreamConnectionProcessor):
    class ConnectionConfig:
        """Stratum V2 connection configuration.

        For now, it is sufficient to record the SetupConnectionSuccess to have full
        connection configuration available.
        """

        def __init__(self, msg: SetupConnectionSuccess):
            self.setup_msg = msg

    class States(enum.Enum):
        INIT = 0
        CONNECTION_SETUP = 1

    def __init__(self, miner: Miner, connection: Connection):
        self.miner = miner
        self.state = self.States.INIT
        self.channel = None
        super().__init__(miner.name, miner.env, miner.bus, connection)
        # Initiate V2 protocol setup
        # TODO-DOC: specification should categorize downstream and upstream flags.
        #  PubKey handling is also not precisely defined yet
        self._send_msg(
            SetupConnection(
                max_version=1,
                min_version=1,
                flags=[DownstreamConnectionFlags.NONE],
                endpoint_hostname='v2pool',
                endpoint_port=connection.port,
                expected_pubkey=PubKey(),
            )
        )
        self.connection_config = None

    def visit_setup_connection_success(self, msg: SetupConnectionSuccess):
        self._emit_protocol_msg_on_bus('Connection setup', msg)
        self.connection_config = self.ConnectionConfig(msg)
        self.state = self.States.CONNECTION_SETUP

        req = OpenMiningChannel(
            req_id=None,
            user=self.name,
            channel_type=MiningChannelType.STANDARD,
            device_info=DeviceInfo(),
            nominal_hashrate=self.miner.speed_ghps * 1e9,
            max_target=self.miner.diff_1_target,
            # Header only mining, now extranonce 2 size required
            min_extranonce_size=0,
            agg_device_count=1,
        )
        # We expect a paired response to our open channel request
        self.request_registry.push(req)
        self._send_msg(req)

    def visit_setup_connection_error(self, msg: SetupConnectionError):
        """Setup connection has failed.

        TODO: consider implementing reconnection attempt with exponential backoff or
         something similar
        """
        self._emit_protocol_msg_on_bus('Connection setup failed', msg)

    def visit_open_mining_channel_success(self, msg: OpenMiningChannelSuccess):
        req = self.request_registry.pop(msg.req_id)

        if req is not None:
            session = self.miner.new_mining_session(
                coins.Target(msg.init_target, self.miner.diff_1_target)
            )
            # TODO find some reasonable extraction of the channel configuration, for now,
            #  we just retain the OpenMiningChannel and OpenMiningChannelSuccess message
            #  pair that provides complete information
            self.channel = PoolMiningChannel(
                session=session,
                cfg=(req, msg),
                conn_uid=self.connection.uid,
                channel_id=msg.channel_id,
            )
            session.run()
        else:
            self._emit_protocol_msg_on_bus(
                'Cannot find matching OpenMiningChannel request', msg
            )

    def visit_open_mining_channel_error(self, msg: OpenMiningChannelError):
        req = self.request_registry.pop(msg.req_id)
        self._emit_protocol_msg_on_bus(
            'Open mining channel failed (orig request: {})'.format(req), msg
        )

    def visit_set_target(self, msg: SetTarget):
        if self.__is_channel_valid(msg):
            self.channel.session.set_target(msg.max_target)

    def visit_set_new_prev_hash(self, msg: SetNewPrevHash):
        if self.__is_channel_valid(msg):
            if self.channel.session.job_registry.contains(msg.job_id):
                self.miner.mine_on_new_job(
                    job=self.channel.session.job_registry.get_job(msg.job_id),
                    flush_any_pending_work=True,
                )

    def visit_new_mining_job(self, msg: NewMiningJob):
        if self.__is_channel_valid(msg):
            # Prepare a new job with the current session difficulty target
            job = self.channel.session.new_mining_job(job_uid=msg.job_id)
            # Schedule the job for mining
            if not msg.future_job:
                self.miner.mine_on_new_job(job)

    def visit_submit_shares_success(self, msg: SubmitSharesSuccess):
        if self.__is_channel_valid(msg):
            self.channel.session.account_diff_shares(msg.new_shares_sum)

    def visit_submit_shares_error(self, msg: SubmitSharesError):
        if self.__is_channel_valid(msg):
            # TODO implement accounting for rejected shares
            pass
            # self.channel.session.account_rejected_shares(msg.new_shares_sum)

    def submit_mining_solution(self, job: MiningJob):
        """Callback from the physical miner that succesfully simulated mining some shares

        :param job: Job that the miner has been working on and found solution for it
        """
        # TODO: seq_num is currently unused, we should use it for tracking
        #  accepted/rejected shares
        self._send_msg(
            SubmitShares(
                channel_id=self.channel.id,
                seq_num=0,
                job_id=job.uid,
                nonce=None,
                ntime=None,
                version=None,
            )
        )

    def _on_invalid_message(self, msg):
        self._emit_protocol_msg_on_bus('Received invalid message', msg)

    def __is_channel_valid(self, msg):
        """Validates channel referenced in the message is the open channel of the miner"""
        if self.channel is None:
            bus_error_msg = (
                'Mining Channel not established yet, received channel '
                'message with channel ID({})'.format(msg.channel_id)
            )
            is_valid = False
            self._emit_protocol_msg_on_bus(bus_error_msg, msg)
        elif self.channel.id != msg.channel_id:
            bus_error_msg = 'Unknown channel (expected: {}, actual: {})'.format(
                self.channel.channel_id, msg.channel_id
            )
            is_valid = False
            self._emit_protocol_msg_on_bus(bus_error_msg, msg)
        else:
            is_valid = True

        return is_valid

    def run(self):
        pass
