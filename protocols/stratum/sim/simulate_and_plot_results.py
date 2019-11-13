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
import argparse
import multiprocessing

import matplotlib.pyplot as plt
import numpy as np
import simpy
from event_bus import EventBus

import sim_primitives.coins as coins
import sim_primitives.mining_params as mining_params
from sim_primitives.miner import Miner
from sim_primitives.network import Connection, ConnectionFactory
from sim_primitives.pool import Pool
from sim_primitives.proxy import Proxy
from sim_primitives.stratum_v1.miner import MinerV1
from sim_primitives.stratum_v1.pool import PoolV1
from sim_primitives.stratum_v2.miner import MinerV2
from sim_primitives.stratum_v2.pool import PoolV2
from sim_primitives.stratum_v2.proxy import V2ToV1Translation

bus = EventBus()


def sim_round(args):
    env = simpy.Environment()

    pool = Pool(
        'pool1',
        env,
        bus,
        protocol_type=args.get('pool'),
        default_target=coins.Target.from_difficulty(
            100000, mining_params.diff_1_target
        ),
        enable_vardiff=True,
        simulate_luck=True,
    )
    conn1 = Connection(
        env, 'stratum', mean_latency=args.get('latency'), latency_stddev_percent=10
    )
    conn2 = Connection(
        env, 'stratum', mean_latency=args.get('latency'), latency_stddev_percent=10
    )
    m1 = Miner(
        'miner1',
        env,
        bus,
        diff_1_target=mining_params.diff_1_target,
        protocol_type=args.get('miner'),
        device_information=dict(
            speed_ghps=10000,
            vendor='Bitmain',
            hardward_version='S9i 3.5',
            firmware='braiins-os-2018-09-22-2-hash',
            device_id='ac6f0145fccc1810',
        ),
        simulate_luck=True,
    )
    m2 = Miner(
        'miner2',
        env,
        bus,
        diff_1_target=mining_params.diff_1_target,
        protocol_type=args.get('miner'),
        device_information=dict(
            speed_ghps=13000,
            vendor='Bitmain',
            hardward_version='S9 3',
            firmware='braiins-os-2018-09-22-2-hash',
            device_id='ee030a7e4ea017cb',
        ),
        simulate_luck=True,
    )

    if args.get('proxy'):
        upstream = Proxy(
            'proxy',
            env,
            bus,
            translation_type=args.get('proxy'),
            upstream_connection_factory=ConnectionFactory(
                env=env,
                port='stratum',
                mean_latency=0.01,  # args.get('latency')  # this is small and constant
            ),
            upstream_node=pool,
            default_target=pool.default_target,
        )
    else:
        upstream = pool

    m1.connect_to_pool(conn1, upstream)
    m2.connect_to_pool(conn2, upstream)

    env.run(until=args.get('limit', 500))

    return {
        'accepted_shares': pool.accepted_shares,
        'accepted_submits': pool.accepted_submits,
        'stale_shares': pool.stale_shares,
        'stale_submits': pool.stale_submits,
        'rejected_submits': pool.rejected_submits,
        'latency': args.get('latency'),
    }


def gen_plot(
    args=None,
    file_name='share_graphs.pdf',
    latency_min=0.001,
    latency_max=0.5,
    number_of_points=50,
):
    def gen_params(args):
        for value in np.linspace(latency_min, latency_max, number_of_points):
            config = args.copy()
            config['latency'] = value
            yield config

    with multiprocessing.Pool() as proc_pool:
        x = proc_pool.map(sim_round, gen_params(args))

    accepted_submits = np.array(list(map(lambda x: x['accepted_submits'], x)))
    stale_submits = np.array(list(map(lambda x: x['stale_submits'], x)))
    rejected_submits = np.array(list(map(lambda x: x['rejected_submits'], x)))
    latencies = np.array(list(map(lambda x: x['latency'], x)))

    all_shares = np.array(
        [
            sum(tripple)
            for tripple in zip(accepted_submits, stale_submits, rejected_submits)
        ]
    )

    fig, (ax_tot, ax_acc, ax_stale, ax_rej) = plt.subplots(4, sharex=True)
    ax_tot.plot(latencies, accepted_submits, 'o-')
    ax_acc.plot(latencies, accepted_submits / all_shares, 'o-')
    ax_stale.plot(latencies, stale_submits / all_shares, 'o-')
    ax_rej.plot(latencies, rejected_submits / all_shares, 'o-')

    ax_tot.set(title='Total number of accepted shares')
    ax_acc.set(title='Fraction of accepted shares')
    ax_stale.set(title='Fraction of stale shares')
    ax_rej.set(title='Fraction of rejected shares', xlabel='latency [s]')

    fig.subplots_adjust(hspace=0.5)
    plt.savefig(file_name)


def main():
    parser = argparse.ArgumentParser(
        prog='pool_proxy_miner_sim.py',
        description='Simulates interaction of a mining pool and two miners in V1-V1, V2-V2'
        ' and V2-proxy-V1 configuration and stores result plots in 3 pdf files.',
    )
    parser.add_argument(
        '--latency_min', help='Minimal latency, (default 0.001)', type=float, default=0.001
    )
    parser.add_argument(
        '--latency_max', help='Maximal latency (default 0.5)', type=float, default=0.5
    )
    parser.add_argument(
        '--number_of_points',
        help='specify granularity of simulation',
        type=int,
        default=25,
    )
    parser.add_argument('--limit', help='Length of simulation (default 3000)', type=int, default=3000)
    args = parser.parse_args()

    gen_plot(
        args={'pool': PoolV1, 'miner': MinerV1, 'limit': args.limit},
        file_name='v1v1.pdf',
        latency_min=args.latency_min,
        latency_max=args.latency_max,
        number_of_points=args.number_of_points,
    )
    gen_plot(
        args={'pool': PoolV2, 'miner': MinerV2, 'limit': args.limit},
        file_name='v2v2.pdf',
        latency_min=args.latency_min,
        latency_max=args.latency_max,
        number_of_points=args.number_of_points,
    )
    gen_plot(
        args={
            'pool': PoolV1,
            'miner': MinerV2,
            'proxy': V2ToV1Translation,
            'limit': args.limit,
        },
        file_name='v2v1.pdf',
        latency_min=args.latency_min,
        latency_max=args.latency_max,
        number_of_points=args.number_of_points,
    )


if __name__ == '__main__':
    main()
