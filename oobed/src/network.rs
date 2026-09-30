//! What the network page says of the machine's network: `net status`,
//! read for what a person setting the machine up wants to know of it.
//!
//! In words for a surface that draws the page as it is, and as `detail`
//! (PGSS §3, element types) for one that draws it as more:
//!
//! ```text
//! { "readiness": "routed",           // the machine's, as netd has it
//!   "interfaces": [ {
//!     "name": "eth0",
//!     "state": "connected",          // see Interface::state
//!     "addresses": ["10.0.2.15/24"],
//!     "gateway": "10.0.2.2",         // or absent, and so each below
//!     "gateway6": "fe80::2",
//!     "dns": ["10.0.2.3"],
//!     "network": "Office",           // what the operator called it
//!     "hardware": "52:54:00:12:34:56",
//!     "driver": "virtio_net",
//!     "warning": "..." } ] }
//! ```
//!
//! Read from what `net` prints rather than from netd's socket, as the rest
//! of setup reads lpsd through `lps`: oobed runs the commands an operator
//! would, and needs no library of each daemon's to do it.

use std::net::IpAddr;

use serde_json::{Map, Value, json};

/// The image's baseline rule, which joins every wired interface to the
/// profile `default` (`Rules\Interface\wired`).
const WIRED: &str = "wired";
/// Where netd reads its rules and profiles from, and watches.
const NETWORK_KEY: &str = "Machine\\System\\Network";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Interface {
    pub name: String,
    /// netd's stable id for it, from its bus path and hardware address: the
    /// same card in the same slot keeps it across boots, whatever the kernel
    /// names it.
    pub id: String,
    /// `JOIN`, `IGNORE` or `DOWN`, or nothing where no rule spoke.
    pub verdict: Option<String>,
    /// The rule that spoke for it, as a path under `Rules\Interface`.
    pub rule: Option<String>,
    pub up: bool,
    pub carrier: bool,
    /// How far along it is, for one the machine is using.
    pub readiness: Option<String>,
    pub hardware: Option<String>,
    pub driver: Option<String>,
    pub network: Option<String>,
    pub addresses: Vec<String>,
    pub gateway: Option<String>,
    pub gateway6: Option<String>,
    pub dns: Vec<String>,
    pub warning: Option<String>,
}

impl Interface {
    /// One word for where it has got to:
    ///
    /// - `unused`: the machine's rules leave it alone;
    /// - `off`: the rules keep it down;
    /// - `unplugged`: used, with nothing on the other end;
    /// - `connecting`: something on the other end, and no address yet;
    /// - `local`: an address, and no way beyond the network it is on;
    /// - `connected`: a way out.
    pub fn state(&self) -> &'static str {
        match self.verdict.as_deref() {
            Some("JOIN") => {}
            Some("DOWN") => return "off",
            _ => return "unused",
        }
        if !self.carrier {
            return "unplugged";
        }
        match self.readiness.as_deref() {
            Some("routed") => "connected",
            Some("addressed") => "local",
            _ => "connecting",
        }
    }

    /// Whether setup can give it an address by hand. The image's baseline
    /// joins every wired interface by its rule `wired`, and a manual address
    /// is an exception under that rule, so it is judged only among the wired
    /// interfaces and needs nothing else to hold. An interface some other
    /// rule speaks for, or none, is left to whoever wrote that rule.
    pub fn addressable(&self) -> bool {
        self.verdict.as_deref() == Some("JOIN")
            && !self.id.is_empty()
            && self
                .rule
                .as_deref()
                .is_some_and(|rule| rule == WIRED || rule.starts_with(&format!("{WIRED}/")))
    }

    /// Where it has got to, in words.
    pub fn how(&self) -> &'static str {
        match self.state() {
            "unused" => "not used",
            "off" => "turned off",
            "unplugged" => "not connected",
            "connecting" => "connecting",
            "local" => "connected, with no way beyond its own network",
            _ => "connected",
        }
    }

    fn words(&self) -> String {
        let mut line = format!("{}: {}", self.name, self.how());
        if matches!(self.state(), "local" | "connected") {
            // The first of each: the whole of them is in detail, and a
            // line that lists six addresses stops being read.
            if let Some(address) = self.addresses.first() {
                line.push_str(&format!(", as {address}"));
            }
            if let Some(gateway) = &self.gateway {
                line.push_str(&format!(", through {gateway}"));
            }
        }
        line.push('.');
        if let Some(warning) = &self.warning {
            line.push_str(&format!(" {warning}"));
        }
        line
    }

    fn detail(&self) -> Value {
        let mut d = Map::new();
        d.insert("name".into(), json!(self.name));
        d.insert("state".into(), json!(self.state()));
        d.insert("addresses".into(), json!(self.addresses));
        d.insert("dns".into(), json!(self.dns));
        for (key, value) in [
            ("gateway", &self.gateway),
            ("gateway6", &self.gateway6),
            ("network", &self.network),
            ("hardware", &self.hardware),
            ("driver", &self.driver),
            ("warning", &self.warning),
        ] {
            if let Some(value) = value {
                d.insert(key.into(), json!(value));
            }
        }
        Value::Object(d)
    }
}

