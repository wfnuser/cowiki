//! Pluggable dense text embedding providers for CoWiki's derived knowledge index.
//!
//! CoWiki stays 100% local-first by default via [`LocalHashProjectionEmbedder`],
//! which produces deterministic, stateless L2-normalized `f32` vectors using
//! signed feature hashing over words, word bigrams, subword character shingles,
//! and CJK n-grams (so incremental single-page indexing is bit-identical to a
//! full rebuild). When configured via environment variables or explicit config
//! (with the `remote-embeddings` feature enabled), [`OpenAiCompatibleEmbedder`]
//! seamlessly upgrades retrieval to any OpenAI-compatible `/v1/embeddings` endpoint
//! backed by CoWiki's content-addressed SQLite embedding cache.

use std::collections::HashMap;

pub const DEFAULT_LOCAL_EMBEDDING_DIM: usize = 128;
pub const LOCAL_EMBEDDING_MODEL: &str = "local-hash-proj-v1";

pub trait EmbeddingProvider: Send + Sync {
    fn model_name(&self) -> &str;
    fn dimension(&self) -> usize;
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;

    fn embed_one(&self, text: &str) -> Result<Vec<f32>, String> {
        let mut batch = self.embed_batch(&[text.to_string()])?;
        batch
            .pop()
            .ok_or_else(|| "embedding provider returned an empty batch".to_string())
    }
}

/// Deterministic, zero-dependency local dense embedder using signed random
/// feature hashing across word unigrams, word bigrams, character 4-gram
/// shingles, and CJK unigrams/bigrams.
#[derive(Debug, Clone)]
pub struct LocalHashProjectionEmbedder {
    dim: usize,
    model_name: String,
}

impl Default for LocalHashProjectionEmbedder {
    fn default() -> Self {
        Self::new(DEFAULT_LOCAL_EMBEDDING_DIM)
    }
}

impl LocalHashProjectionEmbedder {
    pub fn new(dim: usize) -> Self {
        let dim = dim.clamp(32, 1024);
        let model_name = if dim == DEFAULT_LOCAL_EMBEDDING_DIM {
            LOCAL_EMBEDDING_MODEL.to_string()
        } else {
            format!("{LOCAL_EMBEDDING_MODEL}-d{dim}")
        };
        Self { dim, model_name }
    }

    fn embed_single(&self, text: &str) -> Vec<f32> {
        let mut counts: HashMap<u64, f32> = HashMap::new();
        let mut words: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut cjk_run: Vec<char> = Vec::new();

        let flush_word = |word: &mut String, words: &mut Vec<String>| {
            if !word.is_empty() {
                words.push(std::mem::take(word));
            }
        };
        let flush_cjk = |run: &mut Vec<char>, counts: &mut HashMap<u64, f32>| {
            for &ch in run.iter() {
                let h = fnv1a64(0xC001_0001, ch.encode_utf8(&mut [0; 4]).as_bytes());
                *counts.entry(h).or_insert(0.0) += 0.9;
            }
            for pair in run.windows(2) {
                let mut buf = [0u8; 8];
                let n0 = pair[0].encode_utf8(&mut buf[..4]).len();
                let n1 = pair[1].encode_utf8(&mut buf[n0..n0 + 4]).len();
                let h = fnv1a64(0xC001_0002, &buf[..n0 + n1]);
                *counts.entry(h).or_insert(0.0) += 1.25;
            }
            run.clear();
        };

        for ch in text.chars() {
            if is_cjk(ch) {
                flush_word(&mut current, &mut words);
                cjk_run.push(ch);
            } else if ch.is_alphanumeric() {
                flush_cjk(&mut cjk_run, &mut counts);
                for lower in ch.to_lowercase() {
                    current.push(lower);
                }
            } else {
                flush_cjk(&mut cjk_run, &mut counts);
                flush_word(&mut current, &mut words);
            }
        }
        flush_cjk(&mut cjk_run, &mut counts);
        flush_word(&mut current, &mut words);

        let filtered: Vec<&str> = words
            .iter()
            .map(String::as_str)
            .filter(|w| w.chars().count() >= 2 && !is_stopword(w))
            .collect();

        for &word in &filtered {
            // Exact word unigram (strongest semantic weight).
            let h = fnv1a64(0xA11C_E001, word.as_bytes());
            *counts.entry(h).or_insert(0.0) += 1.2;

            // Prefix root (captures basic English morphological variants like rotate/rotation).
            let chars: Vec<char> = word.chars().collect();
            if chars.len() >= 5 {
                let stem_len = (chars.len().saturating_sub(2)).clamp(4, 7);
                let stem: String = chars[..stem_len].iter().collect();
                let hs = fnv1a64(0xA11C_E004, stem.as_bytes());
                *counts.entry(hs).or_insert(0.0) += 0.65;
            }

            // Character 4-gram shingles (catches compound identifiers & minor inflections).
            if chars.len() >= 4 {
                let padded: Vec<char> = std::iter::once('^')
                    .chain(chars.iter().copied())
                    .chain(std::iter::once('$'))
                    .collect();
                for gram in padded.windows(4).take(16) {
                    let s: String = gram.iter().collect();
                    let hg = fnv1a64(0xA11C_E002, s.as_bytes());
                    *counts.entry(hg).or_insert(0.0) += 0.35;
                }
            }
        }

        // Adjacent word bigrams capture multi-word concepts and entity phrases.
        for pair in filtered.windows(2) {
            let phrase = format!("{} {}", pair[0], pair[1]);
            let hb = fnv1a64(0xA11C_E003, phrase.as_bytes());
            *counts.entry(hb).or_insert(0.0) += 0.9;
        }

        let mut vec = vec![0.0_f32; self.dim];
        for (hash, raw_weight) in counts {
            // Sublinear TF dampening avoids one repeated token dominating the vector.
            let weight = 1.0 + raw_weight.ln_1p();
            let bucket = (hash as usize) % self.dim;
            let sign = if (hash >> 63) == 0 { 1.0_f32 } else { -1.0_f32 };
            vec[bucket] += sign * weight;
            // Secondary orthogonal projection reduces hash collision variance.
            let h2 = splitmix64(hash);
            let bucket2 = (h2 as usize) % self.dim;
            let sign2 = if (h2 >> 63) == 0 { 0.5_f32 } else { -0.5_f32 };
            vec[bucket2] += sign2 * weight;
        }

        l2_normalize(&mut vec);
        vec
    }
}

