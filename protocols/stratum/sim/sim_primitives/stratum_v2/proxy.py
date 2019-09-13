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

"""Stratum V2->V1 Proxy

"""
import enum

import sim_primitives.stratum_v1.messages as v1_messages

from sim_primitives.network import Connection
from sim_primitives.pool import MiningSession
from sim_primitives.protocol import (
    Message,
    UpstreamConnectionProcessor,
    DownstreamConnectionProcessor,
)
from sim_primitives.proxy import Proxy
from sim_primitives.stratum_v2.messages import *
from sim_primitives.stratum_v2.pool import ChannelRegistry
from sim_primitives.stratum_v2.types import (
    DownstreamConnectionFlags,
    UpstreamConnectionFlags,
)


class V1Client(DownstreamConnectionProcessor):
    def __init__(self, translation, connection: Connection, msg_handler_map):
        self.msg_handler_map = msg_handler_map
        super().__init__(translation.name, translation.env, translation.bus, connection)

    def subscribe_and_authorize(self):

        auth_req = v1_messages.Authorize(
            req_id=None, user_name='some_miner', password='x'
        )
        self.send_request(auth_req)

        sbscr_req = v1_messages.Subscribe(
            req_id=None, signature='some_signature', extranonce1=None, url='some_url'
        )
        self.send_request(sbscr_req)

    def visit_ok_result(self, msg):
        req = self.request_registry.pop(msg.req_id)
        if req:
            self.msg_handler_map[type(req)](msg)

    def visit_error_result(self, msg):
        req = self.request_registry.pop(msg.req_id)
        if req:
            self.msg_handler_map[type(req)](msg)
            self._emit_protocol_msg_on_bus(
                "Error code {}, '{}' for request".format(msg.code, msg.msg), req
            )

    def visit_subscribe_response(self, msg):
        req = self.request_registry.pop(msg.req_id)
        if req:
            self.msg_handler_map[type(msg)](msg)

    def visit_configure_response(self, msg):
        req = self.request_registry.pop(msg.req_id)
        if req:
            self.msg_handler_map[type(msg)](msg)

    def visit_set_difficulty(self, msg):
        self.msg_handler_map[type(msg)](msg)

    def visit_notify(self, msg):
        self.msg_handler_map[type(msg)](msg)

    def _on_invalid_message(self, msg):
        self._emit_protocol_msg_on_bus('Received invalid message', msg)


