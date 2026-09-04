use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

/// Local embedding service backed by nomic-embed-text-v1.5 (768-dim, ONNX).
///
/// The model is downloaded on first init (~274 MB) and cached in the fastembed
/// default cache directory (`~/.cache/huggingface/hub`).
/// Thread-safe: `TextEmbedding` is `Send + Sync`.
pub struct EmbedService {
    model: TextEmbedding,
}

/// Task instructions nomic-embed-text-v1.5 is trained with. Stored vectors and
/// query vectors must be produced with the matching one — see `embed_query`.
const QUERY_PREFIX: &str = "search_query: ";
const DOCUMENT_PREFIX: &str = "search_document: ";

impl EmbedService {
    /// Initialize the embedding model. Blocks until the model is loaded.
    /// Returns an error if the model cannot be downloaded or loaded.
    pub fn init() -> Result<Self> {
        // Cap the token sequence length. Transformer activation memory scales with
        // batch × seq_len × hidden × layers; nomic-embed accepts up to 8192 tokens,
        // and embedding many code chunks at the default length was the real OOM
        // source (the batch-count cap alone did not bound it). 256 tokens is ample
        // for locating code — the symbol name + signature carry the semantic signal —
        // and keeps peak inference memory well under the container limit.
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::NomicEmbedTextV15).with_max_length(256),
        )?;
        Ok(EmbedService { model })
    }

    /// Embed a search query.
    ///
    /// # Why the prefix
    ///
    /// nomic-embed-text-v1.5 is trained with task instructions, and a query and
    /// the document that answers it get different ones. Feeding both the bare
    /// text still produces usable vectors — which is why this went unnoticed —
    /// but it collapses the query/document asymmetry the model was trained to
    /// exploit.
    ///
    /// Measured on a real pair from the u2s corpus, a deploy question against
    /// the convention that answers it and an unrelated CSS note:
    ///
    /// ```text
    /// bare      correct 0.7243   unrelated 0.6838   margin 0.0405
    /// prefixed  correct 0.7469   unrelated 0.6462   margin 0.1007
    /// ```
    ///
    /// Both orderings are right in isolation; across 2,907 entries a margin of
    /// 0.04 is noise, and the live search answered "how do I stop a push
    /// deploying both apps" with a note about a mobile carousel.
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_one(&format!("{QUERY_PREFIX}{text}"))
    }

    /// Embed one text for storage and later retrieval.
    pub fn embed_document(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_one(&format!("{DOCUMENT_PREFIX}{text}"))
    }

    /// Embed texts for storage. Order and count are preserved.
    pub fn embed_documents(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let prefixed: Vec<String> = texts
            .iter()
            .map(|t| format!("{DOCUMENT_PREFIX}{t}"))
            .collect();
        let refs: Vec<&str> = prefixed.iter().map(String::as_str).collect();
        self.embed_batch(&refs)
    }

    /// Embed a single text verbatim. Private: a caller that picks neither
    /// prefix gets the weaker vectors above, and nothing in the type system
    /// would say so.
    fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut results = self.model.embed(vec![text], None)?;
        results.pop().ok_or_else(|| anyhow::anyhow!("empty embedding result"))
    }

    /// Embed a batch of texts. Returns one vector per input in the same order.
    ///
    /// Texts are fed to the model in sub-batches of at most [`EMBED_BATCH`], so the
    /// ONNX runtime never receives a single huge inference batch (a large file can
    /// yield hundreds of chunks — passing them all at once padded to the longest
    /// chunk spikes memory and was an OOM source). Order and count are preserved.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        embed_in_sub_batches(texts, EMBED_BATCH, |batch| {
            // Passing `Some(batch.len())` keeps fastembed's own batch equal to our
            // sub-batch, so no inference call ever exceeds EMBED_BATCH texts.
            self.model.embed(batch.to_vec(), Some(batch.len()))
        })
    }
}

/// Maximum number of texts handed to a single embedding inference call. Small and
/// constant so peak inference memory is bounded regardless of how many chunks a
/// file produces. Peak memory is bounded by `max_length` (capped at 256 tokens),
/// not by this count, so 16 is safe — 8 was pathologically slow for throughput.
const EMBED_BATCH: usize = 16;

/// Split `texts` into contiguous sub-batches of at most `batch` and concatenate the
/// per-batch results, preserving input order and total count. Generic over the
/// embedding closure so the batching logic is unit-testable without loading a model.
fn embed_in_sub_batches<F>(texts: &[&str], batch: usize, mut embed: F) -> Result<Vec<Vec<f32>>>
where
    F: FnMut(&[&str]) -> Result<Vec<Vec<f32>>>,
{
    let batch = batch.max(1);
    let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
    for chunk in texts.chunks(batch) {
        let mut vecs = embed(chunk)?;
        if vecs.len() != chunk.len() {
            anyhow::bail!(
                "embedding count mismatch: got {} for {} inputs",
                vecs.len(),
                chunk.len()
            );
        }
        out.append(&mut vecs);
    }
    Ok(out)
}

