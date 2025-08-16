use anyhow::Result;
use std::{collections::HashMap, fs};
use time::{Duration, OffsetDateTime, Date, format_description::well_known::Rfc3339};

use crate::util::{now_utc, iso};

#[derive(Default, Clone, Copy)]
struct Totals { train: i64, battle: i64 }
impl Totals {
    fn add(&mut self, tag: &str, secs: i64) {
        match tag {
            "train" => self.train += secs,
            "battle" => self.battle += secs,
            _ => {}
        }
    }
    fn total(&self) -> i64 { self.train + self.battle }
}

pub fn render_all() -> Result<()> {
    let now = now_utc();

    let today = now.date();
    let last7_dates = days_back(today, 7);
    let last30_dates = days_back(today, 30);

    let entries = read_all_entries()?;

    let mut all_time = Totals::default();
    let mut per_day: HashMap<Date, Totals> = HashMap::new();

    for v in entries {
        let activity = v.get("activity").and_then(|x| x.as_str()).unwrap_or("");
        let start_iso = v.get("start").and_then(|x| x.as_str()).unwrap_or("");
        let dur_secs = parse_duration_seconds(v.get("duration").and_then(|x| x.as_str()).unwrap_or("PT0S"));

        all_time.add(activity, dur_secs);

        if let Ok(st_dt) = OffsetDateTime::parse(start_iso, &Rfc3339).map(|t| t.date()) {
            per_day.entry(st_dt).or_default().add(activity, dur_secs);
        }
    }

    let last7_tot = sum_over(&per_day, &last7_dates);
    let last30_tot = sum_over(&per_day, &last30_dates);

    let mut last30_tag = Totals::default();
    for d in &last30_dates {
        if let Some(t) = per_day.get(d) {
            last30_tag.train += t.train;
            last30_tag.battle += t.battle;
        }
    }

    let mut last7_rows = last7_dates.clone();
    last7_rows.sort();
    let daily7: Vec<(Date, Totals)> = last7_rows
        .into_iter()
        .map(|d| (d, per_day.get(&d).copied().unwrap_or_default()))
        .collect();

    let streak_any = streak_days(&per_day, today, |t| t.total() > 0);
    let streak_train = streak_days(&per_day, today, |t| t.train > 0);
    let streak_battle = streak_days(&per_day, today, |t| t.battle > 0);

    let sparkline = sparkline_30d(&per_day, &last30_dates);

    let out = render_md(
        now,
        all_time,
        &last7_tot,
        &last30_tot,
        &last30_tag,
        &daily7,
        streak_any,
        streak_train,
        streak_battle,
        &sparkline,
    )?;

    fs::write("README.md", out)?;
    Ok(())
}

/* ---------- Helpers ---------- */

fn read_all_entries() -> Result<Vec<serde_json::Value>> {
    let mut entries = Vec::new();
    if let Ok(rd) = fs::read_dir(".blaze") {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("track-") && name.ends_with(".jsonl")) { continue; }
            if let Ok(s) = fs::read_to_string(e.path()) {
                for line in s.lines().filter(|l| !l.trim().is_empty()) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                        entries.push(v);
                    }
                }
            }
        }
    }
    Ok(entries)
}

fn days_back(today: Date, n: i32) -> Vec<Date> {
    (0..n).map(|i| today - Duration::days((n - 1 - i) as i64)).collect()
}

fn sum_over(per_day: &HashMap<Date, Totals>, days: &[Date]) -> Totals {
    let mut t = Totals::default();
    for d in days {
        if let Some(x) = per_day.get(d) {
            t.train += x.train;
            t.battle += x.battle;
        }
    }
    t
}

fn streak_days<F: Fn(&Totals) -> bool>(per_day: &HashMap<Date, Totals>, end_day: Date, pred: F) -> i32 {
    let mut count = 0;
    let mut d = end_day;
    loop {
        let t = per_day.get(&d).copied().unwrap_or_default();
        if pred(&t) { count += 1; } else { break; }
        d = match d.previous_day() {
            Some(prev) => prev,
            None => break,
        };
        if count > 365 { break; }
    }
    count
}

fn parse_duration_seconds(iso: &str) -> i64 {
    let mut s = iso.trim();
    if !s.starts_with("PT") { return 0; }
    s = &s[2..];
    let mut hours=0; let mut mins=0; let mut secs=0;
    let mut num = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() { num.push(ch); continue; }
        let val = num.parse::<i64>().unwrap_or(0);
        match ch {
            'H' => hours = val,
            'M' => mins  = val,
            'S' => secs  = val,
            _ => {}
        }
        num.clear();
    }
    hours*3600 + mins*60 + secs
}

fn hm(secs: i64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{h}h {m:02}m")
}

fn minutes(secs: i64) -> i64 { secs / 60 }

fn sparkline_30d(per_day: &HashMap<Date, Totals>, last30: &[Date]) -> String {
    const BLOCKS: &[char] = &['▁','▂','▃','▄','▅','▆','▇','█'];
    let vals: Vec<i64> = last30.iter()
        .map(|d| per_day.get(d).map(|t| minutes(t.total())).unwrap_or(0))
        .collect();

    let (min_v, max_v) = match (vals.iter().min(), vals.iter().max()) {
        (Some(a), Some(b)) => (*a, *b),
        _ => (0, 0),
    };

    if max_v == min_v {
        return std::iter::repeat(BLOCKS[0]).take(vals.len()).collect();
    }

    vals.into_iter()
        .map(|v| {
            let norm = (v - min_v) as f64 / (max_v - min_v) as f64;
            let idx = (norm * ((BLOCKS.len() - 1) as f64)).round() as usize;
            BLOCKS[idx]
        })
        .collect()
}