/// The machine's network, as far as `net status` said it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Network {
    pub readiness: Option<String>,
    pub interfaces: Vec<Interface>,
}

impl Network {
    /// What `net status` printed (`netd/net/src/main.rs`): a line or two of
    /// the machine, then a block per interface headed by its name, each of
    /// its lines a label padded to a column and what it says. Loopback is
    /// left out, being nothing anyone connects.
    pub fn parse(text: &str) -> Network {
        let mut network = Network::default();
        let mut current: Option<Interface> = None;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let indented = line.starts_with(' ');
            let line = line.trim();
            let (label, said) = match line.split_once(char::is_whitespace) {
                Some((label, said)) => (label, said.trim()),
                None => (line, ""),
            };
            if !indented {
                if label == "readiness" {
                    network.readiness = Some(said.to_string());
                } else if said.starts_with('[') {
                    network.interfaces.extend(current.take());
                    current = Some(Interface {
                        name: label.to_string(),
                        id: said
                            .trim_start_matches('[')
                            .trim_end_matches(']')
                            .to_string(),
                        ..Default::default()
                    });
                }
                continue;
            }
            let Some(i) = current.as_mut() else { continue };
            match label {
                // "JOIN(default) by wired": the verdict, then its profile;
                // "(none) by backstop" where there is none.
                "verdict" => {
                    let verdict = said
                        .split(|c: char| c == '(' || c.is_whitespace())
                        .next()
                        .unwrap_or_default();
                    if !verdict.is_empty() {
                        i.verdict = Some(verdict.to_string());
                    }
                    i.rule = said
                        .rsplit_once(" by ")
                        .map(|(_, rule)| rule.trim().to_string());
                }
                "state" => {
                    let mut words = said.split(',').map(str::trim);
                    i.up = words.next() == Some("up");
                    i.carrier = words.any(|w| w == "carrier");
                }
                "readiness" => i.readiness = Some(said.to_string()),
                // "mac path driver", of which the path is the machine's.
                "hardware" => {
                    let mut words = said.split_whitespace();
                    i.hardware = words.next().map(str::to_string);
                    i.driver = words.nth(1).map(str::to_string);
                }
                // "Office [uuid] trust …", or "[uuid]" for one not named.
                "network" => {
                    let name = said.split(" [").next().unwrap_or_default().trim();
                    if !name.is_empty() && !name.starts_with('[') {
                        i.network = Some(name.to_string());
                    }
                }
                "address" => i.addresses.push(said.to_string()),
                "gateway" => i.gateway = Some(said.to_string()),
                "gateway6" => i.gateway6 = Some(said.to_string()),
                "dns" => i.dns = said.split_whitespace().map(str::to_string).collect(),
                "warning" => i.warning = Some(said.to_string()),
                _ => {}
            }
        }
        network.interfaces.extend(current);
        network.interfaces.retain(|i| i.name != "lo");
        network
    }

    /// The page's words: the machine, then a line for each interface.
    pub fn words(&self) -> String {
        let used = |state: &str| self.interfaces.iter().any(|i| i.state() == state);
        let first = if used("connected") {
            "This machine is connected to a network."
        } else if used("local") {
            "This machine is on a network, with no way beyond it."
        } else if used("connecting") {
            "This machine is connecting to a network."
        } else if self.interfaces.is_empty() {
            "This machine has no network hardware that Peios can use."
        } else {
            "This machine is not connected to a network."
        };
        let mut words = first.to_string();
        for i in &self.interfaces {
            words.push('\n');
            words.push_str(&i.words());
        }
        words
    }

    pub fn detail(&self) -> Value {
        let mut d = Map::new();
        if let Some(readiness) = &self.readiness {
            d.insert("readiness".into(), json!(readiness));
        }
        d.insert(
            "interfaces".into(),
            Value::Array(self.interfaces.iter().map(Interface::detail).collect()),
        );
        Value::Object(d)
    }
}

