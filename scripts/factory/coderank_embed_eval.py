"""Embed an eval database's chunks and the golden queries with CodeRankEmbed (ONNX).

Takes the dense eval database the retrieval harness built (same chunks, same
files), copies it, and overwrites every stored chunk vector with a
CodeRankEmbed vector, so `factory-retrieval-eval --query-vectors` can score
dense and RRF variants where only the retriever differs. Also writes the query
vectors file that flag reads.

Usage:
  python coderank_embed_eval.py --onnx-dir <dir> --src-db <dense-x.db> \
      --out-db <coderank-x.db> --queries <x.all.jsonl> --out-queries <q.jsonl> \
      [--text skeleton|raw] [--max-length 256] [--batch 16] [--threads 4]

--text skeleton embeds the text the backend embeds today
(`chunker::build_embed_text`: symbol + leading doc comment + up to 3 signature
lines); raw embeds the chunk body, which is what CodeRankEmbed was trained on.
Queries get the model's required instruction prefix.
"""

import argparse
import json
import shutil
import sqlite3
import sys
import time

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

QUERY_PREFIX = "Represent this query for searching relevant code: "


def rust_lines(text: str) -> list[str]:
    """`str::lines` semantics: split on \\n, drop one trailing empty line, strip \\r."""
    parts = text.split("\n")
    if parts and parts[-1] == "":
        parts.pop()
    return [p[:-1] if p.endswith("\r") else p for p in parts]


def is_comment_line(line: str) -> bool:
    return line.startswith(("//", "#", "/*", "*", '"""', "'''", "<!--", "--"))


def build_embed_text(symbol: str | None, content: str) -> str:
    """Port of `indexer::chunker::build_embed_text`."""
    doc: list[str] = []
    sig: list[str] = []
    past_doc = False
    for raw in rust_lines(content):
        line = raw.strip()
        if not line:
            if not sig:
                continue
            break
        if not past_doc and is_comment_line(line):
            doc.append(line)
            continue
        past_doc = True
        sig.append(line)
        if "{" in line or len(sig) >= 3:
            break
    parts = ([symbol] if symbol else []) + doc + sig
    text = "\n".join(parts)
    if not text.strip():
        return "\n".join(rust_lines(content)[:3])
    return text


class Encoder:
    def __init__(self, onnx_dir: str, max_length: int, threads: int):
        self.tok = Tokenizer.from_file(f"{onnx_dir}/tokenizer.json")
        self.tok.enable_truncation(max_length)
        self.tok.enable_padding(pad_id=0, pad_token="[PAD]")
        opts = ort.SessionOptions()
        if threads:
            opts.intra_op_num_threads = threads
        self.sess = ort.InferenceSession(
            f"{onnx_dir}/model.onnx", opts, providers=["CPUExecutionProvider"]
        )

    def encode(self, texts: list[str]) -> np.ndarray:
        enc = self.tok.encode_batch(texts)
        ids = np.array([e.ids for e in enc], dtype=np.int64)
        mask = np.array([e.attention_mask for e in enc], dtype=np.int64)
        types = np.zeros_like(ids)
        hidden = self.sess.run(
            None, {"input_ids": ids, "attention_mask": mask, "token_type_ids": types}
        )[0]
        cls = hidden[:, 0, :].astype(np.float32)
        return cls / np.linalg.norm(cls, axis=1, keepdims=True)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--onnx-dir", required=True)
    ap.add_argument("--src-db", required=True)
    ap.add_argument("--out-db", required=True)
    ap.add_argument("--queries", required=True, help="harness output with a query per line")
    ap.add_argument("--out-queries", required=True)
    ap.add_argument("--text", choices=["skeleton", "raw"], default="skeleton")
    ap.add_argument("--max-length", type=int, default=256)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--threads", type=int, default=4)
    ap.add_argument("--limit", type=int, default=0, help="embed only N chunks (benchmark)")
    args = ap.parse_args()

    enc = Encoder(args.onnx_dir, args.max_length, args.threads)

    queries: list[str] = []
    with open(args.queries) as f:
        for line in f:
            q = json.loads(line).get("query")
            if q and q not in queries:
                queries.append(q)
    qv = enc.encode([QUERY_PREFIX + q for q in queries])
    with open(args.out_queries, "w") as f:
        for q, v in zip(queries, qv):
            f.write(json.dumps({"query": q, "vector": v.tolist()}) + "\n")
    print(f"{len(queries)} query vectors -> {args.out_queries}")

    if args.limit == 0:
        shutil.copy(args.src_db, args.out_db)
        db = sqlite3.connect(args.out_db)
    else:
        db = sqlite3.connect(args.src_db)
    rows = db.execute(
        "SELECT id, symbol, content FROM code_chunks WHERE embedding IS NOT NULL ORDER BY id"
    ).fetchall()
    if args.limit:
        rows = rows[: args.limit]
    texts = [
        build_embed_text(sym, content) if args.text == "skeleton" else content
        for _, sym, content in rows
    ]
    started = time.perf_counter()
    done = 0
    for at in range(0, len(rows), args.batch):
        batch_rows = rows[at : at + args.batch]
        vecs = enc.encode(texts[at : at + args.batch])
        if args.limit == 0:
            db.executemany(
                "UPDATE code_chunks SET embedding = ? WHERE id = ?",
                [(v.astype("<f4").tobytes(), r[0]) for v, r in zip(vecs, batch_rows)],
            )
        done += len(batch_rows)
        if done % 1024 < args.batch:
            rate = done / (time.perf_counter() - started)
            print(f"  {done}/{len(rows)} chunks, {rate:.1f}/s", flush=True)
    elapsed = time.perf_counter() - started
    if args.limit == 0:
        db.commit()
    db.close()
    print(json.dumps({
        "chunks": len(rows), "seconds": round(elapsed, 1),
        "chunks_per_s": round(len(rows) / elapsed, 2), "text": args.text,
        "max_length": args.max_length, "batch": args.batch, "threads": args.threads,
    }))
    return 0


if __name__ == "__main__":
    sys.exit(main())
