"""Read-only vocabulary snapshot audit; never print words, contexts or answers."""
import hashlib
import json
import sqlite3
import sys
from pathlib import Path


def audit(path: str) -> dict:
    with sqlite3.connect(Path(path).resolve().as_uri() + "?mode=ro", uri=True) as connection:
        connection.execute("PRAGMA query_only=ON")
        integrity = connection.execute("PRAGMA integrity_check").fetchone()[0]
        if integrity != "ok":
            raise RuntimeError("Vocabulary integrity check failed")
        counts, content = {}, {}
        for name in ["words", "aliases", "events", "quizzes", "generation_cache"]:
            rows = connection.execute(f"SELECT * FROM {name} ORDER BY 1").fetchall()
            counts[name] = len(rows)
            content[name] = rows
        content["rules"] = connection.execute("SELECT * FROM metadata WHERE key='rules'").fetchall()
        digest = hashlib.sha256(json.dumps(content, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()
        return {"schema": connection.execute("PRAGMA user_version").fetchone()[0], "integrity": integrity, "counts": counts, "digest": digest}


result = audit(sys.argv[-1])
if len(sys.argv) > 2 and audit(sys.argv[1]) != result:
    raise SystemExit("Vocabulary content changed; preserve both snapshots for review")
print(json.dumps({"preserved": len(sys.argv) > 2, **result}))
