import numpy as np
import simpy
from .network import Connection, gen_uid
from event_bus import EventBus
from sim_primitives.stratum_v1.messages import Notify


class Miner(object):
    def __init__(
        self,
        name: str,
        env: simpy.Environment,
        bus: EventBus,
        speed_gh: float,
        connection: Connection,
        simulate_luck=True,
    ):
        self.name = name
        self.env = env
        self.bus = bus
        self.speed_gh = speed_gh
        self.connection = connection
        self.mine_proc = None
        self.job_uid = None
        self.share_diff = None
        self.recv_loop_process = None
        self.is_mining = True
        self.simulate_luck = simulate_luck

    def receive_loop(self):
        while True:
            try:
                x = yield self.connection.incoming.get()
                if isinstance(x, Notify):
                    self.job_uid = x.job_uid
                    # Share difficulty comes separately
                    # self.share_diff = x.share_diff
                    if self.mine_proc is None:
                        self.bus.emit(self.name, self.env.now, 'starting miner')
                        self.mine_proc = self.env.process(self.mine())
                    else:
                        self.mine_proc.interrupt()  # force restart
                #
                # elif isinstance(x, SubmitAccepted):
                #     self.bus.emit(self.name, self.env.now, 'submit accepted')
                # elif isinstance(x, SubmitRejected):
                #     self.bus.emit(self.name, self.env.now, 'submit rejected')
            except simpy.Interrupt:
                self.bus.emit(self.name, self.env.now, 'terminating recv loop')
                break

    def get_actual_speed(self):
        return self.speed_gh if self.is_mining else 0

    def mine(self):
        avg_time = self.share_diff * 4.294967296 / self.speed_gh
        self.bus.emit(
            self.name,
            self.env.now,
            'mining with diff {} / speed {} GHs / avg block time {} / job uid {}'.format(
                self.share_diff, self.speed_gh, avg_time, self.job_uid
            ),
        )
        while True:
            try:
                yield self.env.timeout(
                    np.random.exponential(avg_time) if self.simulate_luck else avg_time
                )
            except simpy.Interrupt:
                if not self.connection.is_connected():
                    self.bus.emit(
                        self.name, self.env.now, 'mining stopped (not connected)'
                    )
                    break

                self.bus.emit(
                    self.name, self.env.now, 'job/diff changed, restarting miner'
                )
                avg_time = self.share_diff * 4.294967296 / self.speed_gh
                self.bus.emit(
                    self.name,
                    self.env.now,
                    'mining with diff {} / speed {} GHs / avg block time {} / job uid {}'.format(
                        self.share_diff, self.speed_gh, avg_time, self.job_uid
                    ),
                )
                continue
            if self.is_mining:
                submit_uid = gen_uid(self.env)
                self.bus.emit(
                    self.name,
                    self.env.now,
                    'solution {} found for job {}'.format(submit_uid, self.job_uid),
                )
                # self.connection.outgoing.put(
                #     SubmitJob(
                #         self.connection.uid, self.job_uid, self.share_diff, submit_uid
                #     )
                # )

    def connect_to_pool(self, target):
        self.bus.emit(
            self.name, self.env.now, 'connecting to pool {}'.format(target.name)
        )
        self.connection.connect_to(target)
        self.recv_loop_process = self.env.process(self.receive_loop())

    def disconnect(self):
        if not self.connection.is_connected():
            raise ValueError('Not connected')
        self.bus.emit(self.name, self.env.now, 'disconnecting')
        if self.mine_proc:
            self.mine_proc.interrupt()
        self.recv_loop_process.interrupt()
        self.connection.disconnect()

    def set_is_mining(self, is_mining):
        self.is_mining = is_mining
