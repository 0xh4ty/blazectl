use std::{fs::{OpenOptions, self}, io::Write, path::PathBuf};
use anyhow::Result;
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::util::now_utc;

#[derive(Serialize)]
pub struct Entry {
    pub activity: String,
    pub start: String,
    pub end: String,
    #[serde(serialize_with="ser_dur_iso")]
    pub duration: Duration,
}

fn ser_dur_iso<S: serde::Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
    // simple ISO-8601 "PT...H...M...S" without days for v0
    let mut secs = d.whole_seconds();
    if secs < 0 { secs = 0; } // clamp
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s_rem = secs % 60;
    let iso = format!("PT{}H{}M{}S", h, m, s_rem);
    s.serialize_str(&iso)
}

pub fn ensure_dirs() -> Result<()> {
    fs::create_dir_all(".blaze")?;
    Ok(())
}

fn month_file(dt: OffsetDateTime) -> PathBuf {
    let ym = format!("{}-{:02}", dt.year(), u8::try_from(dt.month() as i32).unwrap_or(1));
    PathBuf::from(format!(".blaze/track-{ym}.jsonl"))
}

pub fn append_entry(e: &Entry) -> Result<()> {
    let path = month_file(now_utc());
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = serde_json::to_string(e)? + "\n";
    f.write_all(line.as_bytes())?;
    f.flush()?;
    // If you want stronger durability, uncomment:
    // f.sync_all()?;
    Ok(())
}
