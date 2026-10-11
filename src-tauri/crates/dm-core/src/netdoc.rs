//! Network doctor: test each link from this PC to the internet in order
//! (adapter, Wi-Fi signal, router, DNS, internet, clock) and say in plain
//! English where it breaks. Unlike the health check this goes online, so
//! the app asks first: it pings the router and fetches Microsoft's
//! connection-test page, the same one Windows uses. Targets are constants.

use crate::ps;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const TEST_URL: &str = "http://www.msftconnecttest.com/connecttest.txt";
const TEST_BODY: &str = "Microsoft Connect Test";

const FACTS: &str = r#"
function T($b) { try { & $b } catch { $null } }
$f = [ordered]@{}
$cfg = T { ,@(Get-NetIPConfiguration -ErrorAction SilentlyContinue | Where-Object { $_.NetAdapter.Status -eq 'Up' }) }
$f.adapters = @($cfg | Where-Object { $_ } | ForEach-Object {
  [ordered]@{
    name = [string]$_.InterfaceAlias
    ipv4 = @($_.IPv4Address | ForEach-Object { [string]$_.IPAddress })
    gateway = @($_.IPv4DefaultGateway | ForEach-Object { [string]$_.NextHop })
    dns = @($_.DNSServer | Where-Object { $_.AddressFamily -eq 2 } | ForEach-Object { $_.ServerAddresses })
  }
})
$w = T { netsh.exe wlan show interfaces 2>$null }
if ($w) {
  $m = $w | Select-String '^\s*Signal\s*:\s*(\d+)\s*%' | Select-Object -First 1
  if ($m) { $f.wifi_signal = [int]$m.Matches[0].Groups[1].Value }
  $m = $w | Select-String '^\s*SSID\s*:\s*(.+)$' | Select-Object -First 1
  if ($m) { $f.wifi_name = $m.Matches[0].Groups[1].Value.Trim() }
}
$gw = @($f.adapters | ForEach-Object { $_.gateway } | Where-Object { $_ }) | Select-Object -First 1
if ($gw) {
  $f.gateway = [string]$gw
  $ping = New-Object System.Net.NetworkInformation.Ping
  $ok = 0; $ms = @()
  foreach ($i in 1..3) {
    $r = T { $ping.Send([string]$gw, 1000) }
    if ($r -and "$($r.Status)" -eq 'Success') { $ok++; $ms += [double]$r.RoundtripTime }
  }
  $f.gateway_replies = $ok
  if ($ms) { $f.gateway_ms = [math]::Round(($ms | Measure-Object -Average).Average) }
}
$sw = [Diagnostics.Stopwatch]::StartNew()
$dn = T { Resolve-DnsName -Name $env:DM_ARG_HOST -Type A -DnsOnly -QuickTimeout -ErrorAction Stop }
$f.dns_ok = [bool]($dn | Where-Object { $_.IPAddress })
$f.dns_ms = $sw.ElapsedMilliseconds
$sw.Restart()
try {
  $req = [Net.HttpWebRequest]::Create($env:DM_ARG_URL)
  $req.AllowAutoRedirect = $false
  $req.Timeout = 8000
  $resp = $req.GetResponse()
  $f.web_status = [int]$resp.StatusCode
  $rd = New-Object IO.StreamReader($resp.GetResponseStream())
  $f.web_matches = ($rd.ReadToEnd().Trim() -eq $env:DM_ARG_BODY)
  $date = $resp.Headers['Date']
  $resp.Close()
  if ($date) { $f.clock_offset_s = [math]::Round(((Get-Date).ToUniversalTime() - [DateTime]::Parse($date).ToUniversalTime()).TotalSeconds) }
} catch [Net.WebException] {
  if ($_.Exception.Response) { $f.web_status = [int]$_.Exception.Response.StatusCode } else { $f.web_error = $_.Exception.Message }
}
$f.web_ms = $sw.ElapsedMilliseconds
$f | ConvertTo-Json -Depth 4 -Compress
"#;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Facts {
    #[serde(default)]
    pub adapters: Vec<Adapter>,
    pub wifi_signal: Option<u32>,
    pub wifi_name: Option<String>,
    pub gateway: Option<String>,
    pub gateway_replies: Option<u32>,
    pub gateway_ms: Option<f64>,
    pub dns_ok: Option<bool>,
    pub dns_ms: Option<f64>,
    pub web_status: Option<u16>,
    pub web_matches: Option<bool>,
    pub web_error: Option<String>,
    pub web_ms: Option<f64>,
    pub clock_offset_s: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Adapter {
    pub name: String,
    #[serde(default)]
    pub ipv4: Vec<String>,
    #[serde(default)]
    pub gateway: Vec<String>,
    #[serde(default)]
    pub dns: Vec<String>,
}