impl EmbeddingProvider for LocalHashProjectionEmbedder {
    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn dimension(&self) -> usize {
        self.dim
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Ok(texts.iter().map(|text| self.embed_single(text)).collect())
    }
}

/// Optional remote OpenAI-compatible `/v1/embeddings` client (works with OpenAI,
/// DashScope/Qwen, Ollama, vLLM, and LiteLLM proxies).
#[cfg(feature = "remote-embeddings")]
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleEmbedder {
    endpoint: String,
    api_key: String,
    model: String,
    dimensions: Option<usize>,
    batch_size: usize,
    client: reqwest::blocking::Client,
}

#[cfg(feature = "remote-embeddings")]
impl OpenAiCompatibleEmbedder {
    pub fn new(
        endpoint: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        dimensions: Option<usize>,
    ) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| format!("cannot initialize embedding HTTP client: {error}"))?;
        Ok(Self {
            endpoint: endpoint.into(),
            api_key: api_key.into(),
            model: model.into(),
            dimensions,
            batch_size: 16,
            client,
        })
    }

    /// Build a remote embedder when `COWIKI_EMBEDDING_URL`,
    /// `COWIKI_EMBEDDING_API_KEY`, and `COWIKI_EMBEDDING_MODEL` are set.
    pub fn from_env() -> Option<Self> {
        let endpoint = std::env::var("COWIKI_EMBEDDING_URL").ok()?;
        let api_key = std::env::var("COWIKI_EMBEDDING_API_KEY").ok()?;
        let model = std::env::var("COWIKI_EMBEDDING_MODEL").ok()?;
        if endpoint.trim().is_empty() || api_key.trim().is_empty() || model.trim().is_empty() {
            return None;
        }
        let dimensions = std::env::var("COWIKI_EMBEDDING_DIM")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&d| d > 0);
        Self::new(endpoint.trim(), api_key.trim(), model.trim(), dimensions).ok()
    }
}

#[cfg(feature = "remote-embeddings")]
impl EmbeddingProvider for OpenAiCompatibleEmbedder {
    fn model_name(&self) -> &str {
        &self.model
    }

