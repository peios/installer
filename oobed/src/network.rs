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

use serde_json::{Map, Value, json};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Interface {
    pub name: String,
    /// `JOIN`, `IGNORE` or `DOWN`, or nothing where no rule spoke.
    pub verdict: Option<String>,
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

    fn words(&self) -> String {
        let how = match self.state() {
            "unused" => "not used",
            "off" => "turned off",
            "unplugged" => "not connected",
            "connecting" => "connecting",
            "local" => "connected, with no way beyond its own network",
            _ => "connected",
        };
        let mut line = format!("{}: {how}", self.name);
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

#[cfg(test)]
mod tests {
    use super::Network;
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
}
