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

from sim_primitives.miner import Miner
from sim_primitives.network import Connection
from sim_primitives.pool import MiningJob
from sim_primitives.protocol import DownstreamConnectionProcessor
from sim_primitives.stratum_v1.messages import (
    Configure,
    Authorize,
    Subscribe,
    SubscribeResponse,
    SetDifficulty,
    Submit,
    Notify,
    OkResult,
    ErrorResult,
)


class MinerV1(DownstreamConnectionProcessor):
    class States(enum.Enum):
        INIT = enum.auto()
        AUTHORIZED = enum.auto()
        AUTHORIZED_AND_SUBSCRIBED = enum.auto()
        SUBSCRIBED = enum.auto()
        RUNNING = enum.auto()

    def __init__(self, miner: Miner, connection: Connection):
        self.miner = miner
        self.state = self.States.INIT
        self.session = None
        self.desired_submits_per_sec = 0.3
        self.default_difficulty = self.miner.device_information.get('speed_ghps') / (
            4.294_967_296 * self.desired_submits_per_sec
        )
        super().__init__(miner.name, miner.env, miner.bus, connection)
        self.setup()

    def setup(self):
        self.session = self.miner.new_mining_session(self.default_difficulty)
        self.session.run()

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
        if not req:
            self._on_invalid_message(msg)
            return
        if isinstance(req, Authorize):
            if self.state == self.States.INIT:
                self.state = self.States.AUTHORIZED
            elif self.state == self.States.SUBSCRIBED:
                self.state = self.States.AUTHORIZED_AND_SUBSCRIBED
            self._emit_protocol_msg_on_bus('Connection authorized', msg)

    def visit_error_result(self, msg):
        req = self.request_registry.pop(msg.req_id)
        if req:
            self._emit_protocol_msg_on_bus(
                "Error code {}, '{}' for request".format(msg.code, msg.msg), req
            )

    def visit_subscribe_response(self, msg):
        if not self.request_registry.pop(msg.req_id):
            self._on_invalid_message(msg)
            return
        if self.state == self.States.INIT:
            self.state = self.States.SUBSCRIBED
        elif self.state == self.States.AUTHORIZED:
            self.state = self.States.AUTHORIZED_AND_SUBSCRIBED
        self._emit_protocol_msg_on_bus('Connection subscribed', msg)

    def visit_notify(self, msg):
        if self._allowed_to_mine:
            self.state = self.States.RUNNING
            job = self.session.new_mining_job(job_uid=msg.job_id)
            self.miner.mine_on_new_job(job, flush_any_pending_work=msg.clean_jobs)
        else:
            self._emit_protocol_msg_on_bus('Miner not fully initialized', msg)

    def visit_set_difficulty(self, msg):
        self.session.set_target(msg.diff)
        self._emit_protocol_msg_on_bus('Difficulty updated', msg)

    @property
    def _allowed_to_mine(self):
        return self.state in (
            self.States.RUNNING,
            self.States.AUTHORIZED_AND_SUBSCRIBED,
            self.States.SUBSCRIBED,
        )

    def _on_invalid_message(self, msg):
        self._emit_protocol_msg_on_bus('Received invalid message', msg)
