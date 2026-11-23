// The attachment rule: what happens to a stored file some days after the work it belongs to
// is closed. One rule, global, under `attachments` in settings.json. It is checked where it
// is read, not only where it is saved: the file can be edited by hand, and a rule out of
// range is an error, never brought into range (a destructive rule is never guessed).
use serde::{Deserialize, Serialize};
use serde_json::Value;
use workriver_core::{Conditions, Only, Target};

/// What becomes of a file once the rule takes it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum After {
    /// Kept for good: nothing goes (the default).
    Keep,
    Delete,
    /// Moved to the target.
    Move,
}

/// The rule as the user set it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub after: After,
    /// After how many days since the work was closed: 1 to `MAX_DAYS`.
    pub days: u32,
    pub only: Only,
    /// Only files over this many MB (0: any size): up to `MAX_MB`.
    pub min_mb: u32,
    /// Where a move puts the files; read only when moving.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
}

pub const MAX_DAYS: u32 = 3650;
pub const MAX_MB: u32 = 50_000;

impl Default for Rule {
    fn default() -> Self {
        Rule { after: After::Keep, days: 30, only: Only::All, min_mb: 0, target: None }
    }
}

impl Rule {
    /// The conditions core judges a file by.
    pub fn conditions(&self) -> Conditions {
        Conditions { days: self.days, only: self.only, min_mb: self.min_mb }
    }

    /// The rule a settings value gives, checked: keys left out take their defaults, a key
    /// out of range or of an unknown word is an error that names it.
    pub fn of(v: &Value) -> Result<Rule, String> {
        let fix = "; set it in Settings → Attachments";
        let Some(o) = v.as_object() else { return Err(format!("attachments is not a rule{fix}")) };
        let mut rule = Rule::default();
        let word = |key: &str, words: &str| -> Result<Option<Value>, String> {
            match o.get(key) {
                None => Ok(None),
                Some(Value::String(s)) if words.split('|').any(|w| w == s) => Ok(Some(Value::String(s.clone()))),
                Some(x) => Err(format!("{key} is {x}, not one of {}{fix}", words.replace('|', ", "))),
            }
        };
        let number = |key: &str, low: u32, high: u32| -> Result<Option<u32>, String> {
            match o.get(key) {
                None => Ok(None),
                Some(x) => match x.as_u64().filter(|n| (u64::from(low)..=u64::from(high)).contains(n)) {
                    Some(n) => Ok(Some(n as u32)),
                    None => Err(format!("{key} is {x}, not {low} to {high}{fix}")),
                },
            }
        };
        if let Some(w) = word("after", "keep|delete|move")? {
            rule.after = serde_json::from_value(w).map_err(|e| e.to_string())?;
        }
        if let Some(w) = word("only", "all|pictures")? {
            rule.only = serde_json::from_value(w).map_err(|e| e.to_string())?;
        }
        rule.days = number("days", 1, MAX_DAYS)?.unwrap_or(rule.days);
        rule.min_mb = number("minMb", 0, MAX_MB)?.unwrap_or(rule.min_mb);
        if rule.after == After::Move {
            let target = o.get("target").and_then(|t| serde_json::from_value::<Target>(t.clone()).ok()).filter(|t| !t.kind.trim().is_empty());
            rule.target = Some(target.ok_or_else(|| format!("target is missing or has no kind: a move needs a folder{fix}"))?);
        }
        Ok(rule)
    }
}
