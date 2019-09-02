"""Generic mining protocol primitives"""


class Message:
    """Generic message """

    def __init__(self):
        pass
        # self.conn_uid = conn_uid

    def accept(self, visitor):
        """Call visitor method based on the actual message type."""
        getattr(visitor, 'visit_{}'.format(type(self).__name__.lower()))(self)
