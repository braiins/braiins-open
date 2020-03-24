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


class ProtocolType:
    MINING_PROTOCOL = 0
    JOB_NEGOTIATION_PROTOCOL = 1
    TEMPLATE_DISTRIBUTION_PROTOCOL = 2
    JOB_DISTRIBUTION_PROTOCOL = 3


class MiningChannelType(enum.Enum):
    """Stratum V1 mining session follows the state machine below."""

    # Header only mining/standard
    STANDARD = 0
    EXTENDED = 1


class DownstreamConnectionFlags(enum.Enum):
    """Flags provided by downstream node"""

    #: The downstream node requires standard jobs. It doesn’t understand group channels - it is unable to process
    #: extended jobs sent to standard channels thru a group channel.
    REQUIRES_STANDARD_JOBS = 0

    #: If set, the client notifies the server that it will send SetCustomMiningJob on this connection
    REQUIRES_WORK_SELECTION = 1

    #: The client requires version rolling for efficiency or correct operation and the server MUST NOT send jobs
    #: which do not allow version rolling.
    REQUIRES_VERSION_ROLLING = 2


class UpstreamConnectionFlags(enum.Enum):
    """Flags provided by upstream node"""

    #: Upstream node will not accept any changes to the version field. Note that if REQUIRES_VERSION_ROLLING was set
    #: in the SetupConnection::flags field, this bit MUST NOT be set. Further, if this bit is set, extended jobs MUST
    #: NOT indicate support for version rolling.
    REQUIRES_FIXED_VERSION = 0

    #: Upstream node will not accept opening of a standard channel.
    REQUIRES_EXTENDED_CHANNELS = 1


class Hash:
    """Hash value doesn't need specific representation within the simulation"""

    pass


class MerklePath:
    """Merkle path doesn't need specific representation within the simulation"""

    pass


class CoinBasePrefix:
    pass


class CoinBaseSuffix:
    pass
