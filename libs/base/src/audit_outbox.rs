use anyhow::{bail, ensure, Context, Result};
use hbb_common::uuid::Uuid;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub url: String,
    pub value: Value,
}

pub struct Outbox {
    directory: PathBuf,
}

/// Credential-bearing audit requests may use HTTP only to a literal loopback IP.
pub fn is_loopback_http(value: &str) -> Result<bool> {
    let parsed = url::Url::parse(value)?;
    ensure!(parsed.username().is_empty() && parsed.password().is_none(), "Audit URLs must not contain credentials");
    ensure!(parsed.has_host(), "Audit URL must have a host");
    ensure!(parsed.query().is_none() && parsed.fragment().is_none(), "Audit URLs must not contain query credentials or fragments");
    if parsed.scheme() == "https" { return Ok(false); }
    let local = match parsed.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    ensure!(parsed.scheme() == "http" && local, "Audit transport requires HTTPS except literal loopback addresses");
    Ok(true)
}

pub fn file_events(event: Value, info: Value, files: &[(String, i64)]) -> Result<Vec<Value>> {
    ensure!(event.is_object() && info.is_object(), "Audit event and metadata must be objects");
    let batch_count = ((files.len() + 9) / 10).max(1);
    let batch_id = Uuid::new_v4().to_string();
    let mut events = Vec::with_capacity(batch_count);
    for index in 0..batch_count {
        let start = index * 10;
        let end = (start + 10).min(files.len());
        let mut part = event.clone();
        let mut metadata = info.clone();
        metadata["files"] = serde_json::json!(&files[start..end]);
        metadata["num"] = serde_json::json!(end - start);
        metadata["total_num"] = serde_json::json!(files.len());
        metadata["batch_id"] = serde_json::json!(batch_id);
        metadata["batch_index"] = serde_json::json!(index);
        metadata["batch_count"] = serde_json::json!(batch_count);
        part["nonce"] = serde_json::json!(Uuid::new_v4().to_string());
        part["info"] = Value::String(metadata.to_string());
        events.push(part);
    }
    Ok(events)
}

impl Outbox {
    pub fn new(directory: PathBuf) -> Self { Self { directory } }

