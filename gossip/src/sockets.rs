use std::net::UdpSocket;

pub struct Tvu {
    sockets: Vec<UdpSocket>,
    quic_socket: UdpSocket,
}

impl Tvu {
    /// returns new tvu sockets group
    pub fn new(sockets: Vec<UdpSocket>, quic_socket: UdpSocket) -> Self {
        Self {
            sockets,
            quic_socket,
        }
    }

    /// returns tvu sockets slice
    pub fn sockets(&self) -> &[UdpSocket] {
        &self.sockets
    }

    /// returns tvu quic socket ref
    pub fn quic_socket(&self) -> &UdpSocket {
        &self.quic_socket
    }
}

pub struct Tpu {
    sockets: Vec<UdpSocket>,
    forwards: Vec<UdpSocket>,
    votes: Vec<UdpSocket>,
}

impl Tpu {
    /// returns new tpu sockets group
    pub fn new(sockets: Vec<UdpSocket>, forwards: Vec<UdpSocket>, votes: Vec<UdpSocket>) -> Self {
        Self {
            sockets,
            forwards,
            votes,
        }
    }

    /// returns tpu sockets slice
    pub fn sockets(&self) -> &[UdpSocket] {
        &self.sockets
    }

    /// returns tpu forwards sockets slice
    pub fn forwards(&self) -> &[UdpSocket] {
        &self.forwards
    }

    /// returns tpu votes sockets slice
    pub fn votes(&self) -> &[UdpSocket] {
        &self.votes
    }
}

#[cfg(test)]
mod tests {
    use {
        crate::sockets::{Tpu, Tvu},
        solana_net_utils::sockets::{
            bind_to, bind_to_localhost_unique, unique_port_range_for_tests,
        },
        std::net::{IpAddr, Ipv4Addr, UdpSocket},
    };

    fn assert_sockets_range(num_ports: u16, ip_addr: IpAddr, sockets: &[UdpSocket]) {
        assert_eq!(
            num_ports as usize,
            sockets.len(),
            "number of ports and sockets must match"
        );
        for socket in sockets {
            let addr = socket.local_addr().unwrap();
            assert_eq!(ip_addr, addr.ip());
        }
    }

    fn vec_sockets_from_size_and_addr(size: u16, ip_addr: IpAddr) -> Vec<UdpSocket> {
        let mut sockets = vec![];
        unique_port_range_for_tests(size)
            .clone()
            .into_iter()
            .for_each(|port| {
                sockets.push(
                    bind_to(ip_addr, port).expect(&format!("should bind - sockets: {:?}", port)),
                );
            });
        sockets
    }

    #[test]
    fn test_new_tvu_verify_outcome() {
        const NUM_PORTS: u16 = 3;
        const IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let quic_socket = bind_to_localhost_unique().expect("should bind - quic port");
        let sockets = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let tvu_group = Tvu::new(sockets, quic_socket);

        assert_eq!(IP_ADDR, tvu_group.quic_socket().local_addr().unwrap().ip());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tvu_group.sockets());
    }

    #[test]
    fn test_new_tpu_verify_outcome() {
        const NUM_PORTS: u16 = 3;
        const IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let sockets = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let forwards = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let votes = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let tpu_group = Tpu::new(sockets, forwards, votes);

        assert_sockets_range(NUM_PORTS, IP_ADDR, tpu_group.sockets());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tpu_group.forwards());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tpu_group.votes());
    }
}
