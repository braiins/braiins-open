"""Generic protocol primitives"""
import stringcase
import simpy
from event_bus import EventBus
from abc import abstractmethod
from .network import Connection


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




class ConnectionProcessor:
    """Receives and dispatches a message on a single connection."""

    def __init__(self,
                 name: str,
                 env: simpy.Environment,
                 bus: EventBus,
                 connection: Connection
                 ):
        self.name = name
        self.env = env
        self.bus = bus
        self.connection = connection
        self.receive_loop_process = self.env.process(self.__receive_loop(self.connection.uid))

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
                msg = yield self._recv_msg()
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

    def terminate(self):
        self.receive_loop_process.interrupt()


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
    """

    def _send_msg(self, msg):
        self.connection.outgoing.put(msg)

    def _recv_msg(self):
        return self.connection.incoming.get()

    @abstractmethod
    def _on_invalid_message(self, msg):
        pass
