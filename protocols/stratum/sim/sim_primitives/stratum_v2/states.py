"""Protocol state related classes for Stratum V2

"""
from sim_primitives.stratum_v2.messages import SetupConnection, SetupConnectionSuccess


class ConnectionConfig:
    """Stratum V2 connection configurat

    """

    def __init__(self, setup_req: SetupConnection, setup_resp: SetupConnectionSuccess):
        self.req = setup_req
        self.resp = setup_resp
