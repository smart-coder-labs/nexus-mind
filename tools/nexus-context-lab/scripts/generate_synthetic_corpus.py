#!/usr/bin/env python3
"""Generate a deterministic, isolated synthetic context corpus."""
from __future__ import annotations

import argparse
import hashlib
import json
import random
import sqlite3
import struct
from pathlib import Path
from typing import Dict, List


TENANTS = ("tenant-fable", "tenant-orbit", "tenant-pine")
DIMENSION = 64
TERMS = ("retrieval", "policy", "compiler", "memory", "project", "context", "contract", "tenant")


def _hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _embedding(rng: random.Random, dimension: int = DIMENSION) -> List[float]:
    values = [rng.uniform(-1.0, 1.0) for _ in range(dimension)]
    norm = sum(value * value for value in values) ** 0.5
    return [struct.unpack("<f", struct.pack("<f", value / norm))[0] for value in values]


def generate(output: Path, chunks: int = 10_000, seed: int = 20260802, dimension: int = DIMENSION) -> Dict[str, object]:
    if chunks < 1:
        raise ValueError("chunks must be positive")
    output.mkdir(parents=True, exist_ok=True)
    db_path = output / "corpus.sqlite"
    json_path = output / "corpus.json"
    for path in (db_path, json_path):
        if path.exists():
            path.unlink()

    rng = random.Random(seed)
    records = []
    for index in range(chunks):
        tenant = TENANTS[index % len(TENANTS)]
        project = f"{tenant}-project-{(index // len(TENANTS)) % 2 + 1}"
        term = TERMS[index % len(TERMS)]
        records.append({
            "id": f"chunk-{index:05d}",
            "tenant": tenant,
            "project": project,
            "acl": [f"user-{(index % 6) + 1:02d}"],
            "text": f"Synthetic {term} context for {tenant} in {project}; chunk {index:05d}.",
            "metadata": {"source": "synthetic", "ordinal": index, "language": "en"},
            "embedding": _embedding(rng, dimension),
        })

    connection = sqlite3.connect(str(db_path))
    try:
        connection.execute("PRAGMA journal_mode=DELETE")
        connection.execute("PRAGMA synchronous=OFF")
        connection.executescript("""
            CREATE TABLE chunks (
                id TEXT PRIMARY KEY, tenant TEXT NOT NULL, project TEXT NOT NULL,
                text TEXT NOT NULL, embedding BLOB NOT NULL, metadata_json TEXT NOT NULL
            );
            CREATE TABLE acl (chunk_id TEXT NOT NULL, principal TEXT NOT NULL, PRIMARY KEY(chunk_id, principal));
            CREATE TABLE projects (tenant TEXT NOT NULL, project TEXT NOT NULL, PRIMARY KEY(tenant, project));
        """)
        try:
            connection.execute("CREATE VIRTUAL TABLE chunks_fts USING fts5(id UNINDEXED, tenant, project, text)")
        except sqlite3.OperationalError:
            connection.execute("CREATE TABLE chunks_fts (id TEXT, tenant TEXT, project TEXT, text TEXT)")
        for record in records:
            blob = struct.pack(f"<{dimension}f", *record["embedding"])
            connection.execute("INSERT INTO chunks VALUES (?, ?, ?, ?, ?, ?)", (
                record["id"], record["tenant"], record["project"], record["text"], blob,
                json.dumps(record["metadata"], sort_keys=True, separators=(",", ":")),
            ))
            connection.execute("INSERT INTO acl VALUES (?, ?)", (record["id"], record["acl"][0]))
            connection.execute("INSERT OR IGNORE INTO projects VALUES (?, ?)", (record["tenant"], record["project"]))
            connection.execute("INSERT INTO chunks_fts VALUES (?, ?, ?, ?)", (record["id"], record["tenant"], record["project"], record["text"]))
        connection.commit()
    finally:
        connection.close()

    json_path.write_text(json.dumps({
        "schema": "nexus-synthetic-corpus-v1", "seed": seed, "chunks": chunks,
        "dimension": dimension, "tenants": list(TENANTS), "records": records,
    }, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    files = {"sqlite": {"path": db_path.name, "bytes": db_path.stat().st_size, "sha256": _hash_file(db_path)},
             "json": {"path": json_path.name, "bytes": json_path.stat().st_size, "sha256": _hash_file(json_path)}}
    manifest = {"schema": "nexus-synthetic-corpus-v1", "synthetic_corpus": True, "seed": seed,
                "chunks": chunks, "dimension": dimension, "tenants": list(TENANTS), "files": files}
    manifest["corpus_sha256"] = hashlib.sha256(json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate an isolated deterministic synthetic corpus")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--chunks", type=int, default=10_000)
    parser.add_argument("--seed", type=int, default=20260802)
    parser.add_argument("--dimension", type=int, default=DIMENSION)
    args = parser.parse_args()
    print(json.dumps(generate(args.output, args.chunks, args.seed, args.dimension), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