    fn dimension(&self) -> usize {
        self.dimensions.unwrap_or(0)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut all_vectors = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(self.batch_size.max(1)) {
            let mut body = serde_json::json!({
                "model": self.model,
                "input": chunk,
            });
            if let Some(dim) = self.dimensions {
                body["dimensions"] = serde_json::json!(dim);
            }
            let response = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .map_err(|error| format!("embedding request failed: {error}"))?;
            let status = response.status();
            if !status.is_success() {
                let err_body = response.text().unwrap_or_default();
                return Err(format!(
                    "embedding endpoint returned HTTP {status}: {}",
                    err_body.chars().take(240).collect::<String>()
                ));
            }
            let parsed: serde_json::Value = response
                .json()
                .map_err(|error| format!("invalid embedding JSON response: {error}"))?;
            let items = parsed
                .get("data")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| "embedding response missing 'data' array".to_string())?;
            let mut indexed: Vec<(usize, Vec<f32>)> = Vec::with_capacity(items.len());
            for (fallback_idx, item) in items.iter().enumerate() {
                let idx = item
                    .get("index")
                    .and_then(serde_json::Value::as_u64)
                    .map(|v| v as usize)
                    .unwrap_or(fallback_idx);
                let raw = item
                    .get("embedding")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| "embedding item missing 'embedding' array".to_string())?;
                let mut vec: Vec<f32> = raw
                    .iter()
                    .map(|val| val.as_f64().unwrap_or(0.0) as f32)
                    .collect();
                l2_normalize(&mut vec);
                indexed.push((idx, vec));
            }
            indexed.sort_by_key(|(idx, _)| *idx);
            if indexed.len() != chunk.len() {
                return Err(format!(
                    "embedding response count mismatch: expected {}, got {}",
                    chunk.len(),
                    indexed.len()
                ));
            }
            all_vectors.extend(indexed.into_iter().map(|(_, vec)| vec));
        }
        Ok(all_vectors)
    }
}

/// Returns the configured remote embedding provider when environment variables
/// are present, otherwise falls back to the zero-key local projection embedder.
pub fn default_embedder() -> Box<dyn EmbeddingProvider> {
    #[cfg(feature = "remote-embeddings")]
    if let Some(remote) = OpenAiCompatibleEmbedder::from_env() {
        return Box::new(remote);
    }
    Box::new(LocalHashProjectionEmbedder::default())
}

pub fn encode_vector(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for &value in vec {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub fn decode_vector(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut norm_l = 0.0_f32;
    let mut norm_r = 0.0_f32;
    for (&a, &b) in left.iter().zip(right.iter()) {
        dot += a * b;
        norm_l += a * a;
        norm_r += b * b;
    }
    let denom = (norm_l * norm_r).sqrt();
    if denom <= 1e-12 {
        0.0
    } else {
        (dot / denom).clamp(-1.0, 1.0)
    }
}

pub fn l2_normalize(vec: &mut [f32]) {
    let norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 1e-12 {
        for value in vec.iter_mut() {
            *value /= norm;
        }
    }
}

fn fnv1a64(seed: u64, bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn is_cjk(ch: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&ch)
        || ('\u{3400}'..='\u{4DBF}').contains(&ch)
        || ('\u{3040}'..='\u{30FF}').contains(&ch)
        || ('\u{AC00}'..='\u{D7AF}').contains(&ch)
}

fn is_stopword(word: &str) -> bool {
    matches!(
        word,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "been"
            | "by"
            | "can"
            | "did"
            | "do"
            | "does"
            | "for"
            | "from"
            | "had"
            | "has"
            | "have"
            | "how"
            | "in"
            | "into"
            | "is"
            | "it"
            | "its"
            | "of"
            | "on"
            | "or"
            | "that"
            | "the"
            | "their"
            | "them"
            | "there"
            | "these"
            | "they"
            | "this"
            | "to"
            | "was"
            | "were"
            | "what"
            | "when"
            | "where"
            | "which"
            | "who"
            | "why"
            | "with"
            | "would"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_hash_embedder_is_deterministic_and_captures_morphological_similarity() {
        let embedder = LocalHashProjectionEmbedder::default();
        let a = embedder
            .embed_one("Rotating master HSM signing keys every 90 days")
            .unwrap();
        let a2 = embedder
            .embed_one("Rotating master HSM signing keys every 90 days")
            .unwrap();
        let similar = embedder
            .embed_one("Key rotation cadence for hardware signing custody")
            .unwrap();
        let unrelated = embedder
            .embed_one("Sourdough fermentation temperature and hydration ratio")
            .unwrap();

        assert_eq!(a, a2);
        assert_eq!(a.len(), DEFAULT_LOCAL_EMBEDDING_DIM);
        let sim_close = cosine_similarity(&a, &similar);
        let sim_far = cosine_similarity(&a, &unrelated);
        assert!(
            sim_close > sim_far + 0.15,
            "expected semantic/morphological match ({sim_close}) >> unrelated ({sim_far})"
        );
    }

    #[test]
    fn vector_encode_decode_round_trips_losslessly() {
        let v = vec![0.5_f32, -0.25, 0.0, 0.825];
        let encoded = encode_vector(&v);
        assert_eq!(decode_vector(&encoded), v);
    }
}
