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

#[cfg(test)]
mod tests {
    use {
        crate::sockets::Tvu,
        solana_net_utils::sockets::{
            bind_to, bind_to_localhost_unique, unique_port_range_for_tests,
        },
        std::net::{IpAddr, Ipv4Addr},
    };

    #[test]
    fn test_new_tvu_verify_outcome() {
        const NUM_PORTS: usize = 3;
        const IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let mut sockets = vec![];
        let quic_socket = bind_to_localhost_unique().expect("should bind - quic port");
        let port_range = unique_port_range_for_tests(NUM_PORTS as u16);

        port_range.clone().into_iter().for_each(|port| {
            sockets
                .push(bind_to(IP_ADDR, port).expect(&format!("should bind - sockets: {:?}", port)));
        });

        let tvu_group = Tvu::new(sockets, quic_socket);
        assert_eq!(NUM_PORTS, tvu_group.sockets.len());
        assert_eq!(IP_ADDR, tvu_group.quic_socket().local_addr().unwrap().ip());

        for socket in tvu_group.sockets() {
            let addr = socket.local_addr().unwrap();
            assert_eq!(IP_ADDR, addr.ip());
            assert!(
                port_range.clone().contains(&addr.port()),
                "socket port {} not in reserved range {:?}",
                addr.port(),
                port_range
            );
        }
    }
}
