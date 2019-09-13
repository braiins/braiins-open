import enum

from sim_primitives.network import Connection
from sim_primitives.protocol import UpstreamConnectionProcessor
from sim_primitives.proxy import Proxy


class V1ToV2Translation(UpstreamConnectionProcessor):
    """Processes all messages on 1 connection

    """

    def _on_invalid_message(self, msg):
        pass

    class State(enum.Enum):
        pass

    def __init__(self, proxy: Proxy, connection: Connection):
        pass