pub use crate::health::{Action, Status};

#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub id: &'static str,
    pub title: &'static str,
    pub status: Status,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub links: Vec<Link>,
    /// The one-line answer: where it breaks and what to do.
    pub verdict: String,
    pub status: Status,
    pub action: Option<Action>,
}

/// Run the tests. Takes up to about 20 seconds when things time out.
pub fn run() -> Result<Report, String> {
    let host = TEST_URL
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or_default();
    let facts: Facts = ps::run_json(
        FACTS,
        &[("URL", TEST_URL), ("HOST", host), ("BODY", TEST_BODY)],
        Duration::from_secs(60),
    )
    .map_err(|e| e.to_string())?;
    Ok(evaluate(&facts))
}

fn link(id: &'static str, title: &'static str, status: Status, summary: String) -> Link {
    Link {
        id,
        title,
        status,
        summary,
    }
}

fn not_tested(id: &'static str, title: &'static str) -> Link {
    link(
        id,
        title,
        Status::Unknown,
        "Not tested: an earlier step failed.".into(),
    )
}

const REMEDY_RESET: Action = Action::Remedy {
    id: "reset-network",
};
const REMEDY_DNS: Action = Action::Remedy { id: "flush-dns" };
const REMEDY_CLOCK: Action = Action::Remedy { id: "time-sync" };

type Verdict = (Status, String, Option<Action>);

/// An early failure: mark the links after it as not tested.
fn stop_at(mut links: Vec<Link>, from: usize, v: Verdict) -> Report {
    const REST: [(&str, &str); 4] = [
        ("router", "Router"),
        ("dns", "DNS"),
        ("internet", "Internet"),
        ("clock", "Clock"),
    ];
    for (id, title) in &REST[from..] {
        links.push(not_tested(id, title));
    }
    finish(links, Some(v))
}

