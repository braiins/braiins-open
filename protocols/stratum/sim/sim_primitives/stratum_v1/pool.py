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

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        self.authorize_requests = []

    def append_authorize(self, msg: Authorize):
        self.authorize_requests.append(msg)


class PoolV1(Pool):
    class SessionStates(enum.Enum):
        """Stratum V1 mining session follows the state machine below."""

        INIT = 0
        # BIP310 configuration step
        CONFIGURED = 1
        AUTHORIZED = 2
        SUBSCRIBED = 3
        RUNNING = 4

    def __init__(self, *args, **kwargs):
        self.state = self.SessionStates.INIT
        self.mining_sessions = dict()
        # Temporary storage of per session Authorize requests, these will be moved into
        # the session once the session is started after receiving mining.subscribe
        self.tmp_mining_session_authorize_requests = dict()
        super().__init__(*args, **kwargs)

    def disconnect(self, connection: Connection):
        """Terminates the session if the client managed to create one."""
        if connection.uid in self.mining_sessions:
            self.mining_sessions[connection.uid].terminate()
            del self.mining_sessions[connection.uid]
        super().disconnect(connection)

    def new_mining_session_v1(self, uid):
        """Override mining session to build specifically V1 Session"""
        return self.new_mining_session(uid, self.on_vardiff_change, clz=MiningSessionV1)

    def visit_subscribe(self, msg: Subscribe):
        """Handle mining.subscribe.


        """
        if self.state in (self.SessionStates.INIT, self.SessionStates.AUTHORIZED):
            session = self.new_mining_session_v1(self.active_conn_uid)
            self.mining_sessions[self.active_conn_uid] = session

            if self.state == self.SessionStates.AUTHORIZED:
                # Transfer all authorize requests into the session and drop
                map(
                    self.mining_sessions[self.active_conn_uid].authorize,
                    self.tmp_mining_session_authorize_requests[self.active_conn_uid],
                )
                del self.tmp_mining_session_authorize_requests[self.active_conn_uid]
                self.state = self.SessionStates.RUNNING
            else:
                self.state = self.SessionStates.SUBSCRIBED

            self._send_msg(
                session.uid,
                SubscribeResponse(
                    subscription_ids=None,
                    # TODO: Extra nonce 1 is 8 bytes long and hardcoded
                    extra_nonce1=bytes([0] * 8),
                    extra_nonce2_size=self.extra_nonce2_size,
                ),
            )
        else:
            self._send_msg(
                self.active_conn_uid,
                ErrorResult(
                    -1, 'Subscribe not expected when in: {}'.format(self.state)
                ),
            )

    def visit_authorize(self, msg: Authorize):
        """Parse authorize.

        Depending on the state, temporarily store authorize credentials if a
        MiningSession doesn't exist yet (waiting for subscribe) or append them to the
        mining session
        """
        self.bus.emit(
            self.name,
            self.env.now,
            self.active_conn_uid,
            'AUTHORIZE: { }'.format(self.state),
            msg,
        )
        if self.state == self.SessionStates.INIT:
            self.tmp_mining_session_authorize_requests.setdefault(
                self.active_conn_uid, []
            )
            self.tmp_mining_session_authorize_requests[self.active_conn_uid].append(msg)
            self.state = self.SessionStates.AUTHORIZED
        elif self.state == self.SessionStates.SUBSCRIBED:
            self.mining_sessions[self.active_conn_uid].append_authorize(msg)
            self.state = self.SessionStates.RUNNING
        else:
            self._send_msg(
                self.active_conn_uid,
                ErrorResult(
                    -1, 'Authorize not expected when in: {}'.format(self.state)
                ),
            )

    def visit_submit(self, msg: Submit):
        self.bus.emit(
            self.name,
            self.env.now,
            self.active_conn_uid,
            'SUBMIT: { }'.format(self.state),
            msg,
        )
        self.process_submit(msg.job_id, self.mining_sessions[self.active_conn_uid])

    def on_vardiff_change(self, session: MiningSession):
        """Handle difficulty change for the current session.

        Note that to enforce difficulty change as soon as possible,
        the message is accompanied by generating new mining job
        """
        self._send_msg(session.uid, SetDifficulty(session.curr_diff))

        self._send_msg(
            session.uid, self.__build_mining_notify(session, clean_jobs=False)
        )

    def _on_new_block(self):
        """Broadcast a new job to all sessions

        :return:
        """
        # broadcast the changed block to listeners
        for session in self.mining_sessions.values():
            self._send_msg(
                session.uid, self.__build_mining_notify(session, clean_jobs=True)
            )

    def _on_submit_accepted(self):
        pass

    def on_submit_rejected(self):
        pass

    def __build_mining_notify(self, session: MiningSession, clean_jobs: bool):
        """
        :param session:
        :param clean_jobs: flag that causes the client to flush all its jobs
        and immediately start mining on this job
        :return: MiningNotify message
        """
        job = session.job_registry.new_mining_job(retire_old_jobs=clean_jobs)
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
