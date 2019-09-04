import simpy
import numpy as np
from sim_primitives.stratum_v1.pool import PoolV1
from sim_primitives.network import Connection
from sim_primitives.miner import Miner
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
        def subscribe_m1(ts, message):
            print(
                Fore.LIGHTRED_EX,
                'T+{0:.3f}:'.format(ts),
                '(miner1)',
                message,
                Fore.RESET,
            )

        @bus.on('miner2')
        def subscribe_m2(ts, message):
            print(
                Fore.LIGHTGREEN_EX,
                'T+{0:.3f}:'.format(ts),
                '(miner2)',
                message,
                Fore.RESET,
            )

    pool = PoolV1(
        'pool1', env, bus, enable_vardiff=True, simulate_luck=not args.no_luck
    )
    conn1 = Connection(
        env,
        'stratum',
        mean_latency=args.latency,
        latency_stddev_percent=0 if args.no_luck else 10,
    )
    m1 = Miner('miner1', env, bus, 10000, conn1, simulate_luck=not args.no_luck)
    m1.connect_to_pool(pool)
    conn2 = Connection(
        env,
        'stratum',
        mean_latency=args.latency,
        latency_stddev_percent=0 if args.no_luck else 10,
    )
    m2 = Miner('miner2', env, bus, 8000, conn2, simulate_luck=not args.no_luck)
    m2.connect_to_pool(pool)

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