/// An address given to one interface by hand, instead of the one the
/// network offers. Setup keeps it until the end and applies it last, after
/// the account and the machine's name: an interface re-addressed takes with
/// it whatever was reaching the machine through it, a browser included.
#[derive(Debug, Clone, PartialEq)]
pub struct Manual {
    /// The interface, by the name it has now.
    pub interface: String,
    /// And by netd's stable id, which is what the rule names.
    pub id: String,
    /// CIDR, as `192.168.1.20/24`.
    pub address: String,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
}

/// What the manual page asks, by ref, and what was answered to each.
pub struct Answered<'a> {
    pub interface: &'a str,
    pub address: &'a str,
    pub gateway: &'a str,
    pub dns: &'a str,
}

/// An address and the length of its network, from `192.168.1.20/24`.
fn cidr(text: &str) -> Option<(IpAddr, u8)> {
    let (address, length) = text.split_once('/')?;
    let address: IpAddr = address.parse().ok()?;
    let length: u8 = length.parse().ok()?;
    let most = if address.is_ipv4() { 32 } else { 128 };
    (1..=most).contains(&length).then_some((address, length))
}

/// Whether `a` and `b` are on the one network `length` bits long. IPv4
/// only; for IPv6 a gateway is as often link-local as not, and is not
/// second-guessed.
fn same_network(a: IpAddr, b: IpAddr, length: u8) -> bool {
    match (a, b) {
        (IpAddr::V4(a), IpAddr::V4(b)) => {
            let mask = u32::MAX.checked_shl(32 - u32::from(length)).unwrap_or(0);
            u32::from(a) & mask == u32::from(b) & mask
        }
        _ => true,
    }
}

/// The network `address` is on, as `192.168.1.0/24`, for saying so.
fn network_of(address: IpAddr, length: u8) -> String {
    match address {
        IpAddr::V4(a) => {
            let mask = u32::MAX.checked_shl(32 - u32::from(length)).unwrap_or(0);
            format!("{}/{length}", std::net::Ipv4Addr::from(u32::from(a) & mask))
        }
        IpAddr::V6(_) => format!("{address}/{length}"),
    }
}

/// An address a machine can be given: not nothing, not loopback, not a
/// group's.
fn usable(address: IpAddr) -> bool {
    !(address.is_unspecified() || address.is_loopback() || address.is_multicast())
}

