use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub fn now_utc() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

pub fn iso(dt: OffsetDateTime) -> String {
    dt.format(&Rfc3339).unwrap()
}

pub fn parse_iso(s: &str) -> anyhow::Result<OffsetDateTime> {
    Ok(OffsetDateTime::parse(s, &Rfc3339)?)
}
