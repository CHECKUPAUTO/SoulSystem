"""Natural-language → typed intent resolver.

``resolve`` accepts a free-form description and a target Pydantic model,
runs a lightweight resolver agent, and returns a populated instance.
The resolver sees the JSON schema of the target model so it knows exactly
which fields are valid.

This pattern is adapted directly from QuantMind's ``magic.py``.
"""

from __future__ import annotations

import json
from typing import Any, TypeVar

from openai import OpenAI
from pydantic import BaseModel

T = TypeVar("T", bound=BaseModel)

_RESOLVER_TEMPLATE = """\
You are a structured intent resolver. Given a natural-language description,
produce a JSON object that matches the target schema exactly.

Rules:
- Set fields conservatively. Leave unspecified fields at their defaults.
- Never invent values — if something is not stated, leave it empty.
- Use the exact field names and types from the schema.

Target schema:
{schema}
"""


async def resolve(
    text: str,
    target_type: type[T],
    *,
    model: str = "gpt-4o-mini",
    base_url: str | None = None,
    api_key: str | None = None,
    extra_instructions: str | None = None,
) -> T:
    """Parse ``text`` into a typed ``T``.

    Example::

        from soulmind.magic import resolve
        from soulmind.knowledge import SourceRef

        src = await resolve(
            "arXiv paper 2401.12345 about momentum factors",
            SourceRef,
        )
        # → SourceRef(kind="arxiv", uri="2401.12345", ...)
    """
    client = _client(base_url, api_key)
    schema_str = _schema(target_type, extra_instructions)
    instructions = _RESOLVER_TEMPLATE.format(schema=schema_str)

    response = client.chat.completions.create(
        model=model,
        messages=[
            {"role": "system", "content": instructions},
            {"role": "user", "content": text},
        ],
        temperature=0,
        response_format={"type": "json_object"},
    )
    raw = response.choices[0].message.content
    if not raw:
        raise ValueError("Resolver returned empty response")
    return target_type.model_validate_json(raw)


def resolve_sync(
    text: str,
    target_type: type[T],
    *,
    model: str = "gpt-4o-mini",
    base_url: str | None = None,
    api_key: str | None = None,
    extra_instructions: str | None = None,
) -> T:
    """Synchronous convenience wrapper."""
    import asyncio

    import nest_asyncio

    nest_asyncio.apply()
    return asyncio.get_event_loop().run_until_complete(
        resolve(text, target_type, model=model, base_url=base_url, api_key=api_key, extra_instructions=extra_instructions)
    )


# ── Internal helpers ────────────────────────────────────────────────

def _client(base_url: str | None, api_key: str | None) -> OpenAI:
    """Build an OpenAI-compatible client respecting env vars."""
    import os

    return OpenAI(
        base_url=base_url or os.environ.get("OPENAI_BASE_URL", "http://localhost:11434/v1"),
        api_key=api_key or os.environ.get("OPENAI_API_KEY", "ollama"),
    )


def _schema(target_type: type[BaseModel], extra: str | None) -> str:
    """Render the target model's JSON schema."""
    try:
        schema = json.dumps(target_type.model_json_schema(), indent=2)
    except Exception:
        fields = {
            name: repr(f.annotation)
            for name, f in target_type.model_fields.items()
        }
        schema = json.dumps(
            {"title": target_type.__name__, "fields": fields}, indent=2
        )
    if extra:
        schema = f"{schema}\n\nExtra instructions:\n{extra}"
    return schema
