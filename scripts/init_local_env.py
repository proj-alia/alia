#!/usr/bin/env python3
"""Create local-only deployment secrets without displaying them or replacing an existing file."""
import os
from pathlib import Path
import secrets

root = Path(__file__).resolve().parents[1]
path = root / ".env"
fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, "w") as stream:
    stream.write(f"ALIA_DB_PASSWORD={secrets.token_hex(32)}\n")
    stream.write(f"ALIA_INSTANCE_TOKEN={secrets.token_hex(32)}\n")
print("Created private .env; existing files are never replaced.")
