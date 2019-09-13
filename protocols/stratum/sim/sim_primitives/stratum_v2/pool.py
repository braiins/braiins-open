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

"""Stratum V2 pool implementation

"""
from sim_primitives.pool import MiningSession, Pool
from .messages import *
from ..protocol import UpstreamConnectionProcessor
from .types import PubKey, Signature, DownstreamConnectionFlags
import sim_primitives.coins as coins


class MiningChannel:
    def __init__(self, cfg, conn_uid, channel_id):
        """
        :param cfg: configuration is represented by the full OpenMiningChannel or
        OpenMiningChannelSuccess message depending on which end of the channel we are on
        :param conn_uid: backlink to the connection this channel is on
        :param channel_id: unique identifier for the channel
        """
        self.cfg = cfg
        self.conn_uid = conn_uid
        self.id = channel_id

    def set_id(self, channel_id):
        self.id = channel_id


class PoolMiningChannel(MiningChannel):
    """This mining channel contains mining session and future job.

    Currently, the channel holds only 1 future job.
    """

    def __init__(self, session, *args, **kwargs):
        """
        :param session: optional mining session process (TODO: review if this is the right place)
        """
        self.future_job = None
        self.session = session
        super().__init__(*args, **kwargs)

    def terminate(self):
        self.session.terminate()

    def set_session(self, session):
        self.session = session

    def take_future_job(self):
        """Takes future job from the channel."""
        assert (
            self.future_job is not None
        ), 'BUG: Attempt to take a future job from channel: {}'.format(self.id)
        future_job = self.future_job
        self.future_job = None
        return future_job

    def add_future_job(self, job):
        """Stores future job ready for mining should a new block be found"""
        assert (
            self.future_job is None
        ), 'BUG: Attempt to overwrite an existing future job: {}'.format(self.id)
        self.future_job = job


class ChannelRegistry:
    """Keeps track of channels on individual connection"""

    def __init__(self, conn_uid):
        self.conn_uid = conn_uid
        self.channels = []

    def append(self, channel):
        """Simplify registering new channels"""
        new_channel_id = len(self.channels)
        channel.set_id(new_channel_id)
        self.channels.append(channel)

    def get_channel(self, channel_id):
        if channel_id < len(self.channels):
            return self.channels[channel_id]
        else:
            return None


class ConnectionConfig:
    """Stratum V2 connection configuration.

    For now, it is sufficient to record the SetupConnection to have full connection configuration available.
    """

    def __init__(self, msg: SetupConnection):
        self.setup_msg = msg


