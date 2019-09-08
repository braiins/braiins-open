import enum

from ..miner import Miner
from ..protocol import DownstreamConnectionProcessor
from .messages import Authorize, Subscribe, Submit
from ..network import Connection

from ..pool import MiningJob


class MinerV1(DownstreamConnectionProcessor):
    class States(enum.Enum):
        INIT = enum.auto()
        AUTHORIZED = enum.auto()
        SUBSCRIBED = enum.auto()
        READY = enum.auto()
        RUNNING = enum.auto()

    def __init__(self, miner: Miner, connection: Connection):
        self.miner = miner
        self.state = self.States.INIT
        self.session = None
        super().__init__(miner.name, miner.env, miner.bus, connection)
        self.setup()

    def setup(self):
        auth_req = Authorize(req_id=None, user_name='some_miner', password='x')
        self.request_registry.push(auth_req)
        self._send_msg(auth_req)

        sbscr_req = Subscribe(
            req_id=None, signature='some_signature', extranonce1=None, url='some_url'
        )
        self.request_registry.push(sbscr_req)
        self._send_msg(sbscr_req)

    def submit_mining_solution(self, job: MiningJob):
        submit_req = Submit(
            req_id=None,
            user_name=self.miner.name,
            job_id=job.uid,
            extranonce2=None,
            time=self.miner.env.now,
            nonce=None,
        )
        self.request_registry.push(submit_req)
        self._send_msg(submit_req)

    def visit_ok_result(self, msg):
        req = self.request_registry.pop(msg.req_id)
        if req:
            if isinstance(req, Authorize):
                if self.state in (self.States.INIT,):
                    self.state = self.States.AUTHORIZED

    def visit_error_result(self, msg):
        req = self.request_registry.pop(msg.req_id)
        if req:
            self._emit_protocol_msg_on_bus(
                f"Error code {msg.code}, '{msg.msg}' for request", req
            )

    def visit_subscribe_response(self, msg):
        if self.state in (self.States.INIT, self.States.AUTHORIZED):
            self.state = self.States.SUBSCRIBED
            self._emit_protocol_msg_on_bus('Connection subscribed', msg)

    def visit_notify(self, msg):
        if self.state in (self.States.RUNNING, self.States.READY):
            job = self.session.new_mining_job(job_uid=msg.job_id)
            self.miner.mine_on_new_job(job, flush_any_pending_work=msg.clean_jobs)
            self.state = self.States.RUNNING

    def visit_set_difficulty(self, msg):
        try:
            self.session.set_target(msg.diff)
            self._emit_protocol_msg_on_bus('Difficulty updated', msg)
        except AttributeError:
            self.session = self.miner.new_mining_session(msg.diff)
            # self.state = self.States.RUNNING
            self._emit_protocol_msg_on_bus('Mining session created', msg)
        else:
            if self.state is self.States.SUBSCRIBED:
                self.state = self.States.READY

    def _on_invalid_message(self, msg):
        self._emit_protocol_msg_on_bus('Received invalid message', msg)