/// Turn facts into the chain of links and one verdict.
pub fn evaluate(f: &Facts) -> Report {
    let mut links = Vec::new();

    // 1. A connection that is up, with an address from the router.
    // Virtual adapters (Hyper-V, VPN, Tailscale) are up too; the one with
    // a router is the real connection.
    let mut usable: Vec<&Adapter> = f
        .adapters
        .iter()
        .filter(|a| a.ipv4.iter().any(|ip| !ip.starts_with("169.254.")))
        .collect();
    usable.sort_by_key(|a| !a.gateway.iter().any(|g| !g.is_empty()));
    if f.adapters.is_empty() {
        links.push(link(
            "adapter",
            "Connection",
            Status::Problem,
            "No network connection is up.".into(),
        ));
        return stop_at(
            links,
            0,
            (
                Status::Problem,
                "This PC isn't connected to any network. Turn Wi-Fi on (or airplane mode off), or plug in the network cable.".into(),
                None,
            ),
        );
    }
    if usable.is_empty() {
        let names: Vec<&str> = f.adapters.iter().map(|a| a.name.as_str()).collect();
        links.push(link(
            "adapter",
            "Connection",
            Status::Problem,
            format!(
                "{} is up but got no address from the router.",
                names.join(", ")
            ),
        ));
        return stop_at(
            links,
            0,
            (
                Status::Problem,
                "The router didn't give this PC an address. Restart the router; if other devices work, reset this PC's network.".into(),
                Some(REMEDY_RESET),
            ),
        );
    }
    let a = usable[0];
    links.push(link(
        "adapter",
        "Connection",
        Status::Good,
        format!(
            "{}: {}",
            a.name,
            a.ipv4
                .iter()
                .find(|ip| !ip.starts_with("169.254."))
                .map(String::as_str)
                .unwrap_or_default()
        ),
    ));

    // 2. Wi-Fi signal, when on Wi-Fi.
    if let Some(sig) = f.wifi_signal {
        let name = f.wifi_name.as_deref().unwrap_or("Wi-Fi");
        links.push(if sig < 30 {
            link(
                "wifi",
                "Wi-Fi signal",
                Status::Problem,
                format!("{name}: {sig}%, very weak."),
            )
        } else if sig < 55 {
            link(
                "wifi",
                "Wi-Fi signal",
                Status::Warning,
                format!("{name}: {sig}%, weak."),
            )
        } else {
            link(
                "wifi",
                "Wi-Fi signal",
                Status::Good,
                format!("{name}: {sig}%."),
            )
        });
    }

    // 3. Router.
    let Some(gw) = f.gateway.as_deref() else {
        links.push(link(
            "router",
            "Router",
            Status::Problem,
            "No router (default gateway) set.".into(),
        ));
        return stop_at(
            links,
            1,
            (
                Status::Problem,
                "This PC doesn't know its router. Restart the router; if only this PC is affected, reset its network.".into(),
                Some(REMEDY_RESET),
            ),
        );
    };
    let router_ok = f.gateway_replies.unwrap_or(0) > 0;
    links.push(if router_ok {
        link(
            "router",
            "Router",
            Status::Good,
            format!(
                "{gw} answers ({} of 3, {:.0} ms).",
                f.gateway_replies.unwrap_or(0),
                f.gateway_ms.unwrap_or(0.0)
            ),
        )
    } else {
        // Some routers ignore ping, so this alone proves nothing.
        link(
            "router",
            "Router",
            Status::Warning,
            format!("{gw} didn't answer ping (some routers never do)."),
        )
    });

    // 4. DNS and 5. internet.
    let dns_ok = f.dns_ok.unwrap_or(false);
    let web_ok = f.web_status == Some(200) && f.web_matches == Some(true);
    let portal = !web_ok && f.web_status.is_some_and(|s| (200..400).contains(&s));
    links.push(if dns_ok {
        link(
            "dns",
            "DNS",
            Status::Good,
            format!("Names resolve ({:.0} ms).", f.dns_ms.unwrap_or(0.0)),
        )
    } else {
        link("dns", "DNS", Status::Problem, "Names don't resolve.".into())
    });
    links.push(if web_ok {
        link(
            "internet",
            "Internet",
            Status::Good,
            format!(
                "Microsoft's test page loads ({:.0} ms).",
                f.web_ms.unwrap_or(0.0)
            ),
        )
    } else if portal {
        link(
            "internet",
            "Internet",
            Status::Warning,
            "A sign-in page is in the way.".into(),
        )
    } else {
        link(
            "internet",
            "Internet",
            Status::Problem,
            match &f.web_error {
                Some(e) => format!("Can't reach the internet ({e})."),
                None => "Can't reach the internet.".into(),
            },
        )
    });

    // 6. Clock, from the test page's Date header.
    links.push(match f.clock_offset_s {
        Some(s) if s.abs() > 300.0 => link(
            "clock",
            "Clock",
            Status::Problem,
            format!("Off by {}.", offset_text(s)),
        ),
        Some(s) if s.abs() > 120.0 => link(
            "clock",
            "Clock",
            Status::Warning,
            format!("Off by {}.", offset_text(s)),
        ),
        Some(_) => link(
            "clock",
            "Clock",
            Status::Good,
            "Within a minute or two of internet time.".into(),
        ),
        None => link(
            "clock",
            "Clock",
            Status::Unknown,
            "Couldn't compare with internet time.".into(),
        ),
    });

    let clock_bad = f.clock_offset_s.is_some_and(|s| s.abs() > 300.0);
    let verdict: Verdict = if web_ok && dns_ok {
        if clock_bad {
            (
                Status::Problem,
                "The internet works, but the clock is wrong. That breaks secure websites, Microsoft 365 and domain sign-in. Sync the clock.".into(),
                Some(REMEDY_CLOCK),
            )
        } else if f.wifi_signal.is_some_and(|s| s < 30) {
            (
                Status::Warning,
                "Everything works, but the Wi-Fi signal is very weak. Move closer to the router or use a cable if it feels slow or drops.".into(),
                None,
            )
        } else {
            (
                Status::Good,
                "Everything works: connection, router, DNS and internet.".into(),
                None,
            )
        }
    } else if portal {
        (
            Status::Warning,
            "This network wants you to sign in or accept terms first (hotel, guest or café Wi-Fi). Open a browser to any website to get the sign-in page.".into(),
            None,
        )
    } else if !dns_ok && f.web_status.is_none() {
        if router_ok {
            (
                Status::Problem,
                "This PC reaches the router but not the internet. Restart the router; if that doesn't help, call your internet provider. If other devices work, reset this PC's network.".into(),
                Some(REMEDY_RESET),
            )
        } else {
            (
                Status::Problem,
                "This PC can't get past the router. Restart the router and check its lights; if other devices work, reset this PC's network.".into(),
                Some(REMEDY_RESET),
            )
        }
    } else if !dns_ok {
        (
            Status::Problem,
            "The internet answers, but looking up names (DNS) fails, so websites won't open by name. Flush the DNS cache; if it persists, restart the router.".into(),
            Some(REMEDY_DNS),
        )
    } else {
        (
            Status::Problem,
            "Names resolve but web traffic doesn't get through. A firewall, VPN or proxy may be blocking it; reset this PC's network if nothing like that is set up.".into(),
            Some(REMEDY_RESET),
        )
    };
    finish(links, Some(verdict))
}

fn offset_text(s: f64) -> String {
    let m = (s.abs() / 60.0).round();
    let dir = if s > 0.0 { "fast" } else { "slow" };
    if m >= 120.0 {
        format!("about {:.0} hours ({dir})", (m / 60.0).round())
    } else {
        format!("about {m:.0} minutes ({dir})")
    }
}