class PoolV2(UpstreamConnectionProcessor):
    """Processes all messages on 1 connection

    """

    def __init__(self, pool: Pool, connection):
        self.pool = pool
        self.connection_config = None
        self._mining_channel_registry = ChannelRegistry(connection.uid)
        super().__init__(pool.name, pool.env, pool.bus, connection)

    def terminate(self):
        super().terminate()
        for channel in self._mining_channel_registry.channels:
            channel.terminate()

    def _on_invalid_message(self, msg):
        """Ignore any unrecognized messages.

        TODO-DOC: define protocol handling of unrecognized messages
        """
        pass

    def visit_setup_connection(self, msg: SetupConnection):
        if self.connection_config is None:
            self.connection_config = ConnectionConfig(msg)
            # TODO: implement version and flag handling
            self._send_msg(
                SetupConnectionSuccess(
                    used_version=min(msg.min_version, msg.max_version),
                    flags=[DownstreamConnectionFlags.SUPPORTS_EXTENDED_CHANNELS],
                    pubkey=PubKey(),
                )
            )
        else:
            self._send_msg(SetupConnectionError('Connection can only be setup once'))

    def visit_open_mining_channel(self, msg: OpenMiningChannel):
        # Open only channels compatible with this node's configuration
        if (msg.max_target <= self.pool.default_target.diff_1_target) and (
            msg.min_extranonce_size <= self.pool.extranonce2_size
        ):
            # Create the channel and build back-links from session to channel and from
            # channel to connection
            mining_channel = PoolMiningChannel(
                cfg=msg, conn_uid=self.connection.uid, channel_id=None, session=None
            )
            # Appending assigns the channel a unique ID within this connection
            self._mining_channel_registry.append(mining_channel)

            # TODO use partial to bind the mining channel to the _on_vardiff_change and eliminate the need for the
            #  backlink
            session = self.pool.new_mining_session(
                owner=mining_channel, on_vardiff_change=self._on_vardiff_change
            )
            mining_channel.set_session(session)

            self._send_msg(
                OpenMiningChannelSuccess(
                    req_id=msg.req_id,
                    channel_id=mining_channel.id,
                    target=session.curr_target.target,
                    group_channel_id=0,
                )
            )

            # TODO-DOC: explain the (mandatory?) setting 'future_job=True' in
            #  the message since the downstream has no prev hash
            #  immediately after the OpenMiningChannelSuccess
            #  Update the flow diagram in the spec including specifying the
            #  future_job attribute
            new_job_msg = self.__build_new_job_msg(mining_channel, is_future_job=True)
            # Take the future job from the channel so that we have space for producing a new one right away
            future_job = mining_channel.take_future_job()
            assert (
                future_job.uid == new_job_msg.job_id
            ), "BUG: future job on channel {} doesn't match the produced message job ID {}".format(
                future_job.uid, new_job_msg.job_id
            )
            self._send_msg(new_job_msg)
            self._send_msg(
                self.__build_set_new_prev_hash_msg(
                    channel_id=mining_channel.id, future_job_id=new_job_msg.job_id
                )
            )
            # Send out another future job right away
            future_job_msg = self.__build_new_job_msg(
                mining_channel, is_future_job=True
            )
            self._send_msg(future_job_msg)

            # All messages sent, start the session
            session.run()

        else:
            self._send_msg(
                OpenMiningChannelError(
                    msg.req_id, 'Cannot open mining channel: {}'.format(msg)
                )
            )

    def visit_submit_shares(self, msg: SubmitShares):
        """
        TODO: implement aggregation of sending SubmitSharesSuccess for a batch of successful submits
        """
        channel = self._mining_channel_registry.get_channel(msg.channel_id)

        assert (
            channel.conn_uid == self.connection.uid
        ), "Channel conn UID({}) doesn't match current conn UID({})".format(
            channel.conn_uid, self.connection.uid
        )
        self.__emit_channel_msg_on_bus(msg)

        def on_accept(diff_target: coins.Target):
            resp_msg = SubmitSharesSuccess(
                channel.id,
                last_seq_num=msg.seq_num,
                new_submits_accepted_count=1,
                new_shares_sum=diff_target.to_difficulty(),
            )
            self._send_msg(resp_msg)
            self.__emit_channel_msg_on_bus(resp_msg)

        def on_reject(_diff_target: coins.Target):
            resp_msg = SubmitSharesError(
                channel.id, seq_num=msg.seq_num, error_code='Share rejected'
            )
            self._send_msg(resp_msg)
            self.__emit_channel_msg_on_bus(resp_msg)

        self.pool.process_submit(
            msg.job_id, channel.session, on_accept=on_accept, on_reject=on_reject
        )

    def _on_vardiff_change(self, session: MiningSession):
        """Handle difficulty change for the current session.

        Note that to enforce difficulty change as soon as possible,
        the message is accompanied by generating new mining job
        """
        channel = session.owner
        self._send_msg(SetTarget(channel.id, session.curr_target))

        new_job_msg = self.__build_new_job_msg(channel, is_future_job=False)
        self._send_msg(new_job_msg)

    def on_new_block(self):
        """Sends an individual SetNewPrevHash message to all channels

        TODO: it is not quite clear how to handle the case where downstream has
         open multiple channels with the pool. The following needs to be
         answered:
         - Is any downstream node that opens more than 1 mining channel considered a
           proxy = it understands  grouping? MAYBE/YES but see next questions
         - Can we send only 1 SetNewPrevHash message even if the channels are
           standard? NO - see below
         - if only 1 group SetNewPrevHash message is sent what 'future' job should
           it reference? The problem is that we have no defined way where a future
           job is being shared by multiple channels.
        """
        # Pool currently doesn't support grouping channels, all channels belong to
        # group 0. We set the prev hash for all channels at once
        # Retire current jobs in the registries of all channels
        for channel in self._mining_channel_registry.channels:
            future_job = channel.take_future_job()
            prev_hash_msg = self.__build_set_new_prev_hash_msg(
                channel.id, future_job.uid
            )
            channel.session.job_registry.retire_all_jobs()
            channel.session.job_registry.add_job(future_job)
            # Now, we can send out the new prev hash, since all jobs are
            # invalidated. Any further submits for the invalidated jobs will be
            # rejected
            self._send_msg(prev_hash_msg)

        # We can now broadcast future jobs to all channels for the upcoming block
        for channel in self._mining_channel_registry.channels:
            future_new_job_msg = self.__build_new_job_msg(channel, is_future_job=True)
            self._send_msg(future_new_job_msg)

    def __build_set_new_prev_hash_msg(self, channel_id, future_job_id):
        return SetNewPrevHash(
            channel_id,
            self.pool.prev_hash,
            min_ntime=self.env.now,
            max_ntime_offset=7200,
            nbits=None,
            job_id=future_job_id,
            signature=Signature(),
        )

    @staticmethod
    def __build_new_job_msg(mining_channel: PoolMiningChannel, is_future_job: bool):
        """Builds NewMiningJob or NewExtendedMiningJob depending on channel type.

        The method also builds the actual job and registers it as 'future' job within
        the channel if requested.

        :param channel: determines the channel and thus message type
        :param is_future_job: when true, the job won't be considered for the current prev
         hash known to the downstream node but for any future prev hash that explicitly
         selects it
        :return New{Extended}MiningJob
        """
        new_job = mining_channel.session.new_mining_job()
        if is_future_job:
            mining_channel.add_future_job(new_job)

        # Compose the protocol message based on actual channel type
        if mining_channel.cfg.channel_type == MiningChannelType.STANDARD:
            msg = NewMiningJob(
                channel_id=mining_channel.id,
                job_id=new_job.uid,
                future_job=is_future_job,
                merkle_root=Hash(),
                version=None,
            )
        elif mining_channel.cfg.channel_type == MiningChannelType.EXTENDED:
            msg = NewExtendedMiningJob(
                channel_id=mining_channel.id,
                job_id=new_job.uid,
                future_job=is_future_job,
                merkle_path=MerklePath(),
                custom_id=None,
                cb_prefix=CoinBasePrefix(),
                cb_suffix=CoinBaseSuffix(),
            )
        else:
            assert False, 'Unsupported channel type: {}'.format(
                mining_channel.cfg.channel_type
            )

        return msg

    def __emit_channel_msg_on_bus(self, msg: ChannelMessage):
        """Helper method for reporting a channel oriented message on the debugging bus."""
        self._emit_protocol_msg_on_bus('Channel ID: {}'.format(msg.channel_id), msg)
