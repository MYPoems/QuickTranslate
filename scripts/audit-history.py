"""Read-only migration audit: print counts/digests, never history text or credentials."""
import hashlib
import json
import sqlite3
import sys
from pathlib import Path


def audit(path: str) -> dict:
    uri = Path(path).resolve().as_uri() + "?mode=ro"
    with sqlite3.connect(uri, uri=True) as connection:
        connection.execute("PRAGMA query_only=ON")
        integrity = connection.execute("PRAGMA integrity_check").fetchone()[0]
        if integrity != "ok":
            raise RuntimeError("Database integrity check failed")
        # Since the history got its own table it is the one to compare; older databases
        # keep history and cache together in translation_cache.
        split = connection.execute(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'translation_history'"
        ).fetchone()[0]
        table = "translation_history" if split else "translation_cache"
        rows = connection.execute(f"SELECT * FROM {table} ORDER BY id").fetchall()
        favorites = connection.execute(f"SELECT COUNT(*) FROM {table} WHERE favorite = 1").fetchone()[0]
        digest = hashlib.sha256(json.dumps(rows, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
        return {"rows": len(rows), "favorites": favorites, "digest": digest, "integrity": integrity}


before = audit(sys.argv[1])
after = audit(sys.argv[2])
if before != after:
    raise SystemExit("History content changed during upgrade; preserve both files for review")
print(json.dumps({"preserved": True, **after}))
