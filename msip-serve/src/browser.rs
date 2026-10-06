//! Where this machine is in a browser, for a page to say: its GXWI address
//! and the fingerprint of the certificate it is served with.
//!
//! GXWI serves HTTPS with a certificate the machine makes for itself, so a
//! browser warns before anyone first signs in. Someone at the console can
//! check what the browser shows against what the machine says here before
//! going past the warning. Installer and first-boot setup both put it on
//! their first page, which on a medium or a new machine is what the console
//! shows.
//!
//! Read as the page is made, every time: the network may have come up since
//! the last one, and gxwid makes the certificate when it first starts.

use std::net::IpAddr;
use std::process::Command;

/// The fingerprint gxwid writes (gxwid(1)).
const FINGERPRINT: &str = "/var/state/gxwi/certificate.sha256";
/// What GXWI listens on when the registry does not say.
const DEFAULT_PORT: u16 = 7780;

/// A sentence naming the machine's GXWI addresses and its certificate's
/// fingerprint. Nothing where there is no certificate: no GXWI here, or it
/// has not started yet.
pub fn where_to_browse() -> Option<String> {
    let fingerprint = std::fs::read_to_string(FINGERPRINT).ok()?;
    let fingerprint = fingerprint.trim();
    if fingerprint.is_empty() {
        return None;
    }
    let (bound, port) = listen();
    let addresses = match bound {
        Some(address) if !address.is_unspecified() => vec![address],
        _ => addresses(),
    };
    Some(sentence(&addresses, port, fingerprint))
}

fn sentence(addresses: &[IpAddr], port: u16, fingerprint: &str) -> String {
    let urls: Vec<String> = addresses
        .iter()
        .map(|address| match address {
            IpAddr::V4(v4) => format!("https://{v4}:{port}/"),
            IpAddr::V6(v6) => format!("https://[{v6}]:{port}/"),
        })
        .collect();
    let at = match urls.as_slice() {
        [] => format!("In a browser, this machine is at https://<its address>:{port}/."),
        [one] => format!("In a browser, this machine is at {one}."),
        many => format!("In a browser, this machine is at {}.", many.join(" or ")),
    };
    format!("{at} Its certificate's SHA-256 fingerprint is {fingerprint}.")
}

/// GXWI's `Listen` value, as an address and a port: the address only where
/// it names one.
fn listen() -> (Option<IpAddr>, u16) {
    let said = Command::new("reg")
        .args(["get", "Machine\\Software\\GXWI", "Listen"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string());
    match said.and_then(|text| text.parse::<std::net::SocketAddr>().ok()) {
        Some(socket) => (Some(socket.ip()), socket.port()),
        None => (None, DEFAULT_PORT),
    }
}

/// The machine's addresses that another machine could reach: not loopback,
/// and not an IPv6 link-local one, which needs a zone a browser's address bar
/// will not take.
fn addresses() -> Vec<IpAddr> {
    let mut found = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `list` with a list we free below.
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return found;
    }
    let mut at = list;
    while !at.is_null() {
        // SAFETY: `at` is a node of the list getifaddrs gave, not yet freed.
        let entry = unsafe { &*at };
        at = entry.ifa_next;
        if entry.ifa_addr.is_null() {
            continue;
        }
        // SAFETY: a non-null ifa_addr points at a sockaddr whose family says
        // which sockaddr it is.
        let address = unsafe {
            match (*entry.ifa_addr).sa_family as i32 {
                libc::AF_INET => {
                    let v4 = &*(entry.ifa_addr as *const libc::sockaddr_in);
                    IpAddr::from(u32::from_be(v4.sin_addr.s_addr).to_be_bytes())
                }
                libc::AF_INET6 => {
                    let v6 = &*(entry.ifa_addr as *const libc::sockaddr_in6);
                    IpAddr::from(v6.sin6_addr.s6_addr)
                }
                _ => continue,
            }
        };
        let link_local = matches!(address, IpAddr::V6(v6) if v6.segments()[0] & 0xffc0 == 0xfe80);
        if !address.is_loopback() && !link_local && !found.contains(&address) {
            found.push(address);
        }
    }
    // SAFETY: the list getifaddrs gave, freed once.
    unsafe { libc::freeifaddrs(list) };
    // IPv4 first: the address someone is likeliest to type.
    found.sort_by_key(|address| address.is_ipv6());
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sentence_names_every_address_and_the_fingerprint() {
        let one: Vec<IpAddr> = vec!["10.0.2.15".parse().unwrap()];
        assert_eq!(
            sentence(&one, 7780, "AB:CD"),
            "In a browser, this machine is at https://10.0.2.15:7780/. Its certificate's SHA-256 fingerprint is AB:CD."
        );
        let two: Vec<IpAddr> = vec!["192.168.1.4".parse().unwrap(), "2001:db8::4".parse().unwrap()];
        assert!(sentence(&two, 443, "AB").contains("https://192.168.1.4:443/ or https://[2001:db8::4]:443/"));
        assert!(sentence(&[], 7780, "AB").contains("https://<its address>:7780/"));
    }

    #[test]
    fn addresses_leave_out_what_no_other_machine_reaches() {
        for address in addresses() {
            assert!(!address.is_loopback(), "{address}");
        }
    }
}
