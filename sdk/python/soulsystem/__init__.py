"""
SoulSystem Python SDK.

In production, this would wrap a native library via PyO3.
For now, it provides a mock Bus that can publish/receive messages.
"""

class Bus:
    """Mock bus compatible with SoulSystem message bus."""
    def __init__(self):
        self._messages = []

    def publish(self, message):
        self._messages.append(message)

    def receive(self):
        if self._messages:
            return self._messages.pop(0)
        return None
