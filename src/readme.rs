use anyhow::Result;
use std::{collections::HashMap, fs};
use time::{Duration, OffsetDateTime, Date, format_description::well_known::Rfc3339};

use crate::util::{now_utc, iso};

use plotters::prelude::*; // SVG renderer
use plotters::element::PathElement;

#[derive(Default, Clone, Copy)]
pub(crate) struct Totals { train: i64, battle: i64 }
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
    let last75_dates = days_back(today, 75);

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

    // keep ASCII generator available (unused in README but handy)
    let ascii_area = ascii_area_30d(&per_day, &last75_dates, 12);

    // generate SVG asset (scales nicely on mobile/GitHub)
    let _ = std::fs::create_dir_all("assets")?;
    render_activity_svg(&per_day, &last75_dates, "assets/activity.svg", 900, 240)?;

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
        &ascii_area, // still passed for compatibility
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

fn ascii_area_30d(per_day: &HashMap<Date, Totals>, last30: &[Date], height: usize) -> String {
    if last30.is_empty() || height == 0 {
        return String::new();
    }

    // Gather values (minutes per day)
    let vals: Vec<i64> = last30.iter()
        .map(|d| per_day.get(d).map(|t| minutes(t.total())).unwrap_or(0))
        .collect();

    // Find min/max for normalization
    let (min_v, max_v) = match (vals.iter().min(), vals.iter().max()) {
        (Some(a), Some(b)) => (*a, *b),
        _ => (0, 0),
    };

    // Edge: all values equal -> render single baseline or repeated low row
    if max_v == min_v {
        // produce a single-line flat chart (centered) as fallback
        let line: String = vals.iter().map(|_| '▁').collect();
        return line;
    }

    // Normalize into levels 0..(height-1)
    let range = (max_v - min_v) as f64;
    let levels: Vec<usize> = vals.into_iter().map(|v| {
        let norm = (v - min_v) as f64 / range; // 0.0..1.0
        // Multiply by (height-1) so top row is height-1
        let lvl = (norm * ((height - 1) as f64)).round() as isize;
        // clamp (safety)
        lvl.max(0).min((height - 1) as isize) as usize
    }).collect();

    // Build rows top-down
    let mut rows: Vec<String> = Vec::with_capacity(height);
    for row in (0..height).rev() {
        let mut line = String::with_capacity(levels.len());
        for &lvl in &levels {
            // Use an "area fill" style: fill any cell where lvl >= row
            if lvl >= row {
                line.push('█'); // visible block; change to '▓' / '#' etc. if you prefer
            } else {
                line.push(' ');
            }
        }
        rows.push(line);
    }

    // Optionally append a simple baseline (x-axis) with ticks for readability
    let mut baseline = String::with_capacity(levels.len());
    for (i, _) in rows[0].chars().enumerate() {
        // mark every 5th column for rough tick; tune as needed
        if i % 5 == 0 {
            baseline.push('|');
        } else {
            baseline.push('-');
        }
    }

    // Combine: rows + baseline
    let mut out = rows.join("\n");
    out.push('\n');
    out.push_str(&baseline);

    out
}

