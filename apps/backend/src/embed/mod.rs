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

impl EmbedService {
    /// Initialize the embedding model. Blocks until the model is loaded.
    /// Returns an error if the model cannot be downloaded or loaded.
    pub fn init() -> Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::NomicEmbedTextV15),
        )?;
        Ok(EmbedService { model })
    }

    /// Embed a single text. Returns a 768-dimensional vector.
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let mut results = self.model.embed(vec![text], None)?;
        results.pop().ok_or_else(|| anyhow::anyhow!("empty embedding result"))
    }

    /// Embed a batch of texts. Returns one vector per input in the same order.
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        self.model.embed(texts.to_vec(), None)
    }
}

// ── BLOB serialization ────────────────────────────────────────────────────────

/// Serialize a float vector to little-endian bytes for SQLite BLOB storage.
pub fn serialize(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Deserialize little-endian bytes back to a float vector.
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
