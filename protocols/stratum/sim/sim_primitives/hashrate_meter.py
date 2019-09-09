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

"""
this class estimates miner speed from reported shares
implemented using rolling time window
the HashrateMeter.roll method is called automatically each 5 seconds by default (granularity = 5)
"""
import numpy as np
import simpy


class HashrateMeter(object):
    def __init__(
        self,
        env: simpy.Environment,
        window_size: int = 60,
        granularity: int = 5,
        auto_hold_threshold=None,
    ):
        self.env = env
        self.time_started = 0
        self.window_size = window_size
        self.granularity = granularity
        self.pow_buffer = np.zeros(self.window_size // self.granularity)
        self.submit_buffer = np.zeros(self.window_size // self.granularity)
        self.frozen_time_buffer = np.zeros(self.window_size // self.granularity)
        self.roll_proc = env.process(self.roll())
        self.auto_hold_threshold = auto_hold_threshold
        self.on_hold = False
        self.put_on_hold_proc = None

    def reset(self, time_started):
        self.pow_buffer = np.zeros(self.window_size // self.granularity)
        self.submit_buffer = np.zeros(self.window_size // self.granularity)
        self.frozen_time_buffer = np.zeros(self.window_size // self.granularity)
        self.time_started = time_started
        if self.put_on_hold_proc:
            self.put_on_hold_proc.interrupt()  # terminate the current auto-on-hold process if exists

    def roll(self):
        while True:
            try:
                yield self.env.timeout(self.granularity)
                if not self.on_hold:
                    self.pow_buffer = np.roll(self.pow_buffer, 1)
                    self.pow_buffer[0] = 0
                    self.submit_buffer = np.roll(self.submit_buffer, 1)
                    self.submit_buffer[0] = 0
                    self.frozen_time_buffer = np.roll(self.frozen_time_buffer, 1)
                    self.frozen_time_buffer[0] = 0
                else:
                    self.frozen_time_buffer[0] += self.granularity
            except simpy.Interrupt:
                break

    def on_hold_after_timeout(self):
        try:
            yield self.env.timeout(self.auto_hold_threshold)
            self.on_hold = True
            self.put_on_hold_proc = None
        except simpy.Interrupt:
            pass  # do nothing

    def measure(self, share_diff: int):
        """Account for the shares

        TODO: consider changing the interface to accept the difficulty target directly
        """
        self.pow_buffer[0] += share_diff
        self.submit_buffer[0] += 1
        self.on_hold = False  # reset frozen status whenever a share is submitted
        if self.auto_hold_threshold:
            if self.put_on_hold_proc:
                self.put_on_hold_proc.interrupt()  # terminate the current auto-on-hold process if exists
            self.put_on_hold_proc = self.env.process(
                self.on_hold_after_timeout()
            )  # will trigger after the threshold

    def get_speed(self):
        total_time_held = np.sum(self.frozen_time_buffer)
        time_elapsed = self.env.now - self.time_started - total_time_held
        if time_elapsed > self.window_size:
            time_elapsed = self.window_size
        total_work = np.sum(self.pow_buffer)
        if time_elapsed < 1 or total_work == 0:
            return None

        return total_work * 4.294967296 / time_elapsed

    def get_submit_per_secs(self):
        total_time_held = np.sum(self.frozen_time_buffer)
        time_elapsed = self.env.now - self.time_started - total_time_held
        if time_elapsed < 1:
            return None
        elif time_elapsed > self.window_size:
            time_elapsed = self.window_size
        return np.sum(self.submit_buffer) / time_elapsed

    def is_on_hold(self):
        return self.on_hold

    def terminate(self):
        self.roll_proc.interrupt()
        if self.put_on_hold_proc:
            self.put_on_hold_proc.interrupt()  # terminate the current auto-on-hold process if exists
