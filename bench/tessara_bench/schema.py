"""Minimal JSON-schema checker for the subset our schemas use (plan D8, §16).

Supports: type (incl. ["string","null"]), enum, required, properties, additionalProperties=false,
items, maxItems, maxLength. llama-server already constrains output with a grammar; this is the
second, independent check the plan requires ("code validates again regardless", §7.3).
"""
from __future__ import annotations

_TYPES = {
    "object": dict,
    "array": list,
    "string": str,
    "boolean": bool,
    "null": type(None),
}


def _is_type(value, name: str) -> bool:
    if name == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if name == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    return isinstance(value, _TYPES[name])


def validate(value, schema: dict, path: str = "$") -> list[str]:
    """Return a list of violations (empty = valid)."""
    errs: list[str] = []
    if "enum" in schema and value not in schema["enum"]:
        return [f"{path}: {value!r} not in enum"]
    t = schema.get("type")
    if t is not None:
        names = t if isinstance(t, list) else [t]
        if not any(_is_type(value, n) for n in names):
            return [f"{path}: expected {t}, got {type(value).__name__}"]
    if isinstance(value, str) and "maxLength" in schema and len(value) > schema["maxLength"]:
        errs.append(f"{path}: longer than {schema['maxLength']}")
    if isinstance(value, list):
        if "maxItems" in schema and len(value) > schema["maxItems"]:
            errs.append(f"{path}: more than {schema['maxItems']} items")
        if "items" in schema:
            for i, v in enumerate(value):
                errs += validate(v, schema["items"], f"{path}[{i}]")
    if isinstance(value, dict):
        props = schema.get("properties", {})
        for k in schema.get("required", []):
            if k not in value:
                errs.append(f"{path}: missing {k}")
        if schema.get("additionalProperties") is False:
            for k in value:
                if k not in props:
                    errs.append(f"{path}: unexpected key {k}")
        for k, sub in props.items():
            if k in value:
                errs += validate(value[k], sub, f"{path}.{k}")
    return errs
