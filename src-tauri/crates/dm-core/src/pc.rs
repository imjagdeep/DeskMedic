//! A short "about this PC" for the home page: make and model, Windows
//! version, processor, memory, how long since the last restart, and the
//! network it's on. Read-only; each value is optional.

use crate::ps;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const SCRIPT: &str = r#"
function T($b) { try { & $b } catch { $null } }
$cs = T { Get-CimInstance Win32_ComputerSystem }
$os = T { Get-CimInstance Win32_OperatingSystem }
$cpu = T { Get-CimInstance Win32_Processor | Select-Object -First 1 }
$cv = T { Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' }
$net = T { Get-NetIPConfiguration | Where-Object { $_.NetAdapter.Status -eq 'Up' -and $_.IPv4DefaultGateway } | Select-Object -First 1 }
$f = [ordered]@{}
if ($cs) {
  $f.maker = [string]$cs.Manufacturer
  $f.model = [string]$cs.Model
  $f.memory_gb = [math]::Round($cs.TotalPhysicalMemory / 1GB, 1)
  $f.domain = if ($cs.PartOfDomain) { [string]$cs.Domain } else { $null }
}
if ($os) {
  $f.windows = ([string]$os.Caption).Replace('Microsoft ', '')
  $f.uptime_days = [math]::Round(((Get-Date) - $os.LastBootUpTime).TotalDays, 1)
}
if ($cv) {
  $f.version = [string]$cv.DisplayVersion
  $f.build = "$($cv.CurrentBuild).$($cv.UBR)"
}
if ($cpu) {
  $f.cpu = ([string]$cpu.Name).Trim()
  $f.cores = [int]$cpu.NumberOfCores
}
if ($net) {
  $f.network = [string]$net.InterfaceAlias
  $f.ip = [string](@($net.IPv4Address)[0].IPAddress)
}
$f | ConvertTo-Json -Compress
"#;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Summary {
    pub maker: Option<String>,
    pub model: Option<String>,
    pub memory_gb: Option<f64>,
    pub domain: Option<String>,
    pub windows: Option<String>,
    pub version: Option<String>,
    pub build: Option<String>,
    pub uptime_days: Option<f64>,
    pub cpu: Option<String>,
    pub cores: Option<u32>,
    pub network: Option<String>,
    pub ip: Option<String>,
}

pub fn summary() -> Result<Summary, String> {
    ps::run_json(SCRIPT, &[], Duration::from_secs(60)).map_err(|e| e.to_string())
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn reads_this_pc() {
        let s = super::summary().unwrap();
        assert!(s.windows.is_some(), "{s:?}");
        assert!(s.memory_gb.unwrap_or(0.0) > 0.5, "{s:?}");
    }
}