    pub async fn enqueue(&self, url: String, mut value: Value) -> Result<PathBuf> {
        is_loopback_http(&url)?;
        let nonce = value.get("nonce").and_then(Value::as_str).context("Missing audit nonce")?;
        let nonce = Uuid::parse_str(nonce)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
        let fields = value.as_object_mut().context("Audit payload must be an object")?;
        fields.entry("occurred_at_ms").or_insert_with(|| serde_json::json!(now.as_millis() as u64));
        let bytes = serde_json::to_vec(&Record { url, value })?;
        ensure!(bytes.len() <= 1024 * 1024, "Audit record exceeds the receiver size limit");
        tokio::fs::create_dir_all(&self.directory).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&self.directory, std::fs::Permissions::from_mode(0o700)).await?;
        }
        let stamp = now.as_nanos();
        let path = self.directory.join(format!("{stamp:020}-{nonce}.json"));
        let temp = path.with_extension("tmp");
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temp).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temp, &path).await?;
        Ok(path)
    }

    pub async fn pending(&self) -> Result<Vec<PathBuf>> {
        let mut entries = match tokio::fs::read_dir(&self.directory).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut paths = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() && entry.path().extension().is_some_and(|e| e == "json") {
                paths.push(entry.path());
            }
        }
        paths.sort();
        Ok(paths)
    }

    pub async fn load(&self, path: &Path) -> Result<Record> {
        self.check_path(path)?;
        let metadata = tokio::fs::symlink_metadata(path).await?;
        ensure!(metadata.is_file() && !metadata.file_type().is_symlink(), "Audit record must be a regular file");
        ensure!(metadata.len() <= 1024 * 1024, "Oversized audit record");
        let record: Record = serde_json::from_slice(&tokio::fs::read(path).await?)?;
        is_loopback_http(&record.url)?;
        Ok(record)
    }

    pub async fn flush<F, Fut>(&self, destinations: &[String], mut send: F) -> Result<usize>
    where
        F: FnMut(Record) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let mut deferred = 0;
        for path in self.pending().await? {
            let record = match self.load(&path).await {
                Ok(record) => record,
                Err(_) => { deferred += 1; continue; }
            };
            if !destinations.contains(&record.url) || send(record).await.is_err() {
                deferred += 1;
                continue;
            }
            self.acknowledge(&path).await?;
        }
        Ok(deferred)
    }

    pub async fn acknowledge(&self, path: &Path) -> Result<()> {
        self.check_path(path)?;
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn check_path(&self, path: &Path) -> Result<()> {
        if path.parent() != Some(self.directory.as_path()) || path.extension().map_or(true, |e| e != "json") {
            bail!("Audit path is outside the outbox");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audit_transport_rejects_remote_plaintext_and_url_credentials() {
        for value in ["http://example.com/api/audit/conn", "http://localhost/api/audit/conn", "file:///tmp/event"] {
            assert!(is_loopback_http(value).is_err(), "{}", value);
        }
        for value in ["http://127.0.0.1:21114/api/audit/conn", "http://[::1]/api/audit/conn"] {
            assert!(is_loopback_http(value).unwrap());
        }
        assert!(!is_loopback_http("https://example.com/api/audit/conn").unwrap());
        let mut credential_url = url::Url::parse("https://example.com").unwrap();
        credential_url.set_username("unit").unwrap();
        assert!(is_loopback_http(credential_url.as_str()).is_err());
    }

    #[test]
    fn file_audit_batches_keep_all_names_and_unique_nonces() {
        let files: Vec<_> = (0..23).map(|i| (format!("file-{i}"), i)).collect();
        let events = file_events(json!({"id":"device","path":"destination"}), json!({"ip":"127.0.0.1"}), &files).unwrap();
        assert_eq!(events.len(), 3);
        let mut recovered = Vec::new();
        let mut nonces = std::collections::HashSet::new();
        for event in events {
            assert_eq!(event["path"], "destination");
            assert!(nonces.insert(event["nonce"].as_str().unwrap().to_owned()));
            let info: Value = serde_json::from_str(event["info"].as_str().unwrap()).unwrap();
            assert_eq!(info["total_num"], 23);
            assert_eq!(info["ip"], "127.0.0.1");
            let names: Vec<(String,i64)> = serde_json::from_value(info["files"].clone()).unwrap();
            assert_eq!(info["num"], names.len());
            recovered.extend(names);
        }
        assert_eq!(recovered, files);
        assert_eq!(file_events(json!({}), json!({}), &[]).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn pending_audit_survives_restart_until_acknowledged() {
        let root = std::env::temp_dir().join(format!("audit-outbox-{}", Uuid::new_v4()));
        let outbox = Outbox::new(root.clone());
        assert!(outbox.pending().await.unwrap().is_empty());
        let value = json!({"id":"device", "nonce":Uuid::new_v4().to_string(),"action":"open", "occurred_at_ms":123});
        let path = outbox.enqueue("https://example.com/api/audit/conn".into(), value.clone()).await.unwrap();
        // An interrupted write is never sent as a complete event.
        tokio::fs::write(root.join("interrupted.tmp"), b"{").await.unwrap();
        drop(outbox);
        let outbox = Outbox::new(root.clone());
        assert_eq!(outbox.pending().await.unwrap(), vec![path.clone()]);
        assert_eq!(outbox.load(&path).await.unwrap().value, value);
        assert!(outbox.acknowledge(&root.join("../outside.json")).await.is_err());
        assert_eq!(outbox.pending().await.unwrap().len(), 1);
        outbox.acknowledge(&path).await.unwrap();
        assert!(outbox.pending().await.unwrap().is_empty());
        outbox.acknowledge(&path).await.unwrap();
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn failed_delivery_is_retried_with_same_nonce_and_old_hosts_are_not_sent() {
        let root = std::env::temp_dir().join(format!("audit-retry-{}", Uuid::new_v4()));
        let outbox = Outbox::new(root.clone());
        let url = "https://example.com/api/audit/conn".to_string();
        let value = json!({"id":"device","nonce":Uuid::new_v4().to_string()});
        outbox.enqueue(url.clone(), value.clone()).await.unwrap();
        assert_eq!(outbox.flush(&[url.clone()], |_| async { bail!("receiver offline") }).await.unwrap(), 1);
        assert_eq!(outbox.pending().await.unwrap().len(), 1);
        assert_eq!(outbox.flush(&[], |_| async { panic!("old host must not be sent") }).await.unwrap(), 1);
        assert_eq!(outbox.flush(&[url.clone()], |record| {
            assert_eq!(record.value["nonce"], value["nonce"]);
            assert!(record.value["occurred_at_ms"].is_u64());
            async { Ok(()) }
        }).await.unwrap(), 0);
        assert!(outbox.pending().await.unwrap().is_empty());
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn invalid_or_unwritable_audit_is_not_reported_as_persisted() {
        let root = std::env::temp_dir().join(format!("audit-failure-{}", Uuid::new_v4()));
        tokio::fs::write(&root, b"not a directory").await.unwrap();
        let outbox = Outbox::new(root.clone());
        let value = json!({"nonce":Uuid::new_v4().to_string()});
        assert!(outbox.enqueue("https://example.com".into(), value).await.is_err());
        assert!(outbox.enqueue("https://example.com".into(), json!({"nonce":"../escape"})).await.is_err());
        tokio::fs::remove_file(root).await.unwrap();
    }
}
