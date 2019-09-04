from .network import Connection, AcceptingConnection, gen_uid

import hashlib
import numpy as np
import simpy
from event_bus import EventBus
from sim_primitives.hashrate_meter import HashrateMeter
from abc import abstractmethod


class MiningJob:
    """This class allows the simulation to track per job difficulty target for
    correct accounting"""

    def __init__(self, uid, diff_target):
        """
        :param uid:
        :param diff_target: difficulty target
        """
        self.uid = uid
        self.diff_target = diff_target


class MiningJobRegistry:
    """Registry of jobs that have been assigned for mining.

    The registry intentionally doesn't remove any jobs from the simulation so that we
    can explicitely account for 'stale' hashrate. When this requirement is not needed,
    the __next_job_uid() can be adjusted accordingly"""

    def __init__(self):
        # Tracking mininum valid job ID
        self.min_valid_job_uid = self.next_job_uid = 0
        # Registered jobs based on their uid
        self.jobs = dict()

    def new_mining_job(self, diff_target):
        """Prepares new mining job and registers it internally"""
        new_job = MiningJob(diff_target, self.__next_job_uid())
        self.jobs[new_job.uid] = new_job
        return new_job

    def get_job_diff_target(self, job_uid):
        return self.jobs[job_uid].diff_target

    def contains(self, job_uid):
        """Job ID presence check
        :return True when when such Job ID exists in the registry (it may still not
        be valid)"""
        return job_uid in self.jobs

    def is_job_uid_valid(self, job_uid):
        """A valid job """
        return self.jobs[job_uid] >= self.min_valid_job_uid

    def retire_all_jobs(self):
        """Make all jobs invalid"""
        self.min_valid_job_uid = self.next_job_uid

    def __next_job_uid(self):
        """Initializes a new job ID for this session.
        """
        curr_job_uid = self.next_job_uid
        self.next_job_uid += 1

        return curr_job_uid


class MiningSession:
    """Represents a mining session that can adjust its difficulty target"""

    min_factor = 0.25
    max_factor = 4

    def __init__(
        self,
        name: str,
        env: simpy.Environment,
        bus: EventBus,
        owner,
        diff,
        diff_1_target,
        enable_vardiff,
        vardiff_time_window=None,
        vardiff_desired_submits_per_sec=None,
        on_vardiff_change=None,
    ):
        """
        :param diff_1_target: Difficulty 1 target to calculate current maximum target
        based on current difficulty. This value is network/coin specific.
        """
        self.name = name
        self.env = env
        self.bus = bus
        self.owner = owner
        self.curr_diff = diff
        self.diff_1_target = diff_1_target
        self.enable_vardiff = enable_vardiff
        self.meter = None
        self.vardiff_process = None
        self.vardiff_time_window_size = vardiff_time_window
        self.vardiff_desired_submits_per_sec = vardiff_desired_submits_per_sec
        self.on_vardiff_change = on_vardiff_change

        self.job_registry = MiningJobRegistry()

    @property
    def curr_target(self):
        """Derives target from current difficulty on the session"""
        return self.diff2target(self.curr_diff)

    def target2diff(self, target):
        """Converts target to difficulty at the network where this session is running."""
        return self.diff_1_target // target

    def diff2target(self, diff):
        """Converts difficu to target at the network where this session is running."""
        return self.diff_1_target // diff

    def run(self):
        """Explicit activation starts any simulation processes associated with the session"""
        if self.enable_vardiff:
            self.meter = HashrateMeter(self.env)
            self.vardiff_process = self.env.process(self.__vardiff_loop())

    def terminate(self):
        """Complete shutdown of the session"""
        if self.enable_vardiff:
            self.vardiff_process.interrupt()
            self.meter.terminate()

    def __vardiff_loop(self):
        while True:
            try:
                submits_per_sec = self.meter.get_submit_per_secs()
                if submits_per_sec == 0:
                    # no accepted shares, we will halve the diff
                    factor = 0.5
                else:
                    factor = submits_per_sec / self.vardiff_desired_submits_per_sec
                if factor < self.min_factor:
                    factor = self.min_factor
                elif factor > self.max_factor:
                    factor = self.max_factor
                new_diff = self.curr_diff * factor
                self.curr_diff = int(round(new_diff))
                self.bus.emit(
                    self.name, self.env.now, self.owner, 'DIFF_UPDATE', self.curr_diff
                )
                self.on_vardiff_change(self)
                yield self.env.timeout(self.vardiff_time_window_size)
            except simpy.Interrupt:
                break


