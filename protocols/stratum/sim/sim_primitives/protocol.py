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

"""Generic protocol primitives"""
import stringcase
from abc import abstractmethod

import simpy
from event_bus import EventBus

from sim_primitives.network import Connection


class Message:
    """Generic message that accepts visitors and dispatches their processing."""

    class VisitorMethodNotImplemented(Exception):
        """Custom handling to report if visitor method is missing"""

        def __init__(self, method_name):
            self.method_name = method_name

        def __str__(self):
            return self.method_name

    def __init__(self, req_id=None):
        self.req_id = req_id

    def accept(self, visitor):
        """Call visitor method based on the actual message type."""
        method_name = 'visit_{}'.format(stringcase.snakecase(type(self).__name__))
        try:
            visit_method = getattr(visitor, method_name)
        except AttributeError:
            raise self.VisitorMethodNotImplemented(method_name)

        visit_method(self)

    def _format(self, content):
        return '{}({})'.format(type(self).__name__, content)


class RequestRegistry:
    """Generates unique request ID for messages and provides simple registry"""

    def __init__(self):
        self.next_req_id = 0
        self.requests = dict()

    def push(self, req: Message):
        """Assigns a unique request ID to a message and registers it"""
        req.req_id = self.__next_req_id()
        assert (
            self.requests.get(req.req_id) is None
        ), 'BUG: request ID already present {}'.format(req.req_id)
        self.requests[req.req_id] = req

    def pop(self, req_id):
        return self.requests.pop(req_id, None)

    def __next_req_id(self):
        curr_req_id = self.next_req_id
        self.next_req_id += 1
        return curr_req_id


class ConnectionProcessor:
    """Receives and dispatches a message on a single connection."""

    def __init__(
        self, name: str, env: simpy.Environment, bus: EventBus, connection: Connection
    ):
        self.name = name
        self.env = env
        self.bus = bus
        self.connection = connection
        self.request_registry = RequestRegistry()
        self.receive_loop_process = self.env.process(self.__receive_loop())

    def terminate(self):
        self.receive_loop_process.interrupt()

    def send_request(self, req):
        """Register the request and send it down the line"""
        self.request_registry.push(req)
        self._send_msg(req)

    @abstractmethod
    def _send_msg(self, msg):
        pass

    @abstractmethod
    def _recv_msg(self):
        pass

    @abstractmethod
    def _on_invalid_message(self, msg):
        pass

    def _emit_aux_msg_on_bus(self, log_msg: str):
        self.bus.emit(self.name, self.env.now, self.connection.uid, log_msg)

    def _emit_protocol_msg_on_bus(self, log_msg: str, msg: Message):
        self._emit_aux_msg_on_bus('{}: {}'.format(log_msg, msg))

    def __receive_loop(self):
        """Receive process for a particular connection dispatches each received message
        """
        while True:
            try:
                msg = yield self.env.process(self._recv_msg())
                self._emit_protocol_msg_on_bus('INCOMING', msg)

                try:
                    msg.accept(self)
                except Message.VisitorMethodNotImplemented as e:
                    self._emit_protocol_msg_on_bus(
                        "{} doesn't implement:{}() for".format(type(self).__name_, e),
                        msg,
                    )
                #    self._on_invalid_message(msg)

            except simpy.Interrupt:
                self._emit_aux_msg_on_bus('DISCONNECTED')
                break  # terminate the event loop


class UpstreamConnectionProcessor(ConnectionProcessor):
    """Processes messages flowing through an upstream node

    This class only determines the direction in which it accesses the connection.
    """

    def _send_msg(self, msg):
        self.connection.incoming.put(msg)

    def _recv_msg(self):
        return self.connection.outgoing.get()

    @abstractmethod
    def _on_invalid_message(self, msg):
        pass


class DownstreamConnectionProcessor(ConnectionProcessor):
    """Processes messages flowing through a downstream node

    This class only determines the direction in which it accesses the connection.
    Also, the downstream processor is able to initiate the shutdown of the connection.
    """

    def _send_msg(self, msg):
        self.connection.outgoing.put(msg)

    def _recv_msg(self):
        return self.connection.incoming.get()

    def disconnect(self):
        """Downstream node may initiate disconnect

        """
        self.connection.disconnect()

    @abstractmethod
    def _on_invalid_message(self, msg):
        pass
