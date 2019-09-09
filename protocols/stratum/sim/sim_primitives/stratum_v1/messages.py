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

"""Stratum V1 messages."""
from ..protocol import Message


class Configure(Message):
    def __init__(self, req_id, extensions, extension_params):
        self.extensions = extensions
        self.extension_params = extension_params
        super().__init__(req_id)


class Authorize(Message):
    def __init__(self, req_id, user_name, password):
        self.user_name = user_name
        self.password = password
        super().__init__(req_id)


class Subscribe(Message):
    def __init__(self, req_id, signature, extranonce1, url):
        self.signature = signature
        self.extranonce1 = extranonce1
        self.url = url
        super().__init__(req_id)


class SubscribeResponse(Message):
    def __init__(self, req_id, subscription_ids, extranonce1, extranonce2_size):
        self.subscription_ids = subscription_ids
        self.extranonce1 = extranonce1
        self.extranonce2_size = extranonce2_size
        super().__init__(req_id)


class SetDifficulty(Message):
    def __init__(self, diff):
        self.diff = diff
        super().__init__()


class Submit(Message):
    def __init__(self, req_id, user_name, job_id, extranonce2, time, nonce):
        self.user_name = user_name
        self.job_id = job_id
        self.extranonce2 = extranonce2
        self.time = time
        self.nonce = nonce
        super().__init__(req_id)


class Notify(Message):
    def __init__(
        self,
        job_id,
        prev_hash,
        coin_base_1,
        coin_base_2,
        merkle_branch,
        version,
        bits,
        time,
        clean_jobs,
    ):
        self.job_id = job_id
        self.prev_hash = prev_hash
        self.coin_base_1 = coin_base_1
        self.coin_base_2 = coin_base_2
        self.merkle_branch = merkle_branch
        self.version = version
        self.bits = bits
        self.time = time
        self.clean_jobs = clean_jobs
        super().__init__()


class OkResult(Message):
    pass


class ErrorResult(Message):
    def __init__(self, req_id, code, msg):
        self.code = code
        self.msg = msg
        super().__init__(req_id)
