<img src="src/assets/logo.png" alt="DeskMedic icon" width="66" align="left">

# DeskMedic

A portable Windows toolkit for helpdesk technicians: see what fills a disk, clear temporary files safely, fix common Disk Management problems, and run one-click fixes for everyday calls.
<br clear="left">

- **Portable.** One `.exe`, no install. Runs from a USB stick or a network share. Settings and the log live in `%ProgramData%\DeskMedic`, never next to the program.
- **Safe by design.** Everything that changes something is shown first, and every change is written to a log you can paste into a ticket. Documents, Desktop, Downloads, pictures and OneDrive are never touched. It never formats, initializes or deletes data partitions.
- **Offline.** No account, no telemetry. The only network use is a link to check for a newer version, when you click it.

## What it does

| | |
|---|---|
| **Disk map** | Scan a drive and see which folders and files take the space: treemap with drill-down, folder list, the 50 largest files, totals by file type, and user profiles (oldest first) with their sizes. As administrator, NTFS drives are read straight from the master file table, like WizTree; otherwise folder by folder. |
| **Cleanup** | Measures, then clears: temp files (user and Windows, older than a day), Windows Update downloads, the Delivery Optimization cache, crash dumps and error reports, the thumbnail cache, the Recycle Bin, Chrome/Edge/Firefox caches (only while closed; never logins, history or passwords), the Teams cache, Outlook temporary attachments, and Windows component cleanup (DISM). Shows free space before and after. |
| **Disks** | Shows every disk like Disk Management, with plain-English findings and a fix for each: disk offline (including cloned-disk signature collisions), read-only disks or partitions, volumes without a drive letter, file-system errors (check, repair, or schedule a check of C: at restart), nearly full drives, unhealthy drives, and a stuck or disabled Virtual Disk Service. |
| **Extend / shrink** | Resize a data partition within the limits Windows reports. When *Extend Volume* is greyed out because the recovery partition sits between C: and the free space, a guided fix follows Microsoft's documented WinRE steps: turn WinRE off, remove that partition, extend C:, recreate the recovery partition at the end, turn WinRE back on. Each step is checked against the disk before it runs, and a stopped run can be resumed. |
| **Fix-its** | Flush DNS, reset the network stack, clear a stuck print queue, restart Explorer, refresh Group Policy, reset the Teams cache, repair Windows Update, SFC, DISM RestoreHealth, sync the clock. |
| **Log** | Who ran what, on which PC, with what result. Export as text for a ticket. |

## Download and run

Download `DeskMedic-x.y.z.exe` from [Releases](../../releases) and run it. It asks whether to restart as administrator; most fixes need that. Without admin rights you can still scan (more slowly), clean your own temp files and look at the disks.

Needs Windows 10 or 11 with the Microsoft Edge WebView2 runtime (built into Windows 11 and current Windows 10).

**The exe is not code-signed yet.** Windows SmartScreen will say "Windows protected your PC": choose **More info → Run anyway**. Some antivirus products are wary of unsigned tools that delete files; check the SHA-256 on the release page against your download.

## Safety rules (enforced in code, covered by tests)

- Cleanup only works inside a fixed list of folders that hold temporary data. A cleanup folder may not be inside, or contain, a protected folder (Documents, Desktop, Downloads, Pictures, Music, Videos, OneDrive…, or the folder DeskMedic runs from), for any user on the PC.
- Junctions and symbolic links are never followed or deleted, so a link in a temp folder can't lead the cleaner into user files.
- Files that are in use or read-only are skipped.
- Disk changes need administrator rights, a typed confirmation (the disk number or drive letter), and are checked again against a fresh read of the disks right before they run.
- Commands are fixed: programs with fixed arguments or constant PowerShell scripts. Values such as a disk number are passed as validated numbers in environment variables, never pasted into script text.

## Building

Requirements: Windows, Rust (stable), Node.js 22.

```sh
npm ci
npm run tauri dev                      # run with hot reload
npm run tauri build -- --no-bundle     # portable exe in src-tauri/target/release
```

Checks: `npm run typecheck`, and in `src-tauri`: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`. The engine (`src-tauri/crates/dm-core`) has no Tauri dependency and is tested on its own. A few tests need administrator rights and are marked `#[ignore]`; run them with `cargo test -p dm-core -- --ignored` from an elevated prompt.

Set `DESKMEDIC_DATA_DIR` to keep settings and the log somewhere else while testing.

## More tools

- **[DeskZero](https://github.com/imjagdeep/deskzero)**: a tiny offline app that keeps your folders organized. Windows, macOS and Linux.

Made by Jagdeep Sandhu · [github.com/imjagdeep](https://github.com/imjagdeep)

## License

MIT. See [LICENSE](LICENSE).
