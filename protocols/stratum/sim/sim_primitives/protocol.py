"""Generic mining protocol primitives"""
import stringcase


class Message:
    """Generic message """

    def __init__(self, req_id=None):
        self.req_id = req_id

    def accept(self, visitor):
        """Call visitor method based on the actual message type."""
        getattr(visitor, 'visit_{}'.format(stringcase.snakecase(type(self).__name__)))(
            self
        )