class Pool(AcceptingConnection):
    """Represents a generic mining pool.

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
        default_difficulty: int = 100000,
        diff_1_target: int = 0xFFFF << 208,
        extranonce2_size: int = 8,
        avg_pool_block_time: float = 60,
        enable_vardiff: bool = False,
        desired_submits_per_sec: float = 0.3,
        simulate_luck: bool = True,
    ):
        self.name = name
        self.env = env
        self.bus = bus
        self.default_difficulty = default_difficulty
        self.diff_1_target = diff_1_target
        self.extranonce2_size = extranonce2_size
        self.avg_pool_block_time = avg_pool_block_time

        self.connections = dict()
        # Prepare initial prevhash for the very first
        self.__generate_new_prev_hash()
        # TODO: review alternatives for current connection processing
        # Connection that is currently being processed
        self.active_conn_uid = None

        self.recv_loop_processes = dict()
        self.pow_update_process = env.process(self.__pow_update())

        self.meter_accepted = HashrateMeter(self.env)
        self.meter_rejected_stale = HashrateMeter(self.env)
        self.meter_process = env.process(self.__pool_speed_meter())
        self.enable_vardiff = enable_vardiff
        self.desired_submits_per_sec = desired_submits_per_sec
        self.simulate_luck = simulate_luck

        self.extra_meters = []

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
        self.connections[connection.uid] = connection
        self.recv_loop_processes[connection.uid] = self.env.process(
            self.__receive_loop(connection.uid)
        )

    def disconnect(self, connection: Connection):
        if connection.uid not in self.connections:
            return
        self.recv_loop_processes[connection.uid].interrupt()
        del self.connections[connection.uid]
        del self.recv_loop_processes[connection.uid]

    def new_mining_session(self, owner, on_vardiff_change, clz=MiningSession):
        """Creates a new mining session"""
        session = clz(
            name=self.name,
            env=self.env,
            bus=self.bus,
            owner=owner,
            diff=self.default_difficulty,
            diff_1_target=self.diff_1_target,
            enable_vardiff=self.enable_vardiff,
            vardiff_time_window=self.meter_accepted.window_size,
            vardiff_desired_submits_per_sec=self.desired_submits_per_sec,
            on_vardiff_change=on_vardiff_change,
        )
        self.bus.emit(self.name, self.env.now, owner, 'NEW MINING SESSION', session)
        return session

    def add_extra_meter(self, meter: HashrateMeter):
        self.extra_meters.append(meter)

    def process_submit(
        self, submit_job_uid, session: MiningSession, on_accept, on_reject
    ):
        if not session.job_registry.contains(submit_job_uid):
            diff = None
        else:
            diff = session.target2diff(
                session.job_registry.get_job_diff_target(submit_job_uid)
            )

        # Accept all jobs with valid UID
        if session.job_registry.is_job_uid_valid(submit_job_uid):
            self.accepted_submits += 1
            self.accepted_shares += diff
            self.meter_accepted.measure(diff)
            session.meter.measure(diff)
            on_accept(diff)
        else:
            # Account for stale submits and shares (but job exists in the registry)
            if diff is not None:
                self.stale_submits += 1
                self.stale_shares += diff
                self.meter_rejected_stale.measure(diff)
            else:
                self.rejected_submits += 1
            on_reject(diff)

    def _send_msg(self, conn_uid, msg):
        self.connections[conn_uid].incoming.put(msg)

    @abstractmethod
    def _on_new_block(self):
        pass

    @abstractmethod
    def _on_invalid_message(self, msg):
        pass

    def __receive_loop(self, conn_uid: str):
        """
        :param conn_uid:
        """
        while True:
            try:
                msg = yield self.connections[conn_uid].outgoing.get()
                self.bus.emit(
                    self.name, self.env.now, conn_uid, 'INCOMING: {}'.format(msg)
                )
                try:
                    # Set the connection UID for later processing in the
                    # visitor method
                    self.active_conn_uid = conn_uid
                    msg.accept(self)
                except AttributeError as e:
                    self._on_invalid_message(msg)
                self.active_conn_uid = None

            except simpy.Interrupt:
                self.bus.emit(self.name, self.env.now, conn_uid, 'DISCONNECTED')
                break  # terminate the event loop

    def __pow_update(self):
        """This process simulates finding new blocks based on pool's hashrate"""
        while True:
            # simulate pool block time using exponential distribution
            yield self.env.timeout(
                np.random.exponential(self.avg_pool_block_time)
                if self.simulate_luck
                else self.avg_pool_block_time
            )
            # Simulate the new block hash by calculating sha256 of current time
            self.__generate_new_prev_hash()

            self.bus.emit(
                self.name,
                self.env.now,
                None,
                'NEW_BLOCK: {}'.format(self.prev_hash.hex()),
            )
            self._on_new_block()

    def __generate_new_prev_hash(self):
        """Generates a new prevhash based on current time.
        """
        # TODO: this is not very precise as to events that would trigger this method in
        #  the same second would yield the same prev hash value,  we should consider
        #  specifying prev hash as a simple sequence number
        self.prev_hash = hashlib.sha256(bytes(int(self.env.now))).digest()

    def __pool_speed_meter(self):
        while True:
            yield self.env.timeout(self.meter_period)
            speed = self.meter_accepted.get_speed()
            submit_speed = self.meter_accepted.get_submit_per_secs()
            if speed is None or submit_speed is None:
                self.bus.emit(
                    self.name, self.env.now, None, 'SPEED', 'N/A GH/s, N/A submits/s'
                )
            else:
                self.bus.emit(
                    self.name,
                    self.env.now,
                    None,
                    'SPEED',
                    '{0:.2f} GH/s, {1:.4f} submits/s'.format(speed, submit_speed),
                )
