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
from abc import ABC
from abc import abstractmethod

import random
from hashids import Hashids


def gen_uid(env):
    hashids = Hashids()
    return hashids.encode(int(env.now * 16), random.randint(0, 16777216))


class AcceptingConnection(ABC):
    @abstractmethod
    def connect_in(self, connection):
        pass

    @abstractmethod
    def disconnect(self, connection):
        pass


class ConnectionStore:
    """This class represents the propagation network connection."""

    def __init__(self, env, mean_latency, latency_stddev_percent):
        self.env = env
        self.mean_latency = mean_latency
        self.latency_stddev = 0.01 * latency_stddev_percent * mean_latency
        self.store = simpy.Store(env)

    def latency(self):
        if self.latency_stddev < 0.00001:
            delay = self.mean_latency
        else:
            delay = np.random.normal(self.mean_latency, self.latency_stddev)
        yield self.env.timeout(delay)

    def put(self, value):
        self.store.put(value)

    def get(self):
        value = yield self.store.get()
        yield self.env.process(self.latency())
        return value


class Connection:
    def __init__(self, env, port: str, mean_latency=0.01, latency_stddev_percent=10):
        self.uid = gen_uid(env)
        self.env = env
        self.port = port
        self.mean_latency = mean_latency
        self.latency_stddev_percent = latency_stddev_percent
        # Connection directions are from client prospective
        # Outgoing - client will store messages into the outgoing store,
        # while server will pickup the messages from the outgoing store
        self.outgoing = ConnectionStore(env, mean_latency, latency_stddev_percent)
        # Incoming - vice versa
        self.incoming = ConnectionStore(env, mean_latency, latency_stddev_percent)
        self.conn_target = None

    def connect_to(self, conn_target):
        conn_target.connect_in(self)
        self.conn_target = conn_target

    def disconnect(self):
        # TODO: Review whether to use assert's or RuntimeErrors in simulation
        if self.conn_target is None:
            raise RuntimeError('Not connected')
        self.conn_target.disconnect(self)
        self.conn_target = None

    def is_connected(self):
        return self.conn_target is not None


class ConnectionFactory:
    def __init__(self, env, port: str, mean_latency=0.01, latency_stddev_percent=10):
        self.env = env
        self.port = port
        self.mean_latency = mean_latency
        self.latency_stddev_percent = latency_stddev_percent

    def create_connection(self):
        return Connection(
            self.env,
            self.port,
            mean_latency=self.mean_latency,
            latency_stddev_percent=self.latency_stddev_percent,
        )
