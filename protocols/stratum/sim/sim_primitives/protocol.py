"""Generic mining protocol primitives"""
import stringcase


class Message:
    """Generic message """

    def __init__(self):
        pass
        # self.conn_uid = conn_uid

    def accept(self, visitor):
        """Call visitor method based on the actual message type."""
        getattr(visitor, 'visit_{}'.format(stringcase.snakecase(type(self).__name__)))(
            self
        )
