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
