"""Pytest configuration for ZeroBot tests."""

import os
import sys

# Ensure the project root is on the path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Disable Docker-dependent modules during tests
os.environ.setdefault("ZEROBOT_CONFIG", "/dev/null")

import pytest


def pytest_configure(config):
    """Configure pytest-asyncio default loop scope."""
    config.option.asyncio_mode = "auto"


@pytest.fixture(autouse=True)
def disable_docker():
    """Prevent any real Docker calls during tests."""
    import docker

    original_from_env = docker.from_env
    docker.from_env = lambda: None  # noqa: ARG005
    yield
    docker.from_env = original_from_env
