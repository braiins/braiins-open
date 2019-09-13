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
        self.send_request(auth_req)

        sbscr_req = Subscribe(
            req_id=None, signature='some_signature', extranonce1=None, url='some_url'
        )
        self.send_request(sbscr_req)

    def submit_mining_solution(self, job: MiningJob):
        submit_req = Submit(
            req_id=None,
            user_name=self.miner.name,
            job_id=job.uid,
            extranonce2=None,
            time=self.miner.env.now,
            nonce=None,
        )
        self.send_request(submit_req)

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
