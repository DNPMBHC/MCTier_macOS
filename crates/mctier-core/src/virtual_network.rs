use std::net::Ipv4Addr;

/// std creates non-inheritable sockets on Windows. The current mio bind path
/// does not, so children such as the log viewer can otherwise keep ports alive.
pub fn bind_service_listener(
    address: std::net::SocketAddr,
) -> std::io::Result<tokio::net::TcpListener> {
    let listener = std::net::TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    tokio::net::TcpListener::from_std(listener)
}

/// Only literal host addresses in the MCTier virtual subnet can be dialed.
pub fn virtual_host(ip: &str) -> Option<Ipv4Addr> {
    let address = ip.parse::<Ipv4Addr>().ok()?;
    let octets = address.octets();
    (octets[..3] == [10, 126, 126] && (1..=254).contains(&octets[3])).then_some(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_literal_virtual_hosts_are_allowed() {
        for host in [1, 128, 254] {
            assert!(virtual_host(&format!("10.126.126.{host}")).is_some());
        }
        for host in [
            "10.126.126.0",
            "10.126.126.255",
            "127.0.0.1",
            "192.168.1.1",
            "10.126.125.1",
            "::ffff:10.126.126.1",
            "localhost",
            "10.126.126.1.evil.com",
            "10.126.126.01",
            " 10.126.126.1",
            "8.8.8.8",
        ] {
            assert!(virtual_host(host).is_none(), "{host}");
        }
    }
}
