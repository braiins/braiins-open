"""Stratum V1 pool implementation

"""
from sim_primitives.pool import MiningSession, Pool
from .messages import *
from ..protocol import UpstreamConnectionProcessor
import enum


class MiningSessionV1(MiningSession):
    """V1 specific mining session registers authorize requests """

    class States(enum.Enum):
        """Stratum V1 mining session follows the state machine below."""

        INIT = 0
        # BIP310 configuration step
        CONFIGURED = 1
        AUTHORIZED = 2
        SUBSCRIBED = 3
        RUNNING = 4

    def __init__(self, *args, **kwargs):
        self.state = self.States.INIT
        super().__init__(*args, **kwargs)

        self.authorize_requests = []

    def run(self):
        """V1 Session switches its state"""
        super().run()
        self.state = self.States.RUNNING

    def append_authorize(self, msg: Authorize):
        self.authorize_requests.append(msg)


class PoolV1(UpstreamConnectionProcessor):
    """Processes all messages on 1 connection

    """
    def __init__(self, pool, connection):
        self.pool = pool
        self.__mining_session = pool.new_mining_session(connection, self._on_vardiff_change, clz=MiningSessionV1)
        super().__init__(pool.name, pool.env, pool.bus, connection)

    @property
    def mining_session(self):
        """Accessor for the current mining session cannot fail.

        """
        assert (
            self.__mining_session is not None
        ), 'BUG: V1 Connection processor has no mining session!'
        return self.__mining_session

    def terminate(self):
        super().terminate()
        self.mining_session.terminate()

    def visit_subscribe(self, msg: Subscribe):
        """Handle mining.subscribe.
        """
        self.__emit_protocol_msg_on_bus_with_state(msg)

        if self.mining_session.state in (
            self.mining_session.States.INIT,
            self.mining_session.States.AUTHORIZED,
        ):
            # Subscribe is now complete we can activate a mining session that starts
            # generating new jobs immediately
            self.mining_session.state = self.mining_session.States.SUBSCRIBED
            self._send_msg(
                SubscribeResponse(
                    msg.req_id,
                    subscription_ids=None,
                    # TODO: Extra nonce 1 is 8 bytes long and hardcoded
                    extranonce1=bytes([0] * 8),
                    extranonce2_size=self.pool.extranonce2_size,
                )
            )
            # Run the session so that it starts supplying jobs
            self.mining_session.run()
        else:
            self._send_msg(
                ErrorResult(
                    msg.req_id,
                    -1,
                    'Subscribe not expected when in: {}'.format(
                        self.mining_session.state
                    ),
                )
            )

    def visit_authorize(self, msg: Authorize):
        """Parse authorize.
        Sending authorize is legal at any state of the mining session.
        """
        self.mining_session.append_authorize(msg)
        self.__emit_protocol_msg_on_bus_with_state(msg)
        # TODO: Implement username validation and fail to authorize for unknown usernames
        self._send_msg(OkResult(msg.req_id))

    def visit_submit(self, msg: Submit):
        self.__emit_protocol_msg_on_bus_with_state(msg)

        self.pool.process_submit(
            msg.job_id,
            self.mining_session,
            on_accept=lambda diff_target: self._send_msg(OkResult(msg.req_id)),
            on_reject=lambda diff_target: self._send_msg(
                ErrorResult(msg.req_id, -3, 'Too low difficulty')
            ),
        )

    def on_new_block(self):
        self._send_msg(self.__build_mining_notify(clean_jobs=True))

    def _on_invalid_message(self, msg):
        self._send_msg(
            ErrorResult(msg.req_id, -2, 'Unrecognized message: {}'.format(msg)),
        )

    def _on_vardiff_change(self, session: MiningSession):
        """Handle difficulty change for the current session.

        Note that to enforce difficulty change as soon as possible,
        the message is accompanied by generating new mining job
        """
        self._send_msg(SetDifficulty(session.curr_diff_target))

        self._send_msg(self.__build_mining_notify(clean_jobs=False))

    def __build_mining_notify(self, clean_jobs: bool):
        """
        :param clean_jobs: flag that causes the client to flush all its jobs
        and immediately start mining on this job
        :return: MiningNotify message
        """
        session = self.mining_session
        if clean_jobs:
            session.job_registry.retire_all_jobs()
        job = session.new_mining_job()

        return Notify(
            job_id=job.uid,
            prev_hash=self.pool.prev_hash,
            coin_base_1=None,
            coin_base_2=None,
            merkle_branch=None,
            version=None,
            bits=None,
            time=self.env.now,
            clean_jobs=clean_jobs,
        )

    def __emit_protocol_msg_on_bus_with_state(self, msg):
        """Common protocol message logging decorated with mining session state"""
        self._emit_protocol_msg_on_bus(
            '{}(state={})'.format(type(msg).__name__, self.mining_session.state), msg
        )
