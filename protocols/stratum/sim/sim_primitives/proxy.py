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

"""Generic pool module"""
import hashlib

import numpy as np
import simpy
from event_bus import EventBus

import sim_primitives.coins as coins
from sim_primitives.hashrate_meter import HashrateMeter
from sim_primitives.protocol import (
    UpstreamConnectionProcessor,
    DownstreamConnectionProcessor,
)
from sim_primitives.network import Connection, AcceptingConnection, ConnectionFactory
from sim_primitives.pool import MiningSession


class Proxy(AcceptingConnection):
    """Represents a generic proxy for translating of forwarding a protocol.

    The pool keeps statistics about:

    - accepted submits and shares: submit count and difficulty sum (shares) for valid
    solutions
    - stale submits and shares: submit count and difficulty sum (shares) for solutions
    that have been sent after new block is found
    - rejected submits: submit count of invalid submit attempts that don't refer any
    particular job
    """

    meter_period = 60

    def __init__(
        self,
        name: str,
        env: simpy.Environment,
        bus: EventBus,
        translation_type: UpstreamConnectionProcessor,
        upstream_connection_factory: ConnectionFactory,
        upstream_node: AcceptingConnection,
        default_target: coins.Target,
        extranonce2_size: int = 8,
    ):
        """

        :param translation_type: object for handling incoming downstream
        connections (requires an UpstreamConnectionProcessor as we are handling
        incoming connections)
        """
        self.name = name
        self.env = env
        self.bus = bus
        self.default_target = default_target
        self.extranonce2_size = extranonce2_size

        # Per connection message processors
        self.connection_processors = dict()
        self.connection_processor_clz = translation_type

        self.upstream_node = upstream_node
        self.upstream_connection_factory = upstream_connection_factory

        self.meter_accepted = HashrateMeter(self.env)
        self.meter_rejected_stale = HashrateMeter(self.env)
        self.meter_process = env.process(self.__pool_speed_meter())

        self.accepted_submits = 0
        self.stale_submits = 0
        self.rejected_submits = 0

        self.accepted_shares = 0
        self.stale_shares = 0

    def reset_stats(self):
        self.accepted_submits = 0
        self.stale_submits = 0
        self.rejected_submits = 0
        self.accepted_shares = 0
        self.stale_shares = 0

    def connect_in(self, connection: Connection):
        if connection.port != 'stratum':
            raise ValueError('{} port is not supported'.format(connection.port))
        # Build message processor for the new connection
        self.connection_processors[connection.uid] = self.connection_processor_clz(
            self, connection
        )

    def disconnect(self, connection: Connection):
        if connection.uid not in self.connection_processors:
            return
        self.connection_processors[connection.uid].terminate()
        del self.connection_processors[connection.uid]

    def new_mining_session(self, owner, on_vardiff_change, clz=MiningSession):
        """Creates a new mining session"""
        session = clz(
            name=self.name,
            env=self.env,
            bus=self.bus,
            owner=owner,
            diff_target=self.default_target,
            enable_vardiff=self.enable_vardiff,
            vardiff_time_window=self.meter_accepted.window_size,
            vardiff_desired_submits_per_sec=self.desired_submits_per_sec,
            on_vardiff_change=on_vardiff_change,
        )
        self.__emit_aux_msg_on_bus('NEW MINING SESSION ()'.format(session))

        return session

    def account_accepted_shares(self, diff_target: coins.Target):
        self.accepted_submits += 1
        self.accepted_shares += diff_target.to_difficulty()
        self.meter_accepted.measure(diff_target.to_difficulty())

    def account_stale_shares(self, diff_target: coins.Target):
        self.stale_submits += 1
        self.stale_shares += diff_target.to_difficulty()
        self.meter_rejected_stale.measure(diff_target.to_difficulty())

    def account_rejected_submits(self):
        self.rejected_submits += 1

    def process_submit(
        self, submit_job_uid, session: MiningSession, on_accept, on_reject
    ):
        if session.job_registry.contains(submit_job_uid):
            diff_target = session.job_registry.get_job_diff_target(submit_job_uid)
            # Global accounting
            self.account_accepted_shares(diff_target)
            # Per session accounting
            session.account_diff_shares(diff_target.to_difficulty())
            on_accept(diff_target)
        elif session.job_registry.contains_invalid(submit_job_uid):
            diff_target = session.job_registry.get_invalid_job_diff_target(
                submit_job_uid
            )
            self.account_stale_shares(diff_target)
            on_reject(diff_target)
        else:
            self.account_rejected_submits()
            on_reject(None)

    def __pool_speed_meter(self):
        while True:
            yield self.env.timeout(self.meter_period)
            speed = self.meter_accepted.get_speed()
            submit_speed = self.meter_accepted.get_submit_per_secs()
            if speed is None or submit_speed is None:
                self.__emit_aux_msg_on_bus('SPEED: N/A Gh/s, N/A submits/s')
            else:
                self.__emit_aux_msg_on_bus(
                    'SPEED: {0:.2f} Gh/s, {1:.4f} submits/s'.format(speed, submit_speed)
                )

    def __emit_aux_msg_on_bus(self, msg):
        self.bus.emit(self.name, self.env.now, None, msg)
