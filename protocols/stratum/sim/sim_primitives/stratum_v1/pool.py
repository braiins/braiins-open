"""Stratum V1 pool implementation

"""
from ..network import Connection
from sim_primitives.pool import MiningSession, Pool
from .messages import *
import enum
import simpy
from event_bus import EventBus


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


class PoolV1(Pool):
    def __init__(self, *args, **kwargs):
        self.mining_sessions = dict()
        super().__init__(*args, **kwargs)

    def connect_in(self, connection: Connection):
        """Starts a new session on the incoming connection"""
        session = self.new_mining_session(
            connection, self._on_vardiff_change, clz=MiningSessionV1
        )
        # The new mining session is always identified by the connection UID as there can only be at most 1 session
        self.mining_sessions[connection.uid] = session
        # Register the connection only after building the mining session
        super().connect_in(connection)

    def disconnect(self, connection: Connection):
        """Shutdown the connection and terminate the session if the client managed to create one."""
        # Disconnect before shutting down the session
        super().disconnect(connection)

        if connection.uid in self.mining_sessions:
            self.mining_sessions[connection.uid].terminate()
            del self.mining_sessions[connection.uid]

    def visit_subscribe(self, msg: Subscribe):
        """Handle mining.subscribe.
        """
        mining_session = self._current_mining_session()

        if mining_session.state in (
            mining_session.States.INIT,
            mining_session.States.AUTHORIZED,
        ):
            # Subscribe is now complete we can activate a mining session that starts
            # generating new jobs immediately
            mining_session.state = mining_session.States.SUBSCRIBED
            self._send_msg(
                mining_session.owner.uid,
                SubscribeResponse(
                    subscription_ids=None,
                    # TODO: Extra nonce 1 is 8 bytes long and hardcoded
                    extranonce1=bytes([0] * 8),
                    extranonce2_size=self.extranonce2_size,
                ),
            )
            # Run the session so that it starts supplying jobs
            mining_session.run()
        else:
            self._send_msg(
                self.active_conn_uid,
                ErrorResult(
                    -1,
                    'Subscribe not expected when in: {}'.format(mining_session.state),
                ),
            )

    def visit_authorize(self, msg: Authorize):
        """Parse authorize.
        Sending authorize is legal at any state of the mining session.
        """
        mining_session = self._current_mining_session()
        mining_session.append_authorize(msg)
        self.bus.emit(
            self.name,
            self.env.now,
            self.active_conn_uid,
            'AUTHORIZE: {}'.format(mining_session.state),
            msg,
        )

    def visit_submit(self, msg: Submit):
        mining_session = self._current_mining_session()
        self.bus.emit(
            self.name,
            self.env.now,
            self.active_conn_uid,
            'SUBMIT: { }'.format(mining_session.state),
            msg,
        )
        self.process_submit(
            msg.job_id,
            mining_session,
            on_accept=lambda: self._send_msg(
                mining_session.owner.uid, OkResult(msg.req_id)
            ),
            on_reject=lambda: self._send_msg(
                mining_session.owner.uid,
                ErrorResult(-3, 'Too low difficulty'),
            ),
        )

    def _on_vardiff_change(self, session: MiningSession):
        """Handle difficulty change for the current session.

        Note that to enforce difficulty change as soon as possible,
        the message is accompanied by generating new mining job
        """
        self._send_msg(session.owner.uid, SetDifficulty(session.curr_diff))

        self._send_msg(
            session.owner.uid, self.__build_mining_notify(session, clean_jobs=False)
        )

    def _on_new_block(self):
        """Broadcast a new job to all sessions

        :return:
        """
        # broadcast the changed block to listeners
        for session in self.mining_sessions.values():
            self._send_msg(
                session.owner.uid, self.__build_mining_notify(session, clean_jobs=True)
            )

    def _on_invalid_message(self, msg):
        self._send_msg(
            self.active_conn_uid,
            ErrorResult(-2, 'Unrecognized message: {}'.format(msg)),
        )

    def _current_mining_session(self):
        """Accessor for the current mining session cannot fail.

         The session is always built upon accepting incoming connection"""
        mining_session = self.mining_sessions.get(self.active_conn_uid, None)
        assert (
            mining_session is not None
        ), 'Active connection UID {} is not associated with a mining session!'.format(
            self.active_conn_uid
        )
        return mining_session

    def __build_mining_notify(self, session: MiningSession, clean_jobs: bool):
        """
        :param session:
        :param clean_jobs: flag that causes the client to flush all its jobs
        and immediately start mining on this job
        :return: MiningNotify message
        """
        if clean_jobs:
            session.job_registry.retire_all_jobs()
        job = session.job_registry.new_mining_job(diff_target=session.curr_target)

        return Notify(
            job_id=job.uid,
            prev_hash=self.prev_hash,
            coin_base_1=None,
            coin_base_2=None,
            merkle_branch=None,
            version=None,
            bits=None,
            time=self.env.now,
            clean_jobs=clean_jobs,
        )
