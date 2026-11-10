//! Resource history (plan 3.4): in memory, 15 minutes at 10 s resolution per
//! app (the maximum memory and the average CPU of each 10 s); on disk, one row
//! per app per minute in `history/YYYY-MM-DD.csv`, kept for a number of days.

use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};

use crate::model::HistoryView;

pub const BUCKET_SECONDS: i64 = 10;
pub const BUCKETS: usize = 90;

#[derive(Debug, Clone, Default)]
struct Bucket {
    start: i64,
    memory_max: f64,
    cpu_sum: f64,
    cpu_n: u32,
}

/// One app's last 15 minutes.
#[derive(Debug, Clone, Default)]
pub struct Series {
    buckets: VecDeque<Bucket>,
}

impl Series {
    pub fn add(&mut self, at: DateTime<Utc>, memory_mb: f64, cpu: Option<f64>) {
        let start = at.timestamp() - at.timestamp().rem_euclid(BUCKET_SECONDS);
        if self.buckets.back().is_none_or(|b| b.start != start) {
            self.buckets.push_back(Bucket {
                start,
                ..Default::default()
            });
            while self.buckets.len() > BUCKETS {
                self.buckets.pop_front();
            }
        }
        let b = self.buckets.back_mut().unwrap();
        b.memory_max = b.memory_max.max(memory_mb);
        if let Some(cpu) = cpu {
            b.cpu_sum += cpu;
            b.cpu_n += 1;
        }
    }

    /// The buckets of the last 15 minutes, oldest first; gaps (the app was
    /// not running) are zero.
    pub fn view(&self, now: DateTime<Utc>) -> HistoryView {
        let last = now.timestamp() - now.timestamp().rem_euclid(BUCKET_SECONDS);
        let first = last - BUCKET_SECONDS * (BUCKETS as i64 - 1);
        let mut memory = vec![0.0; BUCKETS];
        let mut cpu = vec![0.0; BUCKETS];
        for b in &self.buckets {
            if b.start < first {
                continue;
            }
            let i = ((b.start - first) / BUCKET_SECONDS) as usize;
            if i < BUCKETS {
                memory[i] = (b.memory_max * 10.0).round() / 10.0;
                cpu[i] = if b.cpu_n > 0 {
                    ((b.cpu_sum / b.cpu_n as f64) * 10.0).round() / 10.0
                } else {
                    0.0
                };
            }
        }
        HistoryView {
            interval_seconds: BUCKET_SECONDS as u32,
            memory_mb: memory,
            cpu_percent: cpu,
        }
    }
}

/// A minute's figures for one app, for the CSV row.
#[derive(Debug, Clone, Default)]
struct Minute {
    memory_max: f64,
    private_max: f64,
    cpu_sum: f64,
    cpu_n: u32,
    gpu_max: f64,
    processes: usize,
}

/// Every app's history, in memory and on disk.
pub struct History {
    dir: PathBuf,
    keep_days: u32,
    series: BTreeMap<String, Series>,
    minute: Option<DateTime<Utc>>,
    rows: BTreeMap<String, Minute>,
    pruned: Option<NaiveDate>,
}

/// One sample of one app.
pub struct Sample {
    pub memory_mb: f64,
    pub private_mb: Option<f64>,
    pub cpu_percent: Option<f64>,
    pub gpu_mem_mb: Option<f64>,
    pub processes: usize,
}

impl History {
    pub fn new(dir: &Path, keep_days: u32) -> History {
        History {
            dir: dir.to_path_buf(),
            keep_days,
            series: BTreeMap::new(),
            minute: None,
            rows: BTreeMap::new(),
            pruned: None,
        }
    }

    pub fn set_keep_days(&mut self, days: u32) {
        self.keep_days = days;
    }

