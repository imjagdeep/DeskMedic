//! User profiles on this PC (Win32_UserProfile), so a technician can spot old
//! profiles filling a shared PC. Listing only; deleting comes later.

use crate::ps;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub path: String,
    pub sid: String,
    /// ISO 8601, or empty when Windows doesn't know.
    pub last_used: String,
    /// Signed in right now.
    pub loaded: bool,
    /// Filled in from a scan of the drive, when there is one.
    #[serde(default)]
    pub size: Option<u64>,
}

const SCRIPT: &str = r#"
@(Get-CimInstance Win32_UserProfile | Where-Object { -not $_.Special } | ForEach-Object {
  [pscustomobject]@{
    path = $_.LocalPath
    sid = $_.SID
    last_used = if ($_.LastUseTime) { $_.LastUseTime.ToUniversalTime().ToString('o') } else { '' }
    loaded = [bool]$_.Loaded
  }
}) | ConvertTo-Json -Compress
"#;

pub fn list() -> Result<Vec<Profile>, ps::PsError> {
    // A single profile comes back as an object, not an array.
    let v: serde_json::Value = ps::run_json(SCRIPT, &[], Duration::from_secs(60))?;
    let items = match v {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Null => Vec::new(),
        other => vec![other],
    };
    let mut out: Vec<Profile> = items
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .map_err(|e| ps::PsError::Parse(e.to_string()))?;
    // Oldest use first: those are the ones worth a look.
    out.sort_by(|a, b| a.last_used.cmp(&b.last_used));
    Ok(out)
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn lists_at_least_the_current_user() {
        let me = std::env::var("USERPROFILE").unwrap().to_lowercase();
        let list = super::list().unwrap();
        assert!(list.iter().any(|p| p.path.to_lowercase() == me), "{list:?}");
    }
}
