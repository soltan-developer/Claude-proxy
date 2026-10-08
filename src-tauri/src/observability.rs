use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const USAGE_FILE_SOFT_LIMIT: u64 = 64 * 1024 * 1024;
const USAGE_RETAINED_RECORDS: usize = 100_000;
const LOG_FILE_SOFT_LIMIT: u64 = 4 * 1024 * 1024;
const LOG_RETAINED_RECORDS: usize = 5_000;

#[derive(Clone)]
pub struct ObservabilityStore {
    usage_path: PathBuf,
    logs_path: PathBuf,
    file_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageRecord {
    timestamp: u64,
    #[serde(default = "unknown_provider")]
    provider: String,
    model: String,
    requested_model: String,
    input_tokens: u64,
    output_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    cache_stats_input_tokens: u64,
    #[serde(default)]
    cache_stats_reported: bool,
    #[serde(default)]
    cache_read_reported: Option<bool>,
    #[serde(default)]
    cache_write_reported: Option<bool>,
    input_estimated: bool,
    output_estimated: bool,
}

fn unknown_provider() -> String {
    "Unknown provider".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageQuery {
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub utc_offset_minutes: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageAggregate {
    pub day_index: i64,
    pub provider: String,
    pub model: String,
    pub requested_model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_input_tokens: u64,
    pub cache_stats_input_tokens: u64,
    pub cache_stats_reported_requests: u64,
    pub cache_read_reported_requests: u64,
    pub cache_write_reported_requests: u64,
    pub requests: u64,
    pub estimated_input_requests: u64,
    pub estimated_output_requests: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntry {
    pub timestamp: u64,
    pub level: String,
    pub category: String,
    pub message: String,
}

impl ObservabilityStore {
    pub fn new(usage_path: PathBuf, logs_path: PathBuf) -> Self {
        Self {
            usage_path,
            logs_path,
            file_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn record_usage(
        &self,
        provider: &str,
        model: &str,
        requested_model: &str,
        input_tokens: u64,
        output_tokens: u64,
        cached_input_tokens: u64,
        cache_write_input_tokens: u64,
        cache_stats_input_tokens: u64,
        cache_stats_reported: bool,
        cache_read_reported: bool,
        cache_write_reported: bool,
        input_estimated: bool,
        output_estimated: bool,
    ) -> Result<(), String> {
        let record = UsageRecord {
            timestamp: unix_time(),
            provider: truncate(provider, 160),
            model: truncate(model, 160),
            requested_model: truncate(requested_model, 160),
            input_tokens,
            output_tokens,
            cached_input_tokens,
            cache_write_input_tokens,
            cache_stats_input_tokens,
            cache_stats_reported,
            cache_read_reported: Some(cache_read_reported),
            cache_write_reported: Some(cache_write_reported),
            input_estimated,
            output_estimated,
        };
        self.append(
            &self.usage_path,
            &record,
            USAGE_FILE_SOFT_LIMIT,
            USAGE_RETAINED_RECORDS,
        )
    }

    pub fn usage(&self, query: UsageQuery) -> Result<Vec<UsageAggregate>, String> {
        if query.start_timestamp >= query.end_timestamp {
            return Err("Choose a valid start and end date.".into());
        }
        let offset_seconds = i64::from(query.utc_offset_minutes.clamp(-840, 840)) * 60;
        let _guard = self
            .file_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let file = match File::open(&self.usage_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("Could not read local usage history: {error}")),
        };

        let mut groups: BTreeMap<(i64, String, String, String), UsageAggregate> = BTreeMap::new();
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let Ok(record) = serde_json::from_str::<UsageRecord>(&line) else {
                continue;
            };
            if record.timestamp < query.start_timestamp || record.timestamp >= query.end_timestamp {
                continue;
            }
            let local_day = ((record.timestamp as i64) - offset_seconds).div_euclid(86_400);
            let key = (
                local_day,
                record.provider.clone(),
                record.model.clone(),
                record.requested_model.clone(),
            );
            let aggregate = groups.entry(key).or_insert_with(|| UsageAggregate {
                day_index: local_day,
                provider: record.provider.clone(),
                model: record.model.clone(),
                requested_model: record.requested_model.clone(),
                input_tokens: 0,
                output_tokens: 0,
                cached_input_tokens: 0,
                cache_write_input_tokens: 0,
                cache_stats_input_tokens: 0,
                cache_stats_reported_requests: 0,
                cache_read_reported_requests: 0,
                cache_write_reported_requests: 0,
                requests: 0,
                estimated_input_requests: 0,
                estimated_output_requests: 0,
            });
            aggregate.input_tokens = aggregate.input_tokens.saturating_add(record.input_tokens);
            aggregate.output_tokens = aggregate.output_tokens.saturating_add(record.output_tokens);
            aggregate.cached_input_tokens = aggregate
                .cached_input_tokens
                .saturating_add(record.cached_input_tokens);
            aggregate.cache_write_input_tokens = aggregate
                .cache_write_input_tokens
                .saturating_add(record.cache_write_input_tokens);
            aggregate.cache_stats_input_tokens = aggregate
                .cache_stats_input_tokens
                .saturating_add(record.cache_stats_input_tokens);
            aggregate.cache_stats_reported_requests = aggregate
                .cache_stats_reported_requests
                .saturating_add(if record.cache_stats_reported { 1 } else { 0 });
            aggregate.cache_read_reported_requests = aggregate
                .cache_read_reported_requests
                .saturating_add(u64::from(record.cache_read_reported == Some(true)));
            aggregate.cache_write_reported_requests = aggregate
                .cache_write_reported_requests
                .saturating_add(u64::from(record.cache_write_reported == Some(true)));
            aggregate.requests = aggregate.requests.saturating_add(1);
            aggregate.estimated_input_requests = aggregate
                .estimated_input_requests
                .saturating_add(if record.input_estimated { 1 } else { 0 });
            aggregate.estimated_output_requests = aggregate
                .estimated_output_requests
                .saturating_add(if record.output_estimated { 1 } else { 0 });
        }
        Ok(groups.into_values().collect())
    }

    pub fn log(&self, level: &str, category: &str, message: &str) {
        let entry = DiagnosticEntry {
            timestamp: unix_time(),
            level: match level {
                "error" => "error",
                "warning" => "warning",
                _ => "info",
            }
            .into(),
            category: truncate(category, 48),
            message: truncate(message, 1_600),
        };
        let _ = self.append(
            &self.logs_path,
            &entry,
            LOG_FILE_SOFT_LIMIT,
            LOG_RETAINED_RECORDS,
        );
    }

    pub fn logs(&self, requested_limit: usize) -> Result<Vec<DiagnosticEntry>, String> {
        let limit = requested_limit.clamp(1, 1_000);
        let _guard = self
            .file_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let file = match File::open(&self.logs_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("Could not read local diagnostics: {error}")),
        };
        let mut entries = VecDeque::with_capacity(limit);
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if let Ok(entry) = serde_json::from_str::<DiagnosticEntry>(&line) {
                if entries.len() == limit {
                    entries.pop_front();
                }
                entries.push_back(entry);
            }
        }
        Ok(entries.into_iter().rev().collect())
    }

    pub fn clear_logs(&self) -> Result<(), String> {
        let _guard = self
            .file_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match fs::remove_file(&self.logs_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Could not clear local diagnostics: {error}")),
        }
    }

    fn append<T: Serialize>(
        &self,
        path: &PathBuf,
        value: &T,
        soft_limit: u64,
        retained_records: usize,
    ) -> Result<(), String> {
        let _guard = self
            .file_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let parent = path
            .parent()
            .ok_or_else(|| "The local data folder is invalid.".to_string())?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create the local data folder: {error}"))?;
        let encoded = serde_json::to_vec(value).map_err(|error| error.to_string())?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| format!("Could not open local history: {error}"))?;
        file.write_all(&encoded)
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|error| format!("Could not write local history: {error}"))?;
        file.flush()
            .map_err(|error| format!("Could not flush local history: {error}"))?;
        let size = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        drop(file);
        if size > soft_limit {
            self.trim_file(path, retained_records)?;
        }
        Ok(())
    }

