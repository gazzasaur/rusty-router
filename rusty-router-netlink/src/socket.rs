use mockall::*;
use mockall::predicate::*;

use std::sync::Arc;
use std::error::Error;

use netlink_sys;
use netlink_packet_core;
use netlink_packet_route;
use netlink_sys::protocols;

// This package is the wrapper interface around the kernel.
// This should be kept as thin as possible as it it integ tested but not unit tested.
// The code used was provided from the Rust Netlink package.

#[automock]
pub trait NetlinkSocketListener {
    fn message_received(&self, callback: fn(netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> ());
}

#[automock]
pub trait NetlinkSocket {
    fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error>>;
}

pub struct DefaultNetlinkSocket {
    socket: netlink_sys::Socket,
    change_socket: Arc<netlink_sys::Socket>,
    callback: fn (netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> (),
}
impl DefaultNetlinkSocket {
    pub fn new() -> Result<DefaultNetlinkSocket, Box<dyn Error>> {
        let mut socket = netlink_sys::Socket::new(protocols::NETLINK_ROUTE)?;
        socket.bind_auto()?;
        socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;

        let mut change_socket = netlink_sys::Socket::new(protocols::NETLINK_ROUTE)?;
        change_socket.bind(&netlink_sys::SocketAddr::new(0, 0x11))?;
        socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;

        Ok(DefaultNetlinkSocket { socket, change_socket: Arc::new(change_socket), callback: |_| () })
    }

    pub fn receive_messages(&self) -> Result<(), Box<dyn Error>> {
        self.process_receive_messages(&self.change_socket, &self.callback)
    }

    fn process_receive_messages(&self, socket: &netlink_sys::Socket, mut callback: impl FnMut(netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> ()) -> Result<(), Box<dyn Error>> {
        let mut receive_buffer = vec![0; (2 as usize).pow(16)];
        
        loop {
            let mut offset = 0;
            let size = socket.recv(&mut receive_buffer[..], 0)?;

            loop {
                let bytes = &receive_buffer[offset..];
                let rx_packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = netlink_packet_route::NetlinkMessage::deserialize(bytes)?;
                let header_length = rx_packet.header.length as usize;

                if rx_packet.payload == netlink_packet_core::NetlinkPayload::Done {
                    return Ok(());
                }
                callback(rx_packet);

                offset += header_length;
                if offset == size || header_length == 0 {
                    break;
                }
            }
        }
    }
}
impl NetlinkSocket for DefaultNetlinkSocket {
    fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error>> {
        let mut buf = vec![0; message.header.length as usize];
        message.serialize(&mut buf[..]);
        self.socket.send(&buf[..], 0)?;

        let mut received_messages = Vec::new();
        self.process_receive_messages(&self.socket, |rx_packet| {
            &received_messages.push(rx_packet);
        })?;
        Ok(received_messages)
    }
}