impl Manual {
    /// What the manual page's answer comes to, or what is wrong with each
    /// field of it, by ref. `network` is the machine's, for the interface.
    pub fn check(
        answered: Answered<'_>,
        network: &Network,
    ) -> Result<Manual, Vec<(String, String)>> {
        let mut wrong = Vec::new();
        let mut say = |r#ref: &str, why: String| wrong.push((r#ref.to_string(), why));
        let interface = network
            .interfaces
            .iter()
            .find(|i| i.name == answered.interface);
        match interface {
            None if answered.interface.is_empty() => {
                say("manual.interface", "Choose an interface.".into())
            }
            None => say(
                "manual.interface",
                "That interface is not on this machine any more.".into(),
            ),
            Some(i) if !i.addressable() => say(
                "manual.interface",
                "Only a wired interface can be given an address here.".into(),
            ),
            Some(_) => {}
        }
        let address = cidr(answered.address.trim());
        match address {
            None => say(
                "manual.address",
                "An address and the length of its network, as 192.168.1.20/24.".into(),
            ),
            Some((a, _)) if !usable(a) => say(
                "manual.address",
                "That is not an address a machine can have.".into(),
            ),
            Some((IpAddr::V4(a), length)) if length < 31 => {
                let bits =
                    u32::from(a) & !(u32::MAX.checked_shl(32 - u32::from(length)).unwrap_or(0));
                let host = !(u32::MAX.checked_shl(32 - u32::from(length)).unwrap_or(0));
                if bits == 0 || bits == host {
                    say(
                        "manual.address",
                        format!(
                            "That is the address of {} as a whole, not one for a machine on it.",
                            network_of(IpAddr::V4(a), length)
                        ),
                    );
                }
            }
            Some(_) => {}
        }
        let gateway = answered.gateway.trim();
        let gateway = if gateway.is_empty() {
            None
        } else {
            match (gateway.parse::<IpAddr>(), address) {
                (Err(_), _) => {
                    say("manual.gateway", "An address, as 192.168.1.1.".into());
                    None
                }
                (Ok(g), _) if !usable(g) => {
                    say(
                        "manual.gateway",
                        "That is not an address a gateway can have.".into(),
                    );
                    None
                }
                (Ok(g), Some((a, _))) if g.is_ipv4() != a.is_ipv4() => {
                    let family = if a.is_ipv4() { "IPv4" } else { "IPv6" };
                    say(
                        "manual.gateway",
                        format!("The gateway must be {family}, as the address is."),
                    );
                    None
                }
                (Ok(g), Some((a, _))) if g == a => {
                    say(
                        "manual.gateway",
                        "That is the address this machine is being given.".into(),
                    );
                    None
                }
                (Ok(g), Some((a, length))) if !same_network(a, g, length) => {
                    say(
                        "manual.gateway",
                        format!(
                            "The gateway must be on the address's own network, {}.",
                            network_of(a, length)
                        ),
                    );
                    None
                }
                (Ok(g), _) => Some(g.to_string()),
            }
        };
        let mut dns = Vec::new();
        for server in answered
            .dns
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
        {
            match server.parse::<IpAddr>() {
                Ok(s) if usable(s) || s.is_loopback() => dns.push(s.to_string()),
                _ => {
                    say(
                        "manual.dns",
                        format!("“{server}” is not an address. Separate two with a space."),
                    );
                    break;
                }
            }
        }
        match (interface, address) {
            (Some(i), Some((a, length))) if wrong.is_empty() => Ok(Manual {
                interface: i.name.clone(),
                id: i.id.clone(),
                address: format!("{a}/{length}"),
                gateway,
                dns,
            }),
            _ => Err(wrong),
        }
    }

    /// The same, in a sentence, saying when it happens.
    pub fn words(&self) -> String {
        let mut words = format!(
            "At the end of setup, {} is given {}",
            self.interface, self.address
        );
        if let Some(gateway) = &self.gateway {
            words.push_str(&format!(", through {gateway}"));
        }
        match self.dns.as_slice() {
            [] => words.push_str(", with no name servers"),
            [one] => words.push_str(&format!(", asking {one} for names")),
            many => {
                let (last, rest) = many.split_last().unwrap_or((&many[0], &[]));
                words.push_str(&format!(
                    ", asking {} and {last} for names",
                    rest.join(", ")
                ))
            }
        }
        words.push_str(". Until then it keeps the address it has.");
        words
    }

    /// The same as `detail` on those words, for a surface that draws it.
    pub fn detail(&self) -> Value {
        let mut d = Map::new();
        d.insert("interface".into(), json!(self.interface));
        d.insert("address".into(), json!(self.address));
        if let Some(gateway) = &self.gateway {
            d.insert("gateway".into(), json!(gateway));
        }
        d.insert("dns".into(), json!(self.dns));
        Value::Object(d)
    }

    /// What is named for this interface under `Profiles` and `Rules`: a
    /// key name, from the interface's, that a registry key can have.
    fn key(&self) -> String {
        let name: String = self
            .interface
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        format!("manual-{name}")
    }

