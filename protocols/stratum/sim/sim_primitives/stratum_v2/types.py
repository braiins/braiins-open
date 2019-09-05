"""Protocol specific types"""
import enum


class DeviceInfo:
    pass


class MiningChannelType(enum.Enum):
    """Stratum V1 mining session follows the state machine below."""

    # Header only mining/standard
    STANDARD = 0
    EXTENDED = 1


class DownstreamConnectionFlags(enum.Enum):
    """Flags provided by downstream node"""

    NONE = 0
    SUPPORTS_EXTENDED_CHANNELS = 1


class UpstreamConnectionFlags(enum.Enum):
    """Flags provided by upstream node"""

    NONE = 0
    DOESNT_SUPPORT_VERSION_ROLLING = 1


class Signature:
    """Message signature doesn't need specific representation within the simulation."""

    pass


class Hash:
    """Hash value doesn't need specific representation within the simulation"""

    pass


class MerklePath:
    """Merkle path doesn't need specific representation within the simulation"""

    pass


class PubKey:
    """Public key doesn't need specific representation within the simulation"""

    pass


class CoinBasePrefix:
    pass


class CoinBaseSuffix:
    pass