    /// Records a sample; writes the previous minute's rows when a new minute starts.
    pub fn record(&mut self, id: &str, at: DateTime<Utc>, sample: &Sample) -> io::Result<()> {
        let minute = at
            .with_second(0)
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or(at);
        let mut result = Ok(());
        if self.minute.is_some_and(|m| m != minute) {
            result = self.flush();
        }
        self.minute = Some(minute);
        self.series.entry(id.to_string()).or_default().add(
            at,
            sample.memory_mb,
            sample.cpu_percent,
        );
        let row = self.rows.entry(id.to_string()).or_default();
        row.memory_max = row.memory_max.max(sample.memory_mb);
        row.private_max = row.private_max.max(sample.private_mb.unwrap_or(0.0));
        if let Some(cpu) = sample.cpu_percent {
            row.cpu_sum += cpu;
            row.cpu_n += 1;
        }
        row.gpu_max = row.gpu_max.max(sample.gpu_mem_mb.unwrap_or(0.0));
        row.processes = row.processes.max(sample.processes);
        result
    }

    pub fn view(&self, id: &str, now: DateTime<Utc>) -> Option<HistoryView> {
        self.series.get(id).map(|s| s.view(now))
    }

    /// Writes the rows of the minute in progress.
    pub fn flush(&mut self) -> io::Result<()> {
        let Some(minute) = self.minute else {
            return Ok(());
        };
        if self.rows.is_empty() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{}.csv", minute.format("%Y-%m-%d")));
        let new = !path.exists();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let mut text = String::new();
        if new {
            text.push_str("minute,app,memoryMb,privateMb,cpuPercent,gpuMemMb,processes\n");
        }
        for (id, row) in std::mem::take(&mut self.rows) {
            let cpu = if row.cpu_n > 0 {
                row.cpu_sum / row.cpu_n as f64
            } else {
                0.0
            };
            text.push_str(&format!(
                "{},{id},{:.1},{:.1},{:.2},{:.1},{}\n",
                crate::util::timestamp(minute),
                row.memory_max,
                row.private_max,
                cpu,
                row.gpu_max,
                row.processes
            ));
        }
        file.write_all(text.as_bytes())?;
        self.prune(minute.date_naive());
        Ok(())
    }

