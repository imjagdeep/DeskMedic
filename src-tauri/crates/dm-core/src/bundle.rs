//! "Export report": one zip with plain-text files a helpdesk can read: system
//! information, a fresh health check, the last network doctor result and the
//! DeskMedic log. Built in a temporary folder and zipped with PowerShell's
//! Compress-Archive (the destination path is passed as data, never as code).

use crate::ps;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const SYSINFO: &str = r#"
function T($b) { try { & $b } catch { $null } }
$cs = T { Get-CimInstance Win32_ComputerSystem }
$os = T { Get-CimInstance Win32_OperatingSystem }
$bios = T { Get-CimInstance Win32_BIOS }
$cpu = T { Get-CimInstance Win32_Processor | Select-Object -First 1 }
$cv = T { Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' }
$o = New-Object System.Collections.Generic.List[string]
$o.Add("Computer:      $env:COMPUTERNAME")
$o.Add("User:          $env:USERDOMAIN\$env:USERNAME")
if ($cs) {
  $o.Add("Maker / model: $($cs.Manufacturer) $($cs.Model)")
  $o.Add($(if ($cs.PartOfDomain) { "Domain:        $($cs.Domain)" } else { "Workgroup:     $($cs.Workgroup)" }))
  $o.Add(("Memory:        {0:N1} GB" -f ($cs.TotalPhysicalMemory / 1GB)))
}
if ($bios) { $o.Add("Serial number: $($bios.SerialNumber)") }
if ($cpu) { $o.Add("Processor:     $($cpu.Name.Trim())") }
if ($os) {
  $o.Add("Windows:       $($os.Caption) $($cv.DisplayVersion) (build $($os.BuildNumber).$($cv.UBR))")
  $o.Add("Installed:     $($os.InstallDate.ToString('yyyy-MM-dd'))")
  $o.Add("Last restart:  $($os.LastBootUpTime.ToString('yyyy-MM-dd HH:mm'))")
}
$o.Add("")
$o.Add("Drives:")
T { Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' } | ForEach-Object {
  $o.Add(("  {0}  {1:N1} GB free of {2:N1} GB ({3})" -f $_.DeviceID, ($_.FreeSpace / 1GB), ($_.Size / 1GB), $_.FileSystem))
}
$o.Add("")
$o.Add("Network:")
T { Get-NetIPConfiguration | Where-Object { $_.NetAdapter.Status -eq 'Up' } } | ForEach-Object {
  $o.Add("  $($_.InterfaceAlias): $(@($_.IPv4Address.IPAddress) -join ', ')  gateway $(@($_.IPv4DefaultGateway.NextHop) -join ', ')  DNS $(@($_.DNSServer | Where-Object { $_.AddressFamily -eq 2 } | ForEach-Object { $_.ServerAddresses }) -join ', ')")
}
$o -join "`r`n"
"#;

const ZIP: &str = r#"
$src = Join-Path $env:DM_ARG_DIR '*'
Compress-Archive -Path $src -DestinationPath $env:DM_ARG_DEST -Force
"#;

/// System information as plain text.
pub fn system_info() -> String {
    ps::run_text(SYSINFO, &[], Duration::from_secs(60))
        .unwrap_or_else(|e| format!("Could not read system information: {e}"))
}

/// Write `files` (name, text) into a zip at `dest`. Names must be plain file
/// names; the temporary folder is removed afterwards.
pub fn write_zip(files: &[(&str, String)], dest: &Path) -> Result<(), String> {
    if !dest
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        return Err("The bundle must be saved as a .zip file.".into());
    }
    // Unique per call: two bundles in the same millisecond must not share
    // (and then delete) one folder.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "deskmedic-bundle-{}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not make a temp folder: {e}"))?;
    let result = (|| {
        for (name, text) in files {
            if name.contains(['/', '\\', ':']) || name.starts_with('.') {
                return Err(format!("bad file name {name}"));
            }
            // A BOM so Notepad on any locale reads it as UTF-8.
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(text.replace("\r\n", "\n").replace('\n', "\r\n").as_bytes());
            std::fs::write(dir.join(name), bytes)
                .map_err(|e| format!("could not write {name}: {e}"))?;
        }
        ps::run_text(
            ZIP,
            &[
                ("DIR", &dir.to_string_lossy()),
                ("DEST", &dest.to_string_lossy()),
            ],
            Duration::from_secs(120),
        )
        .map(|_| ())
        .map_err(|e| format!("could not create the zip: {e}"))
    })();
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        tracing::warn!("could not remove {}: {e}", dir.display());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_non_zip_and_odd_names() {
        let d = tempfile::tempdir().unwrap();
        assert!(write_zip(&[], &d.path().join("x.txt")).is_err());
        let e = write_zip(&[("..\\evil.txt", "x".into())], &d.path().join("x.zip")).unwrap_err();
        assert!(e.contains("bad file name"), "{e}");
    }

    #[cfg(windows)]
    #[test]
    fn writes_a_zip() {
        let d = tempfile::tempdir().unwrap();
        // A space and a quote in the path must not matter.
        let dest = d.path().join("it's a bundle.zip");
        write_zip(
            &[("a.txt", "hello".into()), ("b.txt", "line1\nline2".into())],
            &dest,
        )
        .unwrap();
        assert!(std::fs::metadata(&dest).unwrap().len() > 100);
    }

    #[cfg(windows)]
    #[test]
    fn system_info_has_the_basics() {
        let s = system_info();
        assert!(s.contains("Computer:"), "{s}");
        assert!(s.contains("Windows:"), "{s}");
    }
}
