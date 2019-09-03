"""Stratum V1 messages."""
from ..protocol import Message


class Configure(Message):
    def __init__(self, extensions, extension_params):
        self.extensions = extensions
        self.extension_params = extension_params


class Authorize(Message):
    def __init__(self, user_name, password):
        self.user_name = user_name
        self.password = password


class Subscribe(Message):
    def __init__(self, signature, extranonce1, url):
        self.signature = signature
        self.extranonce1 = extranonce1
        self.url = url


class SubscribeResponse(Message):
    def __init__(self, subscription_ids, extranonce1, extranonce2_size):
        self.subscription_ids = subscription_ids
        self.extranonce1 = extranonce1
        self.extranonce2_size = extranonce2_size


class SetDifficulty(Message):
    def __init__(self, diff):
        self.diff = diff


class Submit(Message):
    def __init__(self, user_name, job_id, extranonce2, time, nonce):
        self.user_name = user_name
        self.job_id = job_id
        self.extranonce2 = extranonce2
        self.time = time
        self.nonce = nonce


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


class OkResult(Message):
    pass


class ErrorResult(Message):
    def __init__(self, code, msg):
        self.code = code
        self.msg = msg