class V2ToV1Translation(UpstreamConnectionProcessor):
    """Processes all messages on 1 connection

    """

    class State(enum.Enum):
        # No message received yet
        INIT = enum.auto()
        # Stratum V1 mining.configure is in progress
        V1_CONFIGURE = enum.auto()
        # Connection successfully setup, waiting for OpenMiningChannel message
        CONNECTION_SETUP = enum.auto()
        # Channel now needs finalization of subscribe+authorize+set difficulty
        # target with the upstream V1 server
        OPEN_MINING_CHANNEL_PENDING = enum.auto()
        # Upstream subscribe/authorize failed state ensures sending
        # OpenMiningChannelError only once
        V1_SUBSCRIBE_OR_AUTHORIZE_FAIL = enum.auto()
        # Channel is operational
        OPERATIONAL = enum.auto()

    def __init__(self, proxy: Proxy, connection):
        self.proxy = proxy
        self.connection_config = None
        self.state = self.State.INIT
        self._mining_channel_registry = ChannelRegistry(connection.uid)

        self.user_identity = None

        self.v2_config = None
        self.v2_mining_channel_params = dict()

        self.v1_client = None
        self.v1_authorized = False
        self.v1_result_handler_map = {
            v1_messages.Authorize: self.handle_authorize_response,
            v1_messages.SubscribeResponse: self.handle_subscribe_response,
            v1_messages.ConfigureResponse: self.handle_configure_response,
            v1_messages.SetDifficulty: self.handle_set_difficulty,
            v1_messages.Notify: self.handle_notify,
            v1_messages.ErrorResult: self.handle_error_result_response,
            v1_messages.Submit: self.handle_submit_response,
        }
        super().__init__(proxy.name, proxy.env, proxy.bus, connection)

    def handle_authorize_response(self, msg: Message):
        self.v1_authorized = True

        if (
            self.v2_mining_channel_params.get('extranonce_prefix')
            and self.state == self.State.OPEN_MINING_CHANNEL_PENDING
        ):
            self.state = self.State.OPERATIONAL
            self._send_open_mining_channel(success=True)

    def handle_subscribe_response(self, msg: Message):
        self.v2_mining_channel_params['extranonce_prefix'] = msg.extranonce1
        if self.v1_authorized and self.state == self.State.OPEN_MINING_CHANNEL_PENDING:
            self.state = self.State.OPERATIONAL
            self._send_open_mining_channel(success=True)

    def handle_configure_response(self, msg: Message):
        if self.state == self.State.V1_CONFIGURE:
            self.state = self.State.CONNECTION_SETUP
            self._send_msg(self.v2_config)

    def handle_error_result_response(self, msg: Message):
        self.state = self.State.V1_SUBSCRIBE_OR_AUTHORIZE_FAIL

    def handle_submit_response(self, msg: Message):
        if isinstance(msg, v1_messages.OkResult):
            self._send_msg(
                SubmitSharesSuccess(
                    channel_id=self.v2_mining_channel_params.get('channel_id'),
                    last_sequence_number=self.v2_mining_channel_params.get('seq_num'),
                    new_submits_accepted_count=1,
                    new_shares_sum=self.v2_mining_channel_params.get(
                        'target'
                    ).to_difficulty(),
                )
            )
        elif isinstance(msg, v1_messages.ErrorResult):
            self._send_msg(
                SubmitSharesError(
                    channel_id=self.v2_mining_channel_params.get('channel_id'),
                    sequence_number=self.v2_mining_channel_params.get(
                        'sequence_number'
                    ),
                    error_code='Share rejected',
                )
            )

    def handle_set_difficulty(self, msg: Message):
        self.v2_mining_channel_params['target'] = msg.diff
        self._send_msg(
            SetTarget(
                channel_id=self.v2_mining_channel_params.get('channel_id'),
                max_target=msg.diff,
            )
        )

    def handle_notify(self, msg: Message):
        v2_new_prev_hash = SetNewPrevHash(
            channel_id=self.v2_mining_channel_params.get('channel_id'),
            job_id=msg.job_id,
            prev_hash=msg.prev_hash,
            min_ntime=msg.time,
            nbits=msg.bits,
        )
        self._send_msg(v2_new_prev_hash)

        v2_new_job = NewMiningJob(
            channel_id=self.v2_mining_channel_params.get('channel_id'),
            job_id=msg.job_id,
            future_job=False,
            merkle_root=msg.merkle_branch[0] if msg.merkle_branch else Hash(),
            version=0,
        )
        self._send_msg(v2_new_job)

    def visit_setup_connection(self, msg: SetupConnection):
        if self.state in (self.State.INIT,):
            # arbitrary for now
            response_flags = set()
            if DownstreamConnectionFlags.REQUIRES_VERSION_ROLLING not in msg.flags:
                response_flags.add(UpstreamConnectionFlags.REQUIRES_FIXED_VERSION)

            self.v2_config = SetupConnectionSuccess(
                used_version=min(msg.min_version, msg.max_version), flags=set()
            )

            # TODO fill out actual extension parameters
            configure_msg = v1_messages.Configure(
                None, extensions=['dummy'], extension_params={'dummy': None}
            )
            # Establish a new upstream connection and run the client that would \
            # process the messages
            conn = self.proxy.upstream_connection_factory.create_connection()
            conn.connect_to(self.proxy.upstream_node)

            self.v1_client = V1Client(self, conn, self.v1_result_handler_map)
            self.v1_client.send_request(configure_msg)
            self.state = self.State.V1_CONFIGURE
        else:
            self._send_msg(SetupConnectionError('Connection can only be setup once'))

    def visit_open_standard_mining_channel(self, msg: OpenStandardMiningChannel):
        import random

        self.state = self.State.OPEN_MINING_CHANNEL_PENDING
        self.v2_mining_channel_params.update(
            req_id=msg.req_id,
            channel_id=random.randrange(2 ** 32),
            group_channel_id=0,
            user_identity=msg.user_identity,
        )
        self.v1_client.subscribe_and_authorize()

    def visit_open_extended_mining_channel(self, msg: OpenExtendedMiningChannel):
        pass

    def visit_submit_shares_standard(self, msg: SubmitSharesStandard):
        """
        TODO: implement aggregation of sending SubmitSharesSuccess for a batch of successful submits
        """
        assert msg.channel_id == self.v2_mining_channel_params.get('channel_id')
        # msg.version
        self.v2_mining_channel_params['sequence_number'] = msg.sequence_number
        self.v1_client.send_request(
            v1_messages.Submit(
                req_id=None,
                user_name=self.v2_mining_channel_params.get('user_identity'),  # TODO
                job_id=msg.job_id,
                extranonce2=None,
                time=msg.ntime,
                nonce=msg.nonce,
            )
        )
        self.__emit_channel_msg_on_bus(msg)

    def __emit_channel_msg_on_bus(self, msg: ChannelMessage):
        """Helper method for reporting a channel oriented message on the debugging bus."""
        self._emit_protocol_msg_on_bus('Channel ID: {}'.format(msg.channel_id), msg)

    def terminate(self):
        super().terminate()
        for channel in self._mining_channel_registry.channels:
            channel.terminate()

    def _on_invalid_message(self, msg):
        """Ignore any unrecognized messages.
        TODO-DOC: define protocol handling of unrecognized messages
        """
        pass

    def _send_open_mining_channel(self, success: bool):
        v2_mining_channel = (
            OpenStandardMiningChannelSuccess(
                req_id=self.v2_mining_channel_params.get('req_id'),
                channel_id=self.v2_mining_channel_params.get('channel_id'),
                target=self.v2_mining_channel_params.get('target'),
                extranonce_prefix=self.v2_mining_channel_params.get(
                    'extranonce_prefix'
                ),
                group_channel_id=self.v2_mining_channel_params.get('group_channel_id'),
            )
            if success
            else OpenMiningChannelError(
                req_id=self.v2_mining_channel_params.get('req_id'),
                error_code=self.v2_mining_channel_params.get('error_code'),
            )
        )
        self._send_msg(v2_mining_channel)
        self.__emit_channel_msg_on_bus(v2_mining_channel)

    # def _on_vardiff_change(self, session: MiningSession):
    #     """Handle difficulty change for the current session.

    #     Note that to enforce difficulty change as soon as possible,
    #     the message is accompanied by generating new mining job
    #     """
    #     channel = session.owner
    #     self._send_msg(SetTarget(channel.id, session.curr_target))

    #     new_job_msg = self.__build_new_job_msg(channel, is_future_job=False)
    #     self._send_msg(new_job_msg)

    # def on_new_block(self):
    #     """Sends an individual SetNewPrevHash message to all channels
    #
    #     TODO: it is not quite clear how to handle the case where downstream has
    #      open multiple channels with the pool. The following needs to be
    #      answered:
    #      - Is any downstream node that opens more than 1 mining channel considered a
    #        proxy = it understands  grouping? MAYBE/YES but see next questions
    #      - Can we send only 1 SetNewPrevHash message even if the channels are
    #        standard? NO - see below
    #      - if only 1 group SetNewPrevHash message is sent what 'future' job should
    #        it reference? The problem is that we have no defined way where a future
    #        job is being shared by multiple channels.
    #     """
    #     # Pool currently doesn't support grouping channels, all channels belong to
    #     # group 0. We set the prev hash for all channels at once
    #     # Retire current jobs in the registries of all channels
    #     for channel in self._mining_channel_registry.channels:
    #         future_job = channel.take_future_job()
    #         prev_hash_msg = self.__build_set_new_prev_hash_msg(
    #             channel.id, future_job.uid
    #         )
    #         channel.session.job_registry.retire_all_jobs()
    #         channel.session.job_registry.add_job(future_job)
    #         # Now, we can send out the new prev hash, since all jobs are
    #         # invalidated. Any further submits for the invalidated jobs will be
    #         # rejected
    #         self._send_msg(prev_hash_msg)
    #
    #     # We can now broadcast future jobs to all channels for the upcoming block
    #     for channel in self._mining_channel_registry.channels:
    #         future_new_job_msg = self.__build_new_job_msg(channel, is_future_job=True)
    #         self._send_msg(future_new_job_msg)

    # def __build_set_new_prev_hash_msg(self, channel_id, future_job_id):
    #     return SetNewPrevHash(
    #         channel_id=channel_id,
    #         prev_hash=self..prev_hash,
    #         min_ntime=self.env.now,
    #         nbits=None,
    #         job_id=future_job_id,
    #     )

    # @staticmethod
    # def __build_new_job_msg(mining_channel: PoolMiningChannel, is_future_job: bool):
    #     """Builds NewMiningJob or NewExtendedMiningJob depending on channel type.
    #
    #     The method also builds the actual job and registers it as 'future' job within
    #     the channel if requested.
    #
    #     :param channel: determines the channel and thus message type
    #     :param is_future_job: when true, the job won't be considered for the current prev
    #      hash known to the downstream node but for any future prev hash that explicitly
    #      selects it
    #     :return New{Extended}MiningJob
    #     """
    #     new_job = mining_channel.session.new_mining_job()
    #     if is_future_job:
    #         mining_channel.add_future_job(new_job)
    #
    #     # Compose the protocol message based on actual channel type
    #     if mining_channel.cfg.channel_type == MiningChannelType.STANDARD:
    #         msg = NewMiningJob(
    #             channel_id=mining_channel.id,
    #             job_id=new_job.uid,
    #             future_job=is_future_job,
    #             merkle_root=Hash(),
    #             version=None,
    #         )
    #     elif mining_channel.cfg.channel_type == MiningChannelType.EXTENDED:
    #         msg = NewExtendedMiningJob(
    #             channel_id=mining_channel.id,
    #             job_id=new_job.uid,
    #             future_job=is_future_job,
    #             merkle_path=MerklePath(),
    #             custom_id=None,
    #             cb_prefix=CoinBasePrefix(),
    #             cb_suffix=CoinBaseSuffix(),
    #         )
    #     else:
    #         assert False, 'Unsupported channel type: {}'.format(
    #             mining_channel.cfg.channel_type
    #         )
    #
    #     return msg
