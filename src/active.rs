use std::{fs, path::PathBuf};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use crate::util::{now_utc, iso};

#[derive(Default, Serialize, Deserialize)]
struct Active {
    #[serde(skip_serializing_if="Option::is_none")]
    train: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    battle: Option<String>,
}

fn path() -> PathBuf { PathBuf::from(".blaze/active.json") }

fn load() -> Result<Active> {
    if !path().exists() { return Ok(Active::default()); }
    let s = fs::read_to_string(path())?;
    if s.trim().is_empty() { return Ok(Active::default()); }
    Ok(serde_json::from_str(&s)?)
}

fn save(a: &Active) -> Result<()> {
    let tmp = ".blaze/active.json.tmp";
    fs::write(tmp, serde_json::to_string_pretty(a)?)?;
    fs::rename(tmp, path())?;
    Ok(())
}

pub fn start(tag: &str) -> Result<()> {
    let mut a = load()?;
    let now = iso(now_utc());
    match tag {
        "train" => {
            if a.train.is_some() { println!("Already running: train since {}", a.train.as_ref().unwrap()); return Ok(()); }
            // auto-stop battle if running
            if a.battle.is_some() { println!("Auto-stop battle before starting train. Run `blazectl stop battle` first."); }
            a.train = Some(now);
        }
        "battle" => {
            if a.battle.is_some() { println!("Already running: battle since {}", a.battle.as_ref().unwrap()); return Ok(()); }
            if a.train.is_some() { println!("Auto-stop train before starting battle. Run `blazectl stop train` first."); }
            a.battle = Some(now);
        }
        _ => return Err(anyhow!("unknown tag: {tag} (use train|battle)")),
    }
    save(&a)
}

pub fn stop(tag: &str) -> Result<Option<crate::store::Entry>> {
    let mut a = load()?;
    let end = now_utc();

    let (start_opt, _clear_train, _clear_battle) = match tag {
        "train"  => (a.train.take(), true,  false),
        "battle" => (a.battle.take(), false, true),
        _ => return Err(anyhow!("unknown tag: {tag} (use train|battle)")),
    };

    match start_opt {
        None => Ok(None),
        Some(start_iso) => {
            save(&a)?;
            let start = crate::util::parse_iso(&start_iso)?;
            let dur = end - start;
            Ok(Some(crate::store::Entry {
                activity: tag.to_string(),
                start: start_iso,
                end: crate::util::iso(end),
                duration: dur,
            }))
        }
    }
}

pub fn status() -> Result<Option<(String, String)>> {
    let a = load()?;
    if let Some(s) = a.train { return Ok(Some(("train".into(), s))); }
    if let Some(s) = a.battle { return Ok(Some(("battle".into(), s))); }
    Ok(None)
}
