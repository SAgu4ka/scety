use std::net::SocketAddr;

pub struct ProxyProtocol;

impl ProxyProtocol {
    pub fn build_v1_header(client_addr: SocketAddr, dest_addr: SocketAddr) -> Vec<u8> {
        let family = if client_addr.is_ipv4() {
            "TCP4"
        } else {
            "TCP6"
        };
        format!(
            "PROXY {} {} {} {} {}\r\n",
            family,
            client_addr.ip(),
            dest_addr.ip(),
            client_addr.port(),
            dest_addr.port()
        )
        .into_bytes()
    }

    pub fn build_v2_header(client_addr: SocketAddr, dest_addr: SocketAddr) -> Vec<u8> {
        const SIG: &[u8; 12] = b"\r\n\r\n\x00\r\nQUIT\n";

        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(SIG);

        buf.push(0x20 | 0x01);

        if client_addr.is_ipv4() && dest_addr.is_ipv4() {
            buf.push((0x1 << 4) | 0x1);
            buf.extend_from_slice(&(12u16.to_be_bytes()));

            if let (std::net::IpAddr::V4(cip), std::net::IpAddr::V4(dip)) =
                (client_addr.ip(), dest_addr.ip())
            {
                buf.extend_from_slice(&cip.octets());
                buf.extend_from_slice(&dip.octets());
                buf.extend_from_slice(&client_addr.port().to_be_bytes());
                buf.extend_from_slice(&dest_addr.port().to_be_bytes());
            }
        } else if client_addr.is_ipv6() && dest_addr.is_ipv6() {
            buf.push((0x2 << 4) | 0x1);
            buf.extend_from_slice(&(36u16.to_be_bytes()));

            if let (std::net::IpAddr::V6(cip), std::net::IpAddr::V6(dip)) =
                (client_addr.ip(), dest_addr.ip())
            {
                for seg in &cip.octets() {
                    buf.push(*seg);
                }
                for seg in &dip.octets() {
                    buf.push(*seg);
                }
                buf.extend_from_slice(&client_addr.port().to_be_bytes());
                buf.extend_from_slice(&dest_addr.port().to_be_bytes());
            }
        } else {
            buf.push(0x00);
            buf.extend_from_slice(&0u16.to_be_bytes());
        }

        buf
    }
}
