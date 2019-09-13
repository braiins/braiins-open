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

import numpy as np
import simpy
from event_bus import EventBus

import sim_primitives.coins as coins
from sim_primitives.hashrate_meter import HashrateMeter
from sim_primitives.network import Connection
from sim_primitives.pool import MiningSession, MiningJob
from sim_primitives.protocol import DownstreamConnectionProcessor


class Miner(object):
    def __init__(
        self,
        name: str,
        env: simpy.Environment,
        bus: EventBus,
        diff_1_target: int,
        protocol_type: DownstreamConnectionProcessor,
        device_information: dict,
        simulate_luck=True,
        *args,
        **kwargs
    ):
        self.name = name
        self.env = env
        self.bus = bus
        self.diff_1_target = diff_1_target
        self.protocol_type = protocol_type
        self.device_information = device_information
        self.connection_processor = None
        self.work_meter = HashrateMeter(env)
        self.mine_proc = None
        self.job_uid = None
        self.share_diff = None
        self.recv_loop_process = None
        self.is_mining = True
        self.simulate_luck = simulate_luck

    def get_actual_speed(self):
        return self.device_information.get('speed_ghps') if self.is_mining else 0

    def mine(self, job: MiningJob):
        share_diff = job.diff_target.to_difficulty()
        avg_time = share_diff * 4.294967296 / self.device_information.get('speed_ghps')

        # Report the current hashrate at the beginning when of mining
        self.__emit_hashrate_msg_on_bus(job, avg_time)

        while True:
            try:
                yield self.env.timeout(
                    np.random.exponential(avg_time) if self.simulate_luck else avg_time
                )
            except simpy.Interrupt:
                self.__emit_aux_msg_on_bus('Mining aborted (external signal)')
                break

            # To simulate miner failures we can disable mining
            if self.is_mining:
                self.work_meter.measure(share_diff)
                self.__emit_hashrate_msg_on_bus(job, avg_time)
                self.__emit_aux_msg_on_bus('solution found for job {}'.format(job.uid))

                self.connection_processor.submit_mining_solution(job)

    def connect_to_pool(self, connection: Connection, target):
        assert self.connection_processor is None, 'BUG: miner is already connected'
        connection.connect_to(target)

        self.connection_processor = self.protocol_type(self, connection)
        self.__emit_aux_msg_on_bus('Connecting to pool {}'.format(target.name))

    def disconnect(self):
        self.__emit_aux_msg_on_bus('Disconnecting from pool')
        if self.mine_proc:
            self.mine_proc.interrupt()
        # Mining is shutdown, terminate any protocol message processing
        self.connection_processor.terminate()
        self.connection_processor.disconnect()
        self.connection_processor = None

    def new_mining_session(self, diff_target: coins.Target):
        """Creates a new mining session"""
        session = MiningSession(
            name=self.name,
            env=self.env,
            bus=self.bus,
            # TODO remove once the backlinks are not needed
            owner=None,
            diff_target=diff_target,
            enable_vardiff=False,
        )
        self.__emit_aux_msg_on_bus('NEW MINING SESSION ()'.format(session))
        return session

    def mine_on_new_job(self, job: MiningJob, flush_any_pending_work=True):
        """Start working on a new job

         TODO implement more advanced flush policy handling (e.g. wait for the current
          job to finish if flush_flush_any_pending_work is not required)
        """
        # Interrupt the mining process for now
        if self.mine_proc is not None:
            self.mine_proc.interrupt()
        # Restart the process with a new job
        self.mine_proc = self.env.process(self.mine(job))

    def set_is_mining(self, is_mining):
        self.is_mining = is_mining

    def __emit_aux_msg_on_bus(self, msg: str):
        self.bus.emit(
            self.name,
            self.env.now,
            self.connection_processor.connection.uid
            if self.connection_processor
            else None,
            msg,
        )

    def __emit_hashrate_msg_on_bus(self, job: MiningJob, avg_share_time):
        """Reports hashrate statistics on the message bus

        :param job: current job that is being mined
        :return:
        """
        self.__emit_aux_msg_on_bus(
            'mining with diff {} | speed {} Gh/s | avg share time {} | job uid {}'.format(
                job.diff_target.to_difficulty(),
                self.work_meter.get_speed(),
                avg_share_time,
                job.uid,
            )
        )