    /// The registry writes that make netd use it, in order, each as the key,
    /// the value's name and `reg set`'s data. The profile comes first and
    /// the rule that names it last, so that a rule never names a profile
    /// only half written.
    ///
    /// The profile is a subkey of the baseline `default`, so it says only
    /// what differs: the address and the way out are its own, and nothing
    /// the network offers of either is taken. The rule is an exception under
    /// the baseline `wired`, naming the interface by its stable id; being
    /// more specific, it speaks for that interface and DHCP stops on it.
    /// netd watches the subtree, so it applies within a moment.
    pub fn registry(&self) -> Vec<(String, &'static str, String)> {
        let key = self.key();
        let profile = format!("{NETWORK_KEY}\\Profiles\\default\\{key}");
        let rule = format!("{NETWORK_KEY}\\Rules\\Interface\\{WIRED}\\{key}");
        let mut writes = vec![
            (profile.clone(), "Address.Offered", "dword:0".to_string()),
            (profile.clone(), "Address.LinkLocal", "dword:0".to_string()),
            (
                profile.clone(),
                "Address.Static",
                format!("multi:{}", self.address),
            ),
            (profile.clone(), "Route.Offered", "dword:0".to_string()),
        ];
        if let Some(gateway) = &self.gateway {
            writes.push((profile.clone(), "Route.Gateway", format!("multi:{gateway}")));
        }
        // With nothing taken from the network there are no name servers but
        // those given here.
        writes.push((profile.clone(), "Dns.Offered", "dword:0".to_string()));
        if !self.dns.is_empty() {
            writes.push((
                profile.clone(),
                "Dns.Servers",
                format!("multi:{}", self.dns.join(",")),
            ));
        }
        writes.push((
            rule.clone(),
            "Interface.Id.Equal",
            format!("sz:{}", self.id),
        ));
        writes.push((rule, "Actions", format!("multi:JOIN(default/{key})")));
        writes
    }
}

#[cfg(test)]
mod tests {
    use super::{Answered, Manual, Network};
    use serde_json::json;

    /// What `net status` printed on the dev VM once installed, with a
    /// second interface nothing is plugged into.
    const STATUS: &str = "hostname   (unset)
readiness  routed

lo  [f5e053da-4128-5dae-89c9-9d7a1c33d2bb]
  verdict    IGNORE by backstop
  state      up, carrier
  hardware   00:00:00:00:00:00
  address    127.0.0.1/8

eth0  [96f5d807-229f-52f7-818d-a863d64664e9]
  verdict    JOIN(default) by wired
  state      up, carrier
  readiness  routed
  hardware   52:54:00:12:34:56 pci-0000:00:03.0 virtio_net
  network    [1e928e4e-2a16-5232-a018-4e6bc4eb6dff]
  address    10.0.2.15/24
  address    fec0::5054:ff:fe12:3456/64
  gateway    10.0.2.2
  gateway6   fe80::2
  dns        10.0.2.3
  lease      bound from 10.0.2.2, 86365s left

eth1  [0b1c9a4e-5e1d-5a7e-9c55-2f1f0f6b8a10]
  verdict    JOIN(default) by wired
  state      up, no-carrier
  readiness  absent
  hardware   52:54:00:ab:cd:ef pci-0000:00:04.0 e1000e
";

    #[test]
    fn net_status_is_read_for_what_a_person_wants_of_it() {
        let network = Network::parse(STATUS);
        assert_eq!(network.readiness.as_deref(), Some("routed"));
        let names: Vec<_> = network.interfaces.iter().map(|i| &i.name[..]).collect();
        assert_eq!(names, ["eth0", "eth1"], "loopback is left out");
        let eth0 = &network.interfaces[0];
        assert_eq!(eth0.state(), "connected");
        assert_eq!(
            eth0.addresses,
            ["10.0.2.15/24", "fec0::5054:ff:fe12:3456/64"]
        );
        assert_eq!(eth0.gateway.as_deref(), Some("10.0.2.2"));
        assert_eq!(eth0.dns, ["10.0.2.3"]);
        assert_eq!(eth0.hardware.as_deref(), Some("52:54:00:12:34:56"));
        assert_eq!(eth0.driver.as_deref(), Some("virtio_net"));
        assert_eq!(eth0.network, None, "a network nobody named has no name");
        assert_eq!(network.interfaces[1].state(), "unplugged");

        assert_eq!(
            network.words(),
            "This machine is connected to a network.\n\
             eth0: connected, as 10.0.2.15/24, through 10.0.2.2.\n\
             eth1: not connected."
        );
        assert_eq!(
            network.detail()["interfaces"][1],
            json!({
                "name": "eth1",
                "state": "unplugged",
                "addresses": [],
                "dns": [],
                "hardware": "52:54:00:ab:cd:ef",
                "driver": "e1000e",
            })
        );
    }