    fn trim_file(&self, path: &PathBuf, retained_records: usize) -> Result<(), String> {
        let file = File::open(path).map_err(|error| error.to_string())?;
        let mut lines = VecDeque::with_capacity(retained_records);
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if lines.len() == retained_records {
                lines.pop_front();
            }
            lines.push_back(line);
        }
        let mut output = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        for line in lines {
            output
                .write_all(line.as_bytes())
                .and_then(|_| output.write_all(b"\n"))
                .map_err(|error| error.to_string())?;
        }
        output.flush().map_err(|error| error.to_string())
    }
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut text: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        text.push('…');
    }
    text
}


#[cfg(test)]
mod v2_coverage_tests {
    use super::*;

    #[test]
    fn cache_reads_and_writes_have_independent_reporting_coverage() {
        let dir = std::env::temp_dir()
            .join(format!("qbs2-usage-test-{}", uuid::Uuid::new_v4()));
        let store = ObservabilityStore::new(dir.join("usage.jsonl"), dir.join("logs.jsonl"));
        store.record_usage("mock", "model", "route", 100, 12, 0, 5, 100,
            true, false, true, false, false).unwrap();
        store.record_usage("mock", "model", "route", 100, 12, 0, 0, 0,
            false, false, false, false, false).unwrap();
        store.record_usage("mock", "model", "route", 100, 12, 0, 0, 100,
            true, true, false, false, false).unwrap();
        let aggregate = store.usage(UsageQuery {
            start_timestamp: unix_time().saturating_sub(10),
            end_timestamp: unix_time().saturating_add(10),
            utc_offset_minutes: 0,
        }).unwrap();
        assert_eq!(aggregate.len(), 1);
        assert_eq!(aggregate[0].requests, 3);
        assert_eq!(aggregate[0].cache_stats_reported_requests, 2);
        assert_eq!(aggregate[0].cache_read_reported_requests, 1);
        assert_eq!(aggregate[0].cache_write_reported_requests, 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn old_usage_records_without_new_fields_deserialize_as_unknown() {
        let record: UsageRecord = serde_json::from_value(serde_json::json!({
            "timestamp":1,"provider":"mock","model":"x","requestedModel":"alias",
            "inputTokens":100,"outputTokens":10,"cachedInputTokens":0,
            "cacheStatsReported":true,"inputEstimated":false,"outputEstimated":false
        })).unwrap();
        assert_eq!(record.cache_read_reported, None);
        assert_eq!(record.cache_write_reported, None);
    }
}
