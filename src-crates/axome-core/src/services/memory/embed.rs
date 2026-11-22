use std::path::Path;

use anyhow::{ensure, Context, Result};
use axome_memory::index::EMBED_DIM;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use sha2::{Digest, Sha256};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};
use tokio::{io::AsyncWriteExt, sync::watch};

const MODEL_URL: &str = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/5c38ec7c405ec4b44b94cc5a9bb96e735b38267a";
const WEIGHTS_SHA256: &str = "3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad";
const FILES: [(&str, u64); 3] = [
    ("config.json", 743),
    ("tokenizer.json", 711_396),
    ("model.safetensors", 133_466_304),
];
const QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";

#[derive(Clone, Debug, PartialEq)]
pub enum EmbedState {
    Disabled,
    Missing,
    Downloading { received: u64, total: u64 },
    Ready,
    Failed(String),
}

fn complete_file(dir: &Path, name: &str, size: u64) -> bool {
    std::fs::metadata(dir.join(name))
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == size)
}

// Final filenames are published only after download verification.
pub fn files_complete(dir: &Path) -> bool {
    FILES
        .iter()
        .all(|(name, size)| complete_file(dir, name, *size))
}

pub async fn download(
    client: &reqwest::Client,
    dir: &Path,
    progress: &watch::Sender<EmbedState>,
) -> Result<()> {
    let result = async {
        tokio::fs::create_dir_all(dir).await?;
        let total = FILES.iter().map(|(_, size)| size).sum();
        let mut received = FILES
            .iter()
            .filter(|(name, size)| complete_file(dir, name, *size))
            .map(|(_, size)| size)
            .sum();
        progress.send_replace(EmbedState::Downloading { received, total });
        for (name, size) in FILES {
            if complete_file(dir, name, size) {
                continue;
            }
            let part = dir.join(format!("{name}.part"));
            let result = async {
                let mut response = client
                    .get(format!("{MODEL_URL}/{name}"))
                    .send()
                    .await?
                    .error_for_status()?;
                let mut file = tokio::fs::File::create(&part).await?;
                let mut downloaded = 0;
                let mut hash = Sha256::new();
                while let Some(bytes) = response.chunk().await? {
                    downloaded += bytes.len() as u64;
                    ensure!(downloaded <= size, "{name}: download exceeds expected size");
                    file.write_all(&bytes).await?;
                    if name == "model.safetensors" {
                        hash.update(&bytes);
                    }
                    received += bytes.len() as u64;
                    progress.send_replace(EmbedState::Downloading { received, total });
                }
                ensure!(
                    downloaded == size,
                    "{name}: expected {size} bytes, got {downloaded}"
                );
                if name == "model.safetensors" {
                    ensure!(
                        format!("{:x}", hash.finalize()) == WEIGHTS_SHA256,
                        "{name}: SHA-256 mismatch"
                    );
                }
                file.flush().await?;
                file.sync_all().await?;
                drop(file);
                tokio::fs::rename(&part, dir.join(name)).await?;
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if let Err(error) = result {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(error).with_context(|| format!("Downloading embedding file {name}"));
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = &result {
        progress.send_replace(EmbedState::Failed(format!("{error:#}")));
    }
    result
}

pub struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
}

impl Embedder {
    pub fn load(dir: &Path) -> Result<Self> {
        ensure!(files_complete(dir), "Embedding model files are incomplete");
        let config: Config =
            serde_json::from_reader(std::fs::File::open(dir.join("config.json"))?)?;
        ensure!(
            config.hidden_size == EMBED_DIM,
            "Unexpected embedding dimension"
        );
        let mut tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: 512,
                ..Default::default()
            }))
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            pad_id: config.pad_token_id as u32,
            ..Default::default()
        }));
        // Downloads replace files atomically; never mutate a mapped weight file in place.
        let weights = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[dir.join("model.safetensors")],
                DType::F32,
                &Device::Cpu,
            )?
        };
        let model = BertModel::load(weights, &config)?;
        Ok(Self { model, tokenizer })
    }

    pub fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut output = Vec::with_capacity(texts.len());
        for batch in texts.chunks(16) {
            let encodings = self
                .tokenizer
                .encode_batch(batch.iter().map(String::as_str).collect(), true)
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            let length = encodings[0].len();
            let shape = (batch.len(), length);
            let tensor = |values: Vec<u32>| Tensor::from_vec(values, shape, &Device::Cpu);
            let ids = tensor(
                encodings
                    .iter()
                    .flat_map(|e| e.get_ids().iter().copied())
                    .collect(),
            )?;
            let types = tensor(
                encodings
                    .iter()
                    .flat_map(|e| e.get_type_ids().iter().copied())
                    .collect(),
            )?;
            let mask = tensor(
                encodings
                    .iter()
                    .flat_map(|e| e.get_attention_mask().iter().copied())
                    .collect(),
            )?;
            let hidden = self.model.forward(&ids, &types, Some(&mask))?;
            let mut vectors = hidden.narrow(1, 0, 1)?.squeeze(1)?.to_vec2::<f32>()?;
            for vector in &mut vectors {
                let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
                ensure!(norm.is_finite() && norm > 0.0, "Invalid embedding norm");
                for value in vector {
                    *value /= norm;
                }
            }
            output.extend(vectors);
        }
        Ok(output)
    }

    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let mut vectors = self.embed_passages(&[format!("{QUERY_PREFIX}{text}")])?;
        Ok(vectors.remove(0))
    }
}
