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

"""Helper module with generic coin algorithms"""


class Target:
    def __init__(self, target: int, diff_1_target: int):
        self.target = target
        self.diff_1_target = diff_1_target

    def to_difficulty(self):
        """Converts target to difficulty at the network specified by diff_1_target"""
        return self.diff_1_target // self.target

    @staticmethod
    def from_difficulty(diff, diff_1_target):
        """Converts difficulty to target at the network specified by diff_1_target"""
        return Target(diff_1_target // diff, diff_1_target)

    def div_by_factor(self, factor: float):
        self.target = self.target // factor

    def __str__(self):
        return '{}(diff={})'.format(type(self).__name__, self.to_difficulty())
