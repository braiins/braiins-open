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


class Channel:
    """This class represents the propagation network connection."""

    def __init__(self, env, mean_latency, latency_stddev_percent):
        self.env = env
        self.mean_latency = mean_latency
        self.latency_stddev = 0.01 * latency_stddev_percent * mean_latency
        self.store = simpy.Store(env)

    def latency(self, value):
        if self.latency_stddev < 0.00001:
            delay = self.mean_latency
        else:
            delay = np.random.normal(self.mean_latency, self.latency_stddev)
        yield self.env.timeout(delay)
        self.store.put(value)

    def put(self, value):
        self.env.process(self.latency(value))

    def get(self):
        return self.store.get()


class Connection:
    def __init__(self, env, port: str, mean_latency=0.01, latency_stddev_percent=10):
        self.uid = gen_uid(env)
        self.env = env
        self.port = port
        self.mean_latency = mean_latency
        self.latency_stddev_percent = latency_stddev_percent
        # Channel directions are from client prospective
        # Outgoing - client will store messages into this channel,
        # while server will pickup the messages from this channell
        self.outgoing = Channel(env, mean_latency, latency_stddev_percent)
        # Incoming - vica versa
        self.incoming = Channel(env, mean_latency, latency_stddev_percent)
        self.target = None

    def connect_to(self, target):
        target.connect_in(self)
        self.target = target

    def disconnect(self):
        if self.target is None:
            raise RuntimeError('Not connected')
        self.target.disconnect(self)
        self.target = None

    def is_connected(self):
        return self.target is not None


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