    #[test]
    fn each_state_is_said_as_it_is() {
        let with = |verdict: &str, state: &str, readiness: &str| {
            Network::parse(&format!(
                "readiness  link\n\nenp1s0  [x]\n  verdict    {verdict}\n  state      {state}\n  readiness  {readiness}\n  network    Office [y] trust private\n"
            ))
            .interfaces
            .remove(0)
        };
        assert_eq!(
            with("IGNORE by backstop", "down, no-carrier", "absent").state(),
            "unused"
        );
        assert_eq!(
            with("(none) by backstop", "down, no-carrier", "absent").state(),
            "unused"
        );
        assert_eq!(
            with("DOWN by rule", "down, carrier", "absent").state(),
            "off"
        );
        assert_eq!(
            with("JOIN(p) by r", "up, carrier", "link").state(),
            "connecting"
        );
        assert_eq!(
            with("JOIN(p) by r", "up, carrier", "addressed").state(),
            "local"
        );
        assert_eq!(
            with("JOIN(p) by r", "up, carrier", "link")
                .network
                .as_deref(),
            Some("Office")
        );
        assert_eq!(
            Network::parse("readiness  absent\n").words(),
            "This machine has no network hardware that Peios can use."
        );
    }

    fn answered<'a>(
        interface: &'a str,
        address: &'a str,
        gateway: &'a str,
        dns: &'a str,
    ) -> Answered<'a> {
        Answered {
            interface,
            address,
            gateway,
            dns,
        }
    }

    #[test]
    fn an_interface_is_known_by_its_id_and_the_rule_that_joined_it() {
        let network = Network::parse(STATUS);
        let eth0 = &network.interfaces[0];
        assert_eq!(eth0.id, "96f5d807-229f-52f7-818d-a863d64664e9");
        assert_eq!(eth0.rule.as_deref(), Some("wired"));
        assert!(eth0.addressable());
        let other = |verdict: &str| {
            Network::parse(&format!("eth9  [x]\n  verdict    {verdict}\n"))
                .interfaces
                .remove(0)
                .addressable()
        };
        assert!(
            other("JOIN(default/db1) by wired/db1"),
            "an exception under wired is wired"
        );
        assert!(
            !other("JOIN(lab) by lab"),
            "another rule's interface is that rule's"
        );
        assert!(!other("IGNORE by backstop"));
        assert!(!other("(none) by backstop"));
    }

    #[test]
    fn a_manual_address_is_checked_field_by_field() {
        let network = Network::parse(STATUS);
        let manual = Manual::check(
            answered(
                "eth0",
                " 192.168.1.20/24 ",
                "192.168.1.1",
                "1.1.1.1, 9.9.9.9",
            ),
            &network,
        )
        .unwrap();
        assert_eq!(
            manual,
            Manual {
                interface: "eth0".into(),
                id: "96f5d807-229f-52f7-818d-a863d64664e9".into(),
                address: "192.168.1.20/24".into(),
                gateway: Some("192.168.1.1".into()),
                dns: vec!["1.1.1.1".into(), "9.9.9.9".into()],
            }
        );
        assert_eq!(
            manual.words(),
            "At the end of setup, eth0 is given 192.168.1.20/24, through 192.168.1.1, asking 1.1.1.1 and 9.9.9.9 \
             for names. Until then it keeps the address it has."
        );
        // Neither a gateway nor name servers is needed.
        let bare = Manual::check(answered("eth0", "fd00::20/64", "", ""), &network).unwrap();
        assert_eq!((bare.gateway, bare.dns.len()), (None, 0));

        let wrong = |a: Answered<'_>| Manual::check(a, &network).unwrap_err();
        assert_eq!(
            wrong(answered("", "192.168.1.20", "10.0.0.1", "one.one")),
            [
                ("manual.interface".into(), "Choose an interface.".into()),
                (
                    "manual.address".into(),
                    "An address and the length of its network, as 192.168.1.20/24.".into()
                ),
                (
                    "manual.dns".into(),
                    "“one.one” is not an address. Separate two with a space.".into()
                ),
            ]
        );
        // Unplugged is no matter: it is addressed for when it is plugged in.
        assert!(Manual::check(answered("eth1", "192.168.1.20/24", "", ""), &network).is_ok());
        let lab = Network::parse("lab0  [x]\n  verdict    JOIN(lab) by lab\n");
        assert_eq!(
            Manual::check(answered("lab0", "192.168.1.20/24", "", ""), &lab).unwrap_err(),
            [(
                "manual.interface".into(),
                "Only a wired interface can be given an address here.".into()
            )]
        );
        assert_eq!(
            wrong(answered("eth0", "192.168.1.0/24", "", "")),
            [(
                "manual.address".into(),
                "That is the address of 192.168.1.0/24 as a whole, not one for a machine on it."
                    .into()
            )]
        );
        assert_eq!(
            wrong(answered("eth0", "127.0.0.2/8", "", ""))[0].1,
            "That is not an address a machine can have."
        );
        assert_eq!(
            wrong(answered("eth0", "192.168.1.20/24", "192.168.2.1", "")),
            [(
                "manual.gateway".into(),
                "The gateway must be on the address's own network, 192.168.1.0/24.".into()
            )]
        );
        assert_eq!(
            wrong(answered("eth0", "192.168.1.20/24", "fe80::1", ""))[0].1,
            "The gateway must be IPv4, as the address is."
        );
        assert_eq!(
            wrong(answered("eth0", "192.168.1.20/24", "192.168.1.20", ""))[0].1,
            "That is the address this machine is being given."
        );
        assert_eq!(
            wrong(answered("eth0", "192.168.1.20/33", "", ""))[0].0,
            "manual.address"
        );
        assert_eq!(
            wrong(answered("eth9", "192.168.1.20/24", "", ""))[0].1,
            "That interface is not on this machine any more."
        );
    }

    #[test]
    fn a_manual_address_is_a_profile_under_default_and_a_rule_under_wired() {
        let manual = Manual {
            interface: "eth0".into(),
            id: "96f5".into(),
            address: "10.0.2.15/24".into(),
            gateway: Some("10.0.2.2".into()),
            dns: vec!["10.0.2.3".into(), "1.1.1.1".into()],
        };
        let profile = "Machine\\System\\Network\\Profiles\\default\\manual-eth0";
        let rule = "Machine\\System\\Network\\Rules\\Interface\\wired\\manual-eth0";
        let writes: Vec<_> = manual
            .registry()
            .into_iter()
            .map(|(k, n, d)| format!("{k} {n} {d}"))
            .collect();
        assert_eq!(
            writes,
            [
                format!("{profile} Address.Offered dword:0"),
                format!("{profile} Address.LinkLocal dword:0"),
                format!("{profile} Address.Static multi:10.0.2.15/24"),
                format!("{profile} Route.Offered dword:0"),
                format!("{profile} Route.Gateway multi:10.0.2.2"),
                format!("{profile} Dns.Offered dword:0"),
                format!("{profile} Dns.Servers multi:10.0.2.3,1.1.1.1"),
                format!("{rule} Interface.Id.Equal sz:96f5"),
                format!("{rule} Actions multi:JOIN(default/manual-eth0)"),
            ]
        );
        // A name a key cannot carry is made one it can.
        let odd = Manual {
            interface: "en\\p 1".into(),
            ..manual
        };
        assert!(odd.registry()[0].0.ends_with("\\manual-en-p-1"));
    }
}
