# Overview

This project is a simulation of the Stratum mining protocol. It currently supports both versions **Stratum V1** and **Stratum V2**. The intention is to verify the design of the **Stratum V2** with regards to translating between both protocol variants. At the same time, the platform can serve as a testbed for various network latency scenarios.

Last but not least, the idea is to have a reference implementation of both protocols that serves the blueprint specification of the messages


# Features

- network latency issues
- complete definition of protocol messages
- pool rejects stale shares since it simulates finding new blocks
- miners simulate finding shares based exponential distribution


## Install

The easiest way to run the simulation is to use python `virtualenv` and
 `virtualenvwrapper`


### The `virtualenvwrapper` way

```
apt install virtualenvwrapper
source /usr/share/virtualenvwrapper/virtualenvwrapper.sh
mkvirtualenv --python=/usr/bin/python3.7 stratum-sim
pip install -r ./requirements.txt
```


### Pure `virtualenv`

```
virtualenv --python=/usr/bin/python3.7 .stratum-sim
. .stratum-sim/bin/activate
pip install -r ./requirements.txt
```


### Python < 3.7
If you happen to have at least python 3.5 on your machine, remove the
 following line from the `requirements.txt` - it is the code formatter:

 ```
git+ssh://git@github.com/braiins/black.git@braiins-codestyle#egg=black
```


## Running Stratum V2 Simulation

`python ./pool_miner_sim.py --verbose --latency=0.2 --v1proto`
led


## Running Stratum V1 Simulation

`python ./pool_miner_sim.py --verbose --latency=0.2 --v1proto`


# Future Work

The simulation is far from complete, currently it supports the following
 scenarios:

```2xminer (V1) ----> pool (V1)```

```2xminer (V2) ----> pool (V1)```

Example scenarios that need to be to be covered:

```miner (V1) ----> proxy (V1:V2) ---> pool (V2)```
```miner (V2) ----> proxy (translating) ---> pool (V1)```

The current simulation output is very basic, below are a few points that could be covered. We are sure there is more that could be extended

- implement BDD scenarios using gherking language to run a full set of simulation scenarios
- provide more advanced statistics with chart plotting
