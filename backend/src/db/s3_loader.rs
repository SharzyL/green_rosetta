use anyhow::{anyhow, Context, Result};
use s3::creds::Credentials;
use s3::{Bucket, Region};

use crate::config;
use crate::db::{Album, Database};

fn region_from_config(cfg: &config::S3StorageConfig) -> Result<Region> {
    if !cfg.endpoint.trim().is_empty() {
        return Ok(Region::Custom {
            region: cfg.region.clone(),
            endpoint: cfg.endpoint.clone(),
        });
    }

    cfg.region
        .parse::<Region>()
        .map_err(|e| anyhow!("Invalid storage.s3.region '{}': {}", cfg.region, e))
}

pub async fn load_database_from_s3(
    cfg: &config::S3StorageConfig,
) -> Result<Database> {
    if cfg.bucket.trim().is_empty() {
        return Err(anyhow!("storage.s3.bucket must be non-empty"));
    }
    let access_key_id = cfg
        .access_key_id
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow!("storage.s3.access_key_id is required to list metadata (or set env AWS_ACCESS_KEY_ID)")
        })?;

    // S3 layout matches the local layout: `metadata/*.yaml`.
    let prefix = "metadata/".to_string();

    let secret = cfg.secret_access_key.clone().filter(|s| !s.trim().is_empty()).ok_or_else(|| {
        anyhow!("storage.s3.secret_access_key is required to list metadata (or set env AWS_SECRET_ACCESS_KEY)")
    })?;

    let region = region_from_config(cfg)?;
    let creds = Credentials::new(
        Some(&access_key_id),
        Some(&secret),
        None,
        None,
        None,
    )
    .context("Create S3 credentials")?;

    let bucket = Bucket::new(&cfg.bucket, region, creds).context("Create S3 bucket client")?;

    let listing = bucket
        .list(prefix.clone(), None)
        .await
        .with_context(|| format!("List S3 metadata prefix '{}'", prefix))?;

    let mut albums = Vec::new();
    for page in listing {
        for obj in page.contents {
            let key = obj.key;
            let ext = key
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext != "yaml" && ext != "yml" {
                continue;
            }

            let body = bucket
                .get_object(&key)
                .await
                .with_context(|| format!("Get S3 object {}", key))?;

            let content = String::from_utf8(body.to_vec())
                .with_context(|| format!("Metadata object '{}' is not valid UTF-8", key))?;

            let mut album: Album = serde_yaml::from_str(&content)
                .with_context(|| format!("Parse YAML in '{}'", key))?;

            // Enabled is runtime-only; ignore any persisted value.
            album.enabled = true;
            albums.push(album);
        }
    }

    if albums.is_empty() {
        return Err(anyhow!("No album YAML files found under s3://{}/{}", cfg.bucket, prefix));
    }

    Ok(Database { albums })
}
