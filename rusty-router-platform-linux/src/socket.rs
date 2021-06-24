use mockall::*;
use mockall::predicate::*;

use async_trait::async_trait;

use std::sync::Arc;
use std::error::Error;
use tokio::sync::Mutex;
use std::sync::atomic::AtomicBool;

use netlink_sys;
use netlink_packet_core;
use netlink_packet_route;
use netlink_sys::protocols;

// This package is the wrapper interface around the kernel.
// This should be kept as thin as possible as it it integ tested but not unit tested.
// The code used was provided from the Rust Netlink package.

#[automock]
#[async_trait]
pub trait NetlinkSocket {
    async fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error>>;
}

pub struct DefaultNetlinkSocket {
    _running: AtomicBool,
    socket: Arc<Mutex<netlink_sys::TokioSocket>>,
    _multicast_socket: Arc<Mutex<netlink_sys::TokioSocket>>
}
impl DefaultNetlinkSocket {
    pub fn new() -> Result<DefaultNetlinkSocket, Box<dyn Error>> {
        let mut socket = netlink_sys::TokioSocket::new(protocols::NETLINK_ROUTE)?;
        socket.bind_auto()?;
        socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;

        let mut multicast_socket = netlink_sys::TokioSocket::new(protocols::NETLINK_ROUTE)?;
        multicast_socket.bind(&netlink_sys::SocketAddr::new(0, 0x11))?;
        multicast_socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;

        Ok(DefaultNetlinkSocket { _running: AtomicBool::from(true), socket: Arc::new(Mutex::new(socket)), _multicast_socket: Arc::new(Mutex::new(multicast_socket))})
    }

    async fn process_receive_messages(socket: &mut netlink_sys::TokioSocket, mut callback: impl FnMut(netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> ()) -> Result<(), Box<dyn Error>> {
        let mut receive_buffer = vec![0; (2 as usize).pow(16)];
        
        loop {
            let mut offset = 0;
            let size = socket.recv(&mut receive_buffer[..]).await?;

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
#[async_trait]
impl NetlinkSocket for DefaultNetlinkSocket {
    async fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error>> {
        let socket = self.socket.clone();
        let mut buf = vec![0; message.header.length as usize];
        message.serialize(&mut buf[..]);
        let mut socket = socket.lock().await;
        socket.send(&buf[..]).await?;
    
        let mut received_messages = Vec::new();
        DefaultNetlinkSocket::process_receive_messages(&mut socket, |rx_packet| {
             &received_messages.push(rx_packet);
        }).await?;
        Ok(received_messages)
    }
}