fn finish(links: Vec<Link>, verdict: Option<Verdict>) -> Report {
    let (status, verdict, action) = verdict.unwrap_or((Status::Unknown, "No result.".into(), None));
    Report {
        links,
        verdict,
        status,
        action,
    }
}

/// Plain text for a ticket.
pub fn to_text(r: &Report) -> String {
    let mut s = format!("Network doctor: {}\n\n", r.verdict);
    for l in &r.links {
        s.push_str(&format!("[{:?}] {}: {}\n", l.status, l.title, l.summary));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn working() -> Facts {
        Facts {
            adapters: vec![Adapter {
                name: "Wi-Fi".into(),
                ipv4: vec!["192.168.1.20".into()],
                gateway: vec!["192.168.1.1".into()],
                dns: vec!["192.168.1.1".into()],
            }],
            wifi_signal: Some(80),
            wifi_name: Some("Home".into()),
            gateway: Some("192.168.1.1".into()),
            gateway_replies: Some(3),
            gateway_ms: Some(2.0),
            dns_ok: Some(true),
            dns_ms: Some(20.0),
            web_status: Some(200),
            web_matches: Some(true),
            web_error: None,
            web_ms: Some(60.0),
            clock_offset_s: Some(3.0),
        }
    }

    #[test]
    fn all_good() {
        let r = evaluate(&working());
        assert_eq!(r.status, Status::Good);
        assert!(
            r.links.iter().all(|l| l.status == Status::Good),
            "{:?}",
            r.links
        );
        assert_eq!(r.links.len(), 6);
    }

    #[test]
    fn no_network_stops_early() {
        let r = evaluate(&Facts::default());
        assert_eq!(r.status, Status::Problem);
        assert!(r.verdict.contains("isn't connected"));
        assert_eq!(r.links[0].status, Status::Problem);
        assert!(r.links[1..].iter().all(|l| l.status == Status::Unknown));
    }

    #[test]
    fn prefers_the_adapter_with_a_router() {
        let mut f = working();
        f.adapters.insert(
            0,
            Adapter {
                name: "vEthernet (Default Switch)".into(),
                ipv4: vec!["172.26.96.1".into()],
                gateway: vec!["".into()],
                dns: vec![],
            },
        );
        assert!(evaluate(&f).links[0].summary.starts_with("Wi-Fi"));
    }

    #[test]
    fn no_dhcp_address() {
        let mut f = working();
        f.adapters[0].ipv4 = vec!["169.254.10.2".into()];
        let r = evaluate(&f);
        assert!(r.verdict.contains("didn't give this PC an address"));
        assert_eq!(r.action, Some(REMEDY_RESET));
    }

    #[test]
    fn router_but_no_internet() {
        let mut f = working();
        f.dns_ok = Some(false);
        f.web_status = None;
        f.web_matches = None;
        f.web_error = Some("timed out".into());
        f.clock_offset_s = None;
        let r = evaluate(&f);
        assert!(
            r.verdict
                .contains("reaches the router but not the internet"),
            "{}",
            r.verdict
        );
        f.gateway_replies = Some(0);
        assert!(evaluate(&f).verdict.contains("can't get past the router"));
    }

    #[test]
    fn dns_only_failure_offers_flush() {
        let mut f = working();
        f.dns_ok = Some(false);
        let r = evaluate(&f);
        assert_eq!(r.action, Some(REMEDY_DNS));
    }

    #[test]
    fn captive_portal() {
        let mut f = working();
        f.web_status = Some(302);
        f.web_matches = None;
        let r = evaluate(&f);
        assert_eq!(r.status, Status::Warning);
        assert!(r.verdict.contains("sign in"));
    }

    #[test]
    fn wrong_clock_offers_sync() {
        let mut f = working();
        f.clock_offset_s = Some(-3600.0 * 3.0);
        let r = evaluate(&f);
        assert_eq!(r.action, Some(REMEDY_CLOCK));
        assert!(r.links.iter().any(|l| l.summary.contains("3 hours (slow)")));
    }

    #[test]
    fn silent_router_is_only_a_warning() {
        let mut f = working();
        f.gateway_replies = Some(0);
        let r = evaluate(&f);
        assert_eq!(r.status, Status::Good);
    }

    #[test]
    fn actions_point_at_real_remedies() {
        let ids: Vec<&str> = crate::fixes::recipes().iter().map(|r| r.id).collect();
        for a in [REMEDY_RESET, REMEDY_DNS, REMEDY_CLOCK] {
            let Action::Remedy { id } = a else {
                panic!("not a remedy")
            };
            assert!(ids.contains(&id), "{id}");
        }
    }
}
