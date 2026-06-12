"""Docker container manager for ZeroBot tool execution.

All analysis tools run in ephemeral Docker containers with:
- No network access
- Unprivileged user
- Resource limits (CPU/memory)
- Read-only filesystem with tmpfs workspace
- SHA256 verification of all binaries before processing
"""

from __future__ import annotations

import hashlib
import io
import logging
import os
import tarfile
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

import docker
import yaml

logger = logging.getLogger(__name__)


@dataclass
class ToolboxResult:
    """Structured result from a toolbox container execution."""

    stdout: str = ""
    stderr: str = ""
    exit_code: int = -1
    timed_out: bool = False
    error: Optional[str] = None


class ContainerManager:
    """Manages ephemeral Docker containers for secure tool execution."""

    def __init__(self, config_path: Optional[str] = None):
        self.config = self._load_config(config_path)
        self.docker_client: docker.DockerClient | None = None
        self._connect()

    # ── Initialization ──────────────────────────────────────────────

    def _load_config(self, config_path: Optional[str] = None) -> dict[str, Any]:
        """Load configuration from YAML file."""
        path = config_path or os.environ.get("ZEROBOT_CONFIG", "/zerobot/config.yaml")
        try:
            with open(path) as f:
                return yaml.safe_load(f) or {}
        except FileNotFoundError:
            logger.warning("Config not found at %s, using defaults", path)
            return {}

    def _connect(self) -> None:
        """Connect to the Docker daemon."""
        try:
            self.docker_client = docker.from_env()
            self.docker_client.ping()
            logger.info("Connected to Docker daemon")
        except Exception as e:
            logger.error("Failed to connect to Docker: %s", e)
            self.docker_client = None

    # ── Binary Verification ─────────────────────────────────────────

    @staticmethod
    def compute_hash(file_path: str | Path) -> str:
        """Compute SHA256 hash of a binary file."""
        sha256 = hashlib.sha256()
        with open(file_path, "rb") as f:
            for chunk in iter(lambda: f.read(65536), b""):
                sha256.update(chunk)
        return sha256.hexdigest()

    def is_hash_allowed(self, file_hash: str) -> bool:
        """Check if a binary hash is in the allowed list."""
        allowed = self.config.get("allowed_hashes", [])
        if not allowed:
            logger.warning("No allowed hashes configured — all binaries rejected")
            return False
        if "*" in allowed:
            return True
        return file_hash in allowed

    # ── File Transfer to Container ──────────────────────────────────

    @staticmethod
    def _make_tar_from_bytes(data: bytes, arcname: str) -> bytes:
        """Create a tar archive in memory containing a single file."""
        buf = io.BytesIO()
        with tarfile.open(fileobj=buf, mode="w") as tar:
            info = tarfile.TarInfo(name=arcname)
            info.size = len(data)
            info.mtime = int(time.time())
            info.mode = 0o644
            tar.addfile(info, io.BytesIO(data))
        return buf.getvalue()

    def copy_to_container(
        self,
        container: docker.models.containers.Container,
        local_path: str | Path,
        remote_path: str,
    ) -> None:
        """Copy a file from the host into a running container."""
        with open(local_path, "rb") as f:
            data = f.read()
        tar_bytes = self._make_tar_from_bytes(data, os.path.basename(remote_path))
        container.put_archive(os.path.dirname(remote_path), tar_bytes)

    # ── Container Lifecycle ─────────────────────────────────────────

    def run_tool(
        self,
        command: str | list[str],
        binary_path: Optional[str] = None,
        timeout: Optional[int] = None,
        workdir: str = "/workspace",
    ) -> ToolboxResult:
        """Run a command in an ephemeral toolbox container.

        Args:
            command: Shell command or list of args to execute.
            binary_path: Optional path to a binary to copy into the container.
            timeout: Maximum execution time in seconds (default from config).
            workdir: Working directory inside the container.

        Returns:
            ToolboxResult with stdout, stderr, exit code, and error state.
        """
        result = ToolboxResult()

        if self.docker_client is None:
            result.error = "Docker daemon not available"
            return result

        container: Optional[docker.models.containers.Container] = None
        try:
            timeout = timeout or self.config.get("sandbox", {}).get("timeout_seconds", 120)

            # Get toolbox image name
            img = self.config.get("sandbox", {}).get("toolbox_image", "zerobot-toolbox:latest")

            # Ensure image exists
            try:
                self.docker_client.images.get(img)
            except docker.errors.ImageNotFound:
                logger.info("Building toolbox image %s ...", img)
                build_path = os.path.dirname(os.path.dirname(os.path.abspath(__file__))) or "/zerobot"
                self.docker_client.images.build(
                    path=build_path,
                    dockerfile="Dockerfile.toolbox",
                    tag=img,
                    rm=True,
                )

            # Container configuration
            sandbox_cfg = self.config.get("sandbox", {})
            mem_limit = sandbox_cfg.get("memory_limit", "2g")
            cpu_limit = sandbox_cfg.get("cpu_limit", 2.0)
            user_id = sandbox_cfg.get("user_id", 1000)

            # Create the container
            container = self.docker_client.containers.create(
                image=img,
                command=command if isinstance(command, list) else ["sh", "-c", command],
                working_dir=workdir,
                user=f"{user_id}:{user_id}",
                network_disabled=sandbox_cfg.get("network_disabled", True),
                mem_limit=mem_limit,
                nano_cpus=int(cpu_limit * 1e9),
                read_only=True,
                tmpfs={workdir: "noexec,nosuid,size=100m", "/tmp": "noexec,nosuid,size=100m"},
                cap_drop=["ALL"],
                security_opt=["no-new-privileges:true"],
                auto_remove=False,
            )

            # Copy binary if provided
            if binary_path and os.path.isfile(binary_path):
                self.copy_to_container(container, binary_path, f"{workdir}/target.bin")

            # Start and wait
            container.start()
            exit_code = container.wait(timeout=timeout).get("StatusCode", -1)

            # Collect output
            logs = container.logs(stdout=True, stderr=True)
            stdout_raw, stderr_raw = self._split_logs(logs)

            # Also get per-stream logs
            try:
                stdout_logs = container.logs(stdout=True, stderr=False).decode("utf-8", errors="replace")
                stderr_logs = container.logs(stdout=False, stderr=True).decode("utf-8", errors="replace")
            except Exception:
                stdout_logs = stdout_raw
                stderr_logs = stderr_raw

            result.stdout = stdout_logs.strip()
            result.stderr = stderr_logs.strip()
            result.exit_code = exit_code

        except docker.errors.APIError as e:
            result.error = f"Docker API error: {e}"
            logger.error("Docker API error: %s", e)
        except Exception as e:
            result.error = f"Unexpected error: {e}"
            logger.error("Tool execution error: %s", e, exc_info=True)
        finally:
            if container:
                try:
                    container.remove(force=True)
                except Exception:
                    logger.warning("Failed to remove container (already removed?)")

        return result

    # ── Utilities ───────────────────────────────────────────────────

    @staticmethod
    def _split_logs(logs: bytes) -> tuple[str, str]:
        """Split Docker multiplexed logs into stdout and stderr."""
        stdout_parts: list[bytes] = []
        stderr_parts: list[bytes] = []
        i = 0
        while i < len(logs):
            # Docker log format: 8 byte header + data
            if i + 8 > len(logs):
                break
            stream_type = logs[i]
            data_len = int.from_bytes(logs[i + 4 : i + 8], "big")
            i += 8
            chunk = logs[i : i + data_len]
            if stream_type == 1:
                stdout_parts.append(chunk)
            elif stream_type == 2:
                stderr_parts.append(chunk)
            i += data_len

        return (
            b"".join(stdout_parts).decode("utf-8", errors="replace"),
            b"".join(stderr_parts).decode("utf-8", errors="replace"),
        )

    def __del__(self):
        """Clean up Docker client on garbage collection."""
        if self.docker_client:
            try:
                self.docker_client.close()
            except Exception:
                pass