    /// Removes day files older than the days kept, once a day.
    fn prune(&mut self, today: NaiveDate) {
        if self.pruned == Some(today) {
            return;
        }
        self.pruned = Some(today);
        let oldest = today - Duration::days(self.keep_days.max(1) as i64 - 1);
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(date) = name
                .strip_suffix(".csv")
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                && date < oldest
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// One app's history from the day files, from `now - hours` to `now`, in
/// `points` equal steps (the maximum memory and the average CPU of the
/// minutes in each step; 0 where nothing ran). For the 24-hour and 7-day
/// charts; `id` is an app's id or `n8tray`.
pub fn read(dir: &Path, id: &str, now: DateTime<Utc>, hours: u32, points: usize) -> HistoryView {
    let points = points.max(1);
    let span = i64::from(hours.max(1)) * 3600;
    let step = (span / points as i64).max(60);
    let first = now.timestamp() - step * points as i64;
    let mut memory = vec![0.0f64; points];
    let mut cpu_sum = vec![0.0f64; points];
    let mut cpu_n = vec![0u32; points];
    let mut day = DateTime::from_timestamp(first, 0)
        .unwrap_or(now)
        .date_naive();
    while day <= now.date_naive() {
        let path = dir.join(format!("{}.csv", day.format("%Y-%m-%d")));
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines().skip(1) {
                let mut f = line.split(',');
                let (Some(minute), Some(app), Some(mem), Some(_), Some(cpu)) =
                    (f.next(), f.next(), f.next(), f.next(), f.next())
                else {
                    continue;
                };
                if app != id {
                    continue;
                }
                let Some(at) = DateTime::parse_from_rfc3339(minute)
                    .ok()
                    .map(|t| t.timestamp())
                else {
                    continue;
                };
                if at < first || at >= first + step * points as i64 {
                    continue;
                }
                let i = ((at - first) / step) as usize;
                if let Ok(mem) = mem.parse::<f64>() {
                    memory[i] = memory[i].max(mem);
                }
                if let Ok(cpu) = cpu.parse::<f64>() {
                    cpu_sum[i] += cpu;
                    cpu_n[i] += 1;
                }
            }
        }
        let Some(next) = day.succ_opt() else { break };
        day = next;
    }
    HistoryView {
        interval_seconds: step as u32,
        memory_mb: memory.iter().map(|m| (m * 10.0).round() / 10.0).collect(),
        cpu_percent: cpu_sum
            .iter()
            .zip(&cpu_n)
            .map(|(s, n)| {
                if *n > 0 {
                    ((s / f64::from(*n)) * 10.0).round() / 10.0
                } else {
                    0.0
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + s, 0).unwrap()
    }

    fn sample(mb: f64, cpu: f64) -> Sample {
        Sample {
            memory_mb: mb,
            private_mb: Some(mb / 2.0),
            cpu_percent: Some(cpu),
            gpu_mem_mb: None,
            processes: 2,
        }
    }

    #[test]
    fn ten_second_buckets_keep_max_memory_and_average_cpu() {
        let mut s = Series::default();
        let base = at(0) - Duration::seconds(at(0).timestamp() % 10);
        s.add(base, 100.0, Some(1.0));
        s.add(base + Duration::seconds(3), 150.0, Some(3.0));
        s.add(base + Duration::seconds(10), 120.0, None);
        let v = s.view(base + Duration::seconds(10));
        assert_eq!(v.memory_mb.len(), BUCKETS);
        assert_eq!(v.memory_mb[BUCKETS - 2], 150.0);
        assert_eq!(v.cpu_percent[BUCKETS - 2], 2.0);
        assert_eq!(v.memory_mb[BUCKETS - 1], 120.0);
        let later = s.view(base + Duration::minutes(30));
        assert!(
            later.memory_mb.iter().all(|m| *m == 0.0),
            "older than 15 minutes"
        );
    }

    #[test]
    fn one_row_per_app_per_minute_and_old_days_pruned() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("2020-01-01.csv"), "old").unwrap();
        let mut h = History::new(dir.path(), 7);
        let t = at(0).with_second(5).unwrap();
        h.record("a", t, &sample(100.0, 2.0)).unwrap();
        h.record("a", t + Duration::seconds(10), &sample(200.0, 4.0))
            .unwrap();
        h.record("b", t + Duration::seconds(10), &sample(50.0, 0.0))
            .unwrap();
        h.record("a", t + Duration::seconds(60), &sample(10.0, 0.0))
            .unwrap();
        let day = t.format("%Y-%m-%d").to_string();
        let text = std::fs::read_to_string(dir.path().join(format!("{day}.csv"))).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[1].contains(",a,200.0,100.0,3.00,0.0,2"));
        assert!(lines[2].contains(",b,50.0,"));
        assert!(!dir.path().join("2020-01-01.csv").exists());
    }

    #[test]
    fn day_files_read_back_in_steps() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::new(dir.path(), 7);
        let t = at(0).with_second(0).unwrap();
        for m in 0..120 {
            let when = t + Duration::minutes(m);
            h.record("a", when, &sample(100.0 + m as f64, 2.0)).unwrap();
            h.record("b", when, &sample(5.0, 0.0)).unwrap();
        }
        h.flush().unwrap();
        let now = t + Duration::minutes(120);
        let v = read(dir.path(), "a", now, 24, 288);
        assert_eq!(v.interval_seconds, 300);
        assert_eq!(v.memory_mb.len(), 288);
        // The last 2 hours are the last 24 five-minute steps.
        assert_eq!(v.memory_mb[288 - 25], 0.0);
        assert_eq!(v.memory_mb[288 - 1], 219.0);
        assert_eq!(v.cpu_percent[288 - 1], 2.0);
        let missing = read(dir.path(), "nobody", now, 168, 168);
        assert!(missing.memory_mb.iter().all(|m| *m == 0.0));
    }
}
