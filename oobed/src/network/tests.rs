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
            "That is the address of 192.168.1.0/24 as a whole, not one for a machine on it.".into()
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