fn render_md(
    now: OffsetDateTime,
    all_time: Totals,
    _last7: &Totals,
    _last30: &Totals,
    last30_tag: &Totals,
    daily7: &[(Date, Totals)],
    streak_any: i32,
    streak_train: i32,
    streak_battle: i32,
    sparkline: &str,
) -> anyhow::Result<String> {
    use std::fmt::Write;
    let version = env!("CARGO_PKG_VERSION");
    let _repo = std::env::current_dir()
        .ok()
        .and_then(|p| p.into_os_string().into_string().ok())
        .unwrap_or_else(|| ".".into());

    let mut s = String::new();

    // Header & quick stats
    writeln!(s, "# BLAZECTL")?;
    writeln!(s)?;
    writeln!(s, "> A minimal, fast, CLI-based time tracker for disciplined solo work.
    Run `start` / `stop` commands, store logs in JSONL, auto-generate README stats,
    and track your **Train** and **Battle** hours with streaks and activity charts.")?;
    writeln!(s)?;
    writeln!(s, "## Field Report")?;
    writeln!(s)?;

    writeln!(s, "- **Updated (UTC):** {}", iso(now))?;
    writeln!(s, "- **All-time (Total):** {}", hm(all_time.total()))?;
    writeln!(s, "- **All-time (Train):** {}", hm(all_time.train))?;
    writeln!(s, "- **All-time (Battle):** {}", hm(all_time.battle))?;
    writeln!(s)?;

    // Per-tag 30d
    writeln!(s, "## Per-tag (last 30d)")?;
    writeln!(s, "- Train: {}", hm(last30_tag.train))?;
    writeln!(s, "- Battle: {}", hm(last30_tag.battle))?;
    writeln!(s)?;

    // Daily (last 7 days)
    writeln!(s, "## Daily (last 7 days)")?;
    writeln!(s, "| Date       | Train | Battle | Total |")?;
    writeln!(s, "|------------|-------|--------|-------|")?;
    let mut rows = daily7.to_vec();
    rows.sort_by_key(|(d, _)| *d);
    for (d, t) in rows {
        writeln!(
            s,
            "| {} | {:>5} | {:>6} | {:>5} |",
            d, hm(t.train), hm(t.battle), hm(t.total())
        )?;
    }
    writeln!(s)?;

    // Streaks
    writeln!(s, "## Streaks")?;
    writeln!(s, "- Any: {} days", streak_any)?;
    writeln!(s, "- Train: {} days", streak_train)?;
    writeln!(s, "- Battle: {} days", streak_battle)?;
    writeln!(s)?;

    // Sparkline
    writeln!(s, "## Activity (last 30d)")?;
    writeln!(s, "{} (total minutes per day)", sparkline)?;
    writeln!(s)?;

    // Installation (clear steps)
    writeln!(s, "## Installation")?;
    writeln!(s, "1. **Install Rust**")?;
    writeln!(s, "   ```bash")?;
    writeln!(s, "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh")?;
    writeln!(s, "   ```")?;
    writeln!(s, "2. **Clone the repository**")?;
    writeln!(s, "   ```bash")?;
    writeln!(s, "   git clone https://github.com/0xh4ty/blazectl.git")?;
    writeln!(s, "   cd blazectl")?;
    writeln!(s, "   # Remove any old tracking data")?;
    writeln!(s, "   rm -rf ~/.blaze")?;
    writeln!(s, "   ```")?;
    writeln!(s, "3. **Build and install**")?;
    writeln!(s, "   ```bash")?;
    writeln!(s, "   cargo install --path .")?;
    writeln!(s, "   ```")?;
    writeln!(s)?;

    // Usage (concise)
    writeln!(s, "## Usage")?;
    writeln!(s, "Start/stop sessions:")?;
    writeln!(s, "```bash")?;
    writeln!(s, "blazectl start train")?;
    writeln!(s, "blazectl stop  train")?;
    writeln!(s, "blazectl start battle")?;
    writeln!(s, "blazectl stop  battle")?;
    writeln!(s, "```")?;
    writeln!(s, "Other commands:")?;
    writeln!(s, "```bash")?;
    writeln!(s, "blazectl status")?;
    writeln!(s, "blazectl render-readme")?;
    writeln!(s, "```")?;
    writeln!(s, "Data is stored in `.blaze/track-YYYY-MM.jsonl` (UTC timestamps, ISO-8601 durations).")?;
    writeln!(s, "Configure keybindings externally (WM/OS).")?;
    writeln!(s)?;

    // License
    writeln!(s, "## License")?;
    writeln!(s, "BLAZECTL is open-source under the [MIT License](LICENSE).")?;
    writeln!(s)?;

    // Footer
    writeln!(s, "---")?;
    writeln!(s)?;
    writeln!(s, "Generated by **blazectl v{}**.", version)?;
    writeln!(s, "Created by [0xh4ty](https://github.com/0xh4ty) for fellow warriors.")?;

    Ok(s)
}