// ── BLOB serialization ────────────────────────────────────────────────────────

/// Serialize a float vector to little-endian bytes for SQLite BLOB storage.
pub fn serialize(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Deserialize little-endian bytes back to a float vector.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn deserialize(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

// ── Similarity ────────────────────────────────────────────────────────────────

/// Cosine similarity in [0, 1]. Returns 0.0 when either vector has zero magnitude.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        0.0
    } else {
        dot / (mag_a * mag_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sub-batching must preserve order and count for N > batch, and must never
    /// hand more than `batch` texts to a single inference call.
    #[test]
    fn embed_sub_batches_preserve_order_and_count() {
        // 50 texts, batch 16 → 4 sub-batches (16,16,16,2).
        let owned: Vec<String> = (0..50).map(|i| format!("t{i}")).collect();
        let texts: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();

        let mut max_seen = 0usize;
        let mut call_count = 0usize;
        // Fake embedder: encodes each text's numeric suffix as a 1-dim vector so we
        // can assert output order maps 1:1 to input order.
        let out = embed_in_sub_batches(&texts, 16, |chunk| {
            max_seen = max_seen.max(chunk.len());
            call_count += 1;
            Ok(chunk
                .iter()
                .map(|t| vec![t.trim_start_matches('t').parse::<f32>().unwrap()])
                .collect())
        })
        .unwrap();

        assert_eq!(out.len(), texts.len(), "count must be preserved");
        assert!(max_seen <= 16, "no inference call may exceed the batch size");
        assert_eq!(call_count, 4, "50 texts / 16 = 4 sub-batches");
        for (i, v) in out.iter().enumerate() {
            assert_eq!(v[0] as usize, i, "order must be preserved at index {i}");
        }
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let v = vec![1.0_f32, 0.5, -0.25, 0.0];
        let bytes = serialize(&v);
        let back = deserialize(&bytes);
        assert_eq!(back.len(), v.len());
        for (a, b) in v.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-7, "roundtrip mismatch: {a} vs {b}");
        }
    }

    #[test]
    fn cosine_identical_vectors_returns_one() {
        let v = vec![1.0_f32, 2.0, 3.0];
        let sim = cosine(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_vectors_returns_zero() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 1.0];
        let sim = cosine(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn cosine_zero_vector_returns_zero() {
        let a = vec![0.0_f32, 0.0];
        let b = vec![1.0_f32, 2.0];
        assert_eq!(cosine(&a, &b), 0.0);
    }
}


#[cfg(test)]
mod prefix_tests {
    use super::*;

    /// The measurement that put the prefixes in, kept as a test so removing
    /// them fails loudly instead of quietly halving retrieval quality.
    ///
    /// Ignored because it downloads the model. Run deliberately:
    /// `cargo test --lib prefix_tests -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn prefixes_widen_the_margin_between_a_right_and_a_wrong_document() {
        let svc = EmbedService::init().expect("model");
        let query = "cómo evito que un push despliegue las dos apps";
        let right = "Sin configuración, cada push despliega ambos proyectos. Pon el \
                     Ignored Build Step de cada proyecto Vercel en `npx turbo-ignore` \
                     para evitar despliegues innecesarios.";
        let wrong = "Categorías: stack vertical de 3 filas en mobile. En mobile (<750px) \
                     reemplaza el carrusel horizontal de tarjetas verticales por un stack \
                     de una sola columna.";

        let q = svc.embed_query(query).unwrap();
        let margin = cosine(&q, &svc.embed_document(right).unwrap())
            - cosine(&q, &svc.embed_document(wrong).unwrap());

        // Bare text measured 0.0405 on this pair; prefixed measured 0.1007.
        // The floor sits between them: it catches a regression to bare text
        // without pinning a model version's exact score.
        assert!(
            margin > 0.07,
            "prefixed retrieval margin collapsed to {margin:.4} — are the task \
             prefixes still applied on both sides?"
        );
    }

    /// The asymmetry itself: the two prefixes must produce different vectors,
    /// or the whole point is lost and only the cost remains.
    #[test]
    #[ignore]
    fn a_query_and_a_document_embed_differently() {
        let svc = EmbedService::init().expect("model");
        let text = "el build usa pnpm, nunca npm";
        assert!(
            cosine(&svc.embed_query(text).unwrap(), &svc.embed_document(text).unwrap()) < 0.999,
            "query and document embeddings of the same text are identical — the \
             prefixes are not reaching the model"
        );
    }
}
