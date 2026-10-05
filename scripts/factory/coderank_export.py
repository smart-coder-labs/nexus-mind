"""Export nomic-ai/CodeRankEmbed to ONNX and check parity with torch.

CodeRankEmbed ships no ONNX file. This writes one with the same interface as
the nomic-embed-text-v1.5 ONNX that fastembed loads today (inputs input_ids,
attention_mask, token_type_ids; output last_hidden_state), so the backend
could run it through fastembed's user-defined model path with CLS pooling.

Usage: python coderank_export.py <out_dir> [--opset 17]
Needs: torch, transformers (<4.50, the model's remote code), einops,
sentence-transformers, onnx, onnxruntime. Set HF_HOME to keep weights off the
internal disk.
"""

import argparse
import shutil
import sys
from pathlib import Path

import numpy as np
import onnxruntime as ort
import torch
from sentence_transformers import SentenceTransformer
from transformers import AutoModel, AutoTokenizer

MODEL = "nomic-ai/CodeRankEmbed"
QUERY_PREFIX = "Represent this query for searching relevant code: "

SAMPLES = [
    QUERY_PREFIX + "add Nexus executor via OpenShell",
    QUERY_PREFIX + "corregir el total del carrito en el punto de venta",
    "pub fn dense_file_ranking(\nconn: &Connection,\ncode_project_id: i64,",
    "def fact(n):\n if n < 0:\n  raise ValueError\n return 1 if n == 0 else n * fact(n - 1)",
    "export function SaleDetailPage() { return <RefundButton/> }",
    # Long inputs: the rotary cache is traced, so parity must hold past the
    # export's example length (a few tokens) up to and beyond 256 tokens.
    "\n".join(f"const venta_{i} = calcularTotal(carrito_{i}, impuesto_{i});" for i in range(120)),
    " ".join(f"fn handler_{i}(req: Request) -> Response {{ route(req, {i}) }}" for i in range(200)),
]


class LastHidden(torch.nn.Module):
    def __init__(self, model):
        super().__init__()
        self.model = model

    def forward(self, input_ids, attention_mask, token_type_ids):
        out = self.model(
            input_ids=input_ids,
            attention_mask=attention_mask,
            token_type_ids=token_type_ids,
        )
        return out[0]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("out_dir")
    ap.add_argument("--opset", type=int, default=17)
    args = ap.parse_args()
    out = Path(args.out_dir)
    out.mkdir(parents=True, exist_ok=True)

    tok = AutoTokenizer.from_pretrained(MODEL)
    model = AutoModel.from_pretrained(
        MODEL, trust_remote_code=True, safe_serialization=True, add_pooling_layer=False
    ).eval()
    wrapped = LastHidden(model).eval()

    enc = tok(["hello world", "a longer example input"], padding=True, return_tensors="pt")
    onnx_path = out / "model.onnx"
    torch.onnx.export(
        wrapped,
        (enc["input_ids"], enc["attention_mask"], enc["token_type_ids"]),
        str(onnx_path),
        input_names=["input_ids", "attention_mask", "token_type_ids"],
        output_names=["last_hidden_state"],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "seq"},
            "attention_mask": {0: "batch", 1: "seq"},
            "token_type_ids": {0: "batch", 1: "seq"},
            "last_hidden_state": {0: "batch", 1: "seq"},
        },
        opset_version=args.opset,
        do_constant_folding=True,
        dynamo=False,
    )
    snapshot = Path(tok.name_or_path)
    if not snapshot.exists():
        from huggingface_hub import snapshot_download

        snapshot = Path(snapshot_download(MODEL))
    for name in ["tokenizer.json", "tokenizer_config.json", "special_tokens_map.json",
                 "vocab.txt", "config.json"]:
        shutil.copy(snapshot / name, out / name)

    # Parity: sentence-transformers (the reference usage) vs ONNX + CLS pooling,
    # at the backend's max length (256) and at a long length.
    st = SentenceTransformer(MODEL, trust_remote_code=True)
    sess = ort.InferenceSession(str(onnx_path), providers=["CPUExecutionProvider"])
    worst = 1.0
    for max_len in (256, 1024):
        st.max_seq_length = max_len
        ref = st.encode(SAMPLES, convert_to_numpy=True, normalize_embeddings=True)
        enc = tok(SAMPLES, padding=True, truncation=True, max_length=max_len, return_tensors="np")
        hidden = sess.run(None, {k: enc[k].astype(np.int64) for k in
                                 ["input_ids", "attention_mask", "token_type_ids"]})[0]
        cls = hidden[:, 0, :]
        cls /= np.linalg.norm(cls, axis=1, keepdims=True)
        cos = (cls * ref).sum(axis=1)
        worst = min(worst, float(cos.min()))
        print(f"max_len={max_len} padded_len={enc['input_ids'].shape[1]} "
              f"cosine torch-vs-onnx: {[f'{c:.7f}' for c in cos]}")
    size = onnx_path.stat().st_size + sum(
        p.stat().st_size for p in out.glob("model.onnx.data")
    )
    print(f"onnx: {onnx_path} ({size / 1e6:.1f} MB), opset {args.opset}, worst cosine {worst:.6f}")
    return 0 if worst > 0.999 else 1


if __name__ == "__main__":
    sys.exit(main())