/// Render activity area chart: raw daily area+line (blue) + single long-trend curve (grey)
/// Trend control points are coarse-bucketed (TREND_WINDOW_DAYS) and extrapolated to chart edges.
/// Raw values are in minutes but scaled to hours/day for the y-axis.
pub(crate) fn render_activity_svg(
    per_day: &HashMap<Date, Totals>,
    dates: &[Date],
    out_path: &str,
    width: u32,
    height: u32,
) -> anyhow::Result<()> {
    // Tunables
    const TREND_WINDOW_DAYS: usize = 8;
    const TREND_SAMPLES_PER_SEGMENT: usize = 50;

    // color palette (user requested)
    let bg = RGBColor(19, 23, 31);              // rgb(19, 22.5, 30.5) -> rounded
    let text_col = RGBColor(194, 199, 208);     // #c2c7d0
    let accent = RGBColor(1, 170, 255);         // #01aaff for the main graph line/points
    let border_accent = RGBColor(88, 186, 236);
    let trend_col = RGBColor(210, 20, 20);      // keep the red trend

    // raw per-day minutes
    let vals: Vec<f64> = dates
        .iter()
        .map(|d| per_day.get(d).map(|t| minutes(t.total()) as f64).unwrap_or(0.0))
        .collect();
    let n = vals.len();
    if n == 0 {
        let root = SVGBackend::new(out_path, (width, height)).into_drawing_area();
        root.fill(&bg)?;
        root.present()?;
        return Ok(());
    }

    // y domain in hours (we keep values in minutes but derive domain in hours)
    let min_v = vals.iter().cloned().fold(f64::INFINITY, f64::min) / 60.0;
    let max_v = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max) / 60.0;
    let (y0, y1) = if (max_v - min_v).abs() < std::f64::EPSILON {
        (0.0, max_v.max(0.5))
    } else {
        let pad = (max_v - min_v) * 0.07;
        ((min_v - pad).max(0.0), max_v + pad)
    };

    let root = SVGBackend::new(out_path, (width, height)).into_drawing_area();
    // fill background with chosen dark color
    root.fill(&bg)?;

    root.draw(&Rectangle::new(
        [(0, 0), (width as i32 - 1, height as i32 - 1)],
        ShapeStyle {
            color: border_accent.to_rgba(),
            filled: false,
            stroke_width: 10,
        },
    ))?;

    // raw points scaled to hours for plotting
    let points_raw: Vec<(f64, f64)> = vals
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v / 60.0))
        .collect();
    let x_upper_f = points_raw.len() as f64;

    // build chart using f64 domain
    let mut chart = ChartBuilder::on(&root)
        .margin(8)
        .x_label_area_size(0)
        .y_label_area_size(50)
        .right_y_label_area_size(0)
        .build_cartesian_2d(0f64..x_upper_f, y0..y1)?;

    // configure mesh: keep grid minimal; style labels with text_col
    chart
        .configure_mesh()
        .disable_mesh()
        .y_desc("hours / day")
        .axis_desc_style(("sans-serif", 14).into_font().color(&text_col))
        .y_label_formatter(&|v| format!("{:.1}", v))
        .y_label_style(("sans-serif", 10).into_font().color(&text_col))
        .x_labels((points_raw.len() / 10).max(2))
        .x_label_style(("sans-serif", 10).into_font().color(&text_col))
        .label_style(("sans-serif", 11).into_font().color(&text_col))
        .axis_style(text_col.stroke_width(1))   // <-- make axis lines use text color
        .draw()?;

    // area + line + dots using accent color (accent filled area with low alpha)
    let area_fill = RGBAColor(accent.0, accent.1, accent.2, 0.10);
    let line_style = accent.stroke_width(2);
    chart.draw_series(AreaSeries::new(points_raw.clone(), 0.0, area_fill))?;
    chart.draw_series(LineSeries::new(points_raw.clone().into_iter(), line_style))?;
    chart.draw_series(points_raw.iter().map(|&(x, y)| {
        Circle::new((x, y), 1, accent.filled())
    }))?;

    // -------- build coarse trend points (minutes -> convert to hours here) --------
    let mut trend_pts: Vec<(f64, f64)> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let end = (i + TREND_WINDOW_DAYS).min(n);
        let slice = &vals[i..end];
        let avg = if slice.is_empty() { 0.0 } else { slice.iter().sum::<f64>() / slice.len() as f64 };
        let center = (i as f64 + (end - 1) as f64) / 2.0;
        trend_pts.push((center, avg / 60.0)); // convert to hours
        i = end;
    }

    // fallback: denser buckets if too few trend points
    if trend_pts.len() < 3 && n >= 3 {
        let mut alt: Vec<(f64, f64)> = Vec::new();
        let step = (TREND_WINDOW_DAYS as f64 / 2.0).ceil() as usize;
        let mut j = 0usize;
        while j < n {
            let end = (j + step).min(n);
            let slice = &vals[j..end];
            let avg = if slice.is_empty() { 0.0 } else { slice.iter().sum::<f64>() / slice.len() as f64 };
            let center = (j as f64 + (end - 1) as f64) / 2.0;
            alt.push((center, avg / 60.0));
            j = end;
        }
        if alt.len() >= trend_pts.len() {
            trend_pts = alt;
        }
    }

    // extrapolate endpoints so trend covers full range
    let x_left = 0.0f64;
    let x_right = (n - 1) as f64;
    if trend_pts.is_empty() {
        trend_pts.push((x_left, vals[0] / 60.0));
        trend_pts.push((x_right, vals[n - 1] / 60.0));
    } else {
        if trend_pts[0].0 > x_left {
            if trend_pts.len() >= 2 {
                let p0 = trend_pts[0];
                let p1 = trend_pts[1];
                let dx = (p1.0 - p0.0).max(1e-9);
                let slope = (p1.1 - p0.1) / dx;
                let y_at_left = p0.1 + slope * (x_left - p0.0);
                trend_pts.insert(0, (x_left, y_at_left));
            } else {
                trend_pts.insert(0, (x_left, vals[0] / 60.0));
            }
        } else {
            trend_pts[0].0 = x_left;
        }

        let last_idx = trend_pts.len() - 1;
        if trend_pts[last_idx].0 < x_right {
            if trend_pts.len() >= 2 {
                let p_last = trend_pts[last_idx];
                let p_prev = trend_pts[last_idx - 1];
                let dx = (p_last.0 - p_prev.0).max(1e-9);
                let slope = (p_last.1 - p_prev.1) / dx;
                let y_at_right = p_last.1 + slope * (x_right - p_last.0);
                trend_pts.push((x_right, y_at_right));
            } else {
                trend_pts.push((x_right, vals[n - 1] / 60.0));
            }
        } else {
            trend_pts[last_idx].0 = x_right;
        }
    }

    // Catmull-Rom spline (dense sampling)
    fn catmull_rom_spline(pts: &[(f64, f64)], samples: usize) -> Vec<(f64, f64)> {
        if pts.len() < 2 { return pts.to_vec(); }
        let mut out = Vec::with_capacity(pts.len() * samples + 1);
        let idx = |i: isize, max: usize| -> usize {
            if i < 0 { 0 } else if (i as usize) >= max { max - 1 } else { i as usize }
        };
        for ii in 0..(pts.len() - 1) {
            let p0 = pts[idx(ii as isize - 1, pts.len())];
            let p1 = pts[ii];
            let p2 = pts[ii + 1];
            let p3 = pts[idx(ii as isize + 2, pts.len())];
            for s in 0..samples {
                let t = s as f64 / (samples as f64);
                let t2 = t * t;
                let t3 = t2 * t;
                let x = 0.5 * (2.0*p1.0 + (-p0.0 + p2.0)*t + (2.0*p0.0 - 5.0*p1.0 + 4.0*p2.0 - p3.0)*t2 + (-p0.0 + 3.0*p1.0 - 3.0*p2.0 + p3.0)*t3);
                let y = 0.5 * (2.0*p1.1 + (-p0.1 + p2.1)*t + (2.0*p0.1 - 5.0*p1.1 + 4.0*p2.1 - p3.1)*t2 + (-p0.1 + 3.0*p1.1 - 3.0*p2.1 + p3.1)*t3);
                out.push((x, y));
            }
        }
        if let Some(last) = pts.last() { out.push(*last); }
        out
    }

    let trend_curve = if trend_pts.len() >= 2 {
        catmull_rom_spline(&trend_pts, TREND_SAMPLES_PER_SEGMENT)
    } else {
        trend_pts.clone()
    };

    // draw trend (red) on top
    chart.draw_series(std::iter::once(PathElement::new(
        trend_curve,
        trend_col.stroke_width(4),
    )))?;

    root.present()?;
    Ok(())
}

fn render_md(
    now: OffsetDateTime,
    all_time: Totals,
    _last7: &Totals,
    _last30: &Totals,
    last30_tag: &Totals,
    daily7: &[(Date, Totals)],
    _streak_any: i32,
    _streak_train: i32,
    _streak_battle: i32,
    _ascii_area: &str,
) -> anyhow::Result<String> {
    use std::fmt::Write;
    let version = env!("CARGO_PKG_VERSION");

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

    // Image-embedded Activity Graph (75 days)
    writeln!(s, "## Activity Graph")?;
    writeln!(s, "![Activity Graph](assets/activity.svg)")?;
    writeln!(s, "(Total hours per day for the last 75 days)")?;
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
