import simpy
import numpy as np
from sim_primitives.stratum_v1.pool import PoolV1
from sim_primitives.stratum_v2.pool import PoolV2
from sim_primitives.stratum_v1.miner import MinerV1
from sim_primitives.stratum_v2.miner import MinerV2
from sim_primitives.network import Connection
from sim_primitives.miner import Miner
from sim_primitives.pool import Pool
import sim_primitives.mining_params as mining_params
import sim_primitives.coins as coins

from event_bus import EventBus
from colorama import init, Fore
import argparse

init()
bus = EventBus()


def main():
    np.random.seed(123)
    parser = argparse.ArgumentParser(
        prog='pool_miner_sim.py',
        description='Simulates interaction of a mining pool and two miners',
    )
    parser.add_argument(
        '--realtime',
        help='run simulation in real-time (otherwise is run as fast as possible)',
        action='store_const',
        const=True,
    )
    parser.add_argument(
        '--rt-factor',
        help='real-time simulation factor, default=1 (enter 0.5 to be twice as fast than the real-time',
        type=float,
        default=1,
    )
    parser.add_argument(
        '--limit',
        type=int,
        help='simulation time limit in seconds, default = 500',
        default=500,
    )
    parser.add_argument(
        '--verbose',
        help='display all events (warning: a lot of text is generated)',
        action='store_const',
        const=True,
    )
    parser.add_argument(
        '--latency',
        help='average network latency in seconds, default=0.01',
        type=float,
        default=0.01,
    )
    parser.add_argument(
        '--no-luck', help='do not simulate luck', action='store_const', const=True
    )

    parser.add_argument(
        '--v1proto',
        help='run simulation with Stratum V1 protocol instead of V2',
        dest='protocol',
        action='store_const',
        const={'pool': PoolV1, 'miner': MinerV1},
        default={'pool': PoolV2, 'miner': MinerV2},
    )

    args = parser.parse_args()
    if args.realtime:
        env = simpy.rt.RealtimeEnvironment(factor=args.rt_factor)
        print(
            '*** starting simulation in real-time mode, factor {}'.format(
                args.rt_factor
            )
        )
    else:
        env = simpy.Environment()
        print('*** starting simulation (running as fast as possible)')

    if args.verbose:

        @bus.on('pool1')
        def subscribe_pool1(ts, conn_uid, message, aux=None):
            print(
                Fore.LIGHTCYAN_EX,
                'T+{0:.3f}:'.format(ts),
                '(pool1)',
                conn_uid if conn_uid is not None else '',
                message,
                aux,
                Fore.RESET,
            )

        @bus.on('miner1')
        def subscribe_m1(ts, conn_uid, message):
            print(
                Fore.LIGHTRED_EX,
                'T+{0:.3f}:'.format(ts),
                '(miner1)',
                conn_uid if conn_uid is not None else '',
                message,
                Fore.RESET,
            )

        @bus.on('miner2')
        def subscribe_m2(ts, conn_uid, message):
            print(
                Fore.LIGHTGREEN_EX,
                'T+{0:.3f}:'.format(ts),
                '(miner2)',
                conn_uid if conn_uid is not None else '',
                message,
                Fore.RESET,
            )

    pool = Pool(
        'pool1',
        env,
        bus,
        pool_protocol_type=args.protocol['pool'],
        default_target=coins.Target.from_difficulty(
            100000, mining_params.diff_1_target
        ),
        enable_vardiff=True,
        simulate_luck=not args.no_luck,
    )
    conn1 = Connection(
        env,
        'stratum',
        mean_latency=args.latency,
        latency_stddev_percent=0 if args.no_luck else 10,
    )
    m1 = Miner(
        'miner1',
        env,
        bus,
        diff_1_target=mining_params.diff_1_target,
        miner_protocol_type=args.protocol['miner'],
        speed_ghps=10000,
        simulate_luck=not args.no_luck,
    )
    m1.connect_to_pool(conn1, pool)
    conn2 = Connection(
        env,
        'stratum',
        mean_latency=args.latency,
        latency_stddev_percent=0 if args.no_luck else 10,
    )
    m2 = Miner(
        'miner2',
        env,
        bus,
        diff_1_target=mining_params.diff_1_target,
        miner_protocol_type=args.protocol['miner'],
        speed_ghps=8000,
        simulate_luck=not args.no_luck,
    )
    m2.connect_to_pool(conn2, pool)

    env.run(until=args.limit)
    print('simulation finished!')
    print(
        'accepted shares:',
        pool.accepted_shares,
        'accepted submits:',
        pool.accepted_submits,
    )
    print(
        'stale shares:',
        pool.stale_shares,
        'stale submits:',
        pool.stale_submits,
        'rejected submits:',
        pool.rejected_submits,
    )


if __name__ == '__main__':
    main()
