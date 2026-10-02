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
        rows = connection.execute("SELECT * FROM translation_cache ORDER BY id").fetchall()
        favorites = connection.execute("SELECT COUNT(*) FROM translation_cache WHERE favorite = 1").fetchone()[0]
        digest = hashlib.sha256(json.dumps(rows, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
        return {"rows": len(rows), "favorites": favorites, "digest": digest, "integrity": integrity}


before = audit(sys.argv[1])
after = audit(sys.argv[2])
if before != after:
    raise SystemExit("History content changed during upgrade; preserve both files for review")
print(json.dumps({"preserved": True, **after}))
