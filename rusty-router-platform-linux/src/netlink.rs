use log::{error, warn};
use mockall::*;
use mockall::predicate::*;

use async_trait::async_trait;

use rand::Rng;
use std::sync::Arc;
use std::error::Error;
use tokio::{sync::Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use netlink_sys;
use netlink_packet_core::{self, DecodeError};
use netlink_packet_route;
use netlink_sys::protocols;
use netlink_packet_route::constants;

// This package is the wrapper interface around the kernel.
// This should be kept as thin as possible as it it integ tested but not unit tested.
// The code used was provided from the Rust Netlink package.

#[automock]
#[async_trait]
pub trait NetlinkSocket {
    async fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> std::result::Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error>>;
}

pub struct DefaultNetlinkSocket {
    running: Arc<AtomicBool>,
    socket: Arc<Mutex<netlink_sys::TokioSocket>>,
}
impl DefaultNetlinkSocket {
    pub fn new() -> Result<DefaultNetlinkSocket, Box<dyn Error>> {
        let mut socket = netlink_sys::TokioSocket::new(protocols::NETLINK_ROUTE)?;
        socket.bind_auto()?;
        socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;

        let mut multicast_socket = netlink_sys::TokioSocket::new(protocols::NETLINK_ROUTE)?;
        multicast_socket.bind(&netlink_sys::SocketAddr::new(0, 0x11))?;
        multicast_socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;

        let running = Arc::new(AtomicBool::new(true));
        let task_running = running.clone();
        tokio::task::spawn(async move {
            DefaultNetlinkSocket::recv_multicast_messages(task_running, Arc::new(Mutex::new(multicast_socket))).await;
        });
        Ok(DefaultNetlinkSocket { running, socket: Arc::new(Mutex::new(socket)) })
    }

    async fn recv_multicast_messages(running: Arc<AtomicBool>, multicast_socket: Arc<Mutex<netlink_sys::TokioSocket>>) {
        let mut receive_buffer = vec![0; (2 as usize).pow(16)];

        while running.load(Ordering::SeqCst) {
            let mut lock = multicast_socket.lock().await;
            let data_future = lock.recv(&mut receive_buffer[..]);
            // Timeout is expected from the multicast socket.
            if let Ok(value) = tokio::time::timeout(std::time::Duration::from_millis(1000), data_future).await {
                match value {
                    Ok(data) => {
                        let res: std::result::Result<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>, DecodeError> = netlink_packet_route::NetlinkMessage::deserialize(&receive_buffer[..data]);
                        if let Ok(rx_packet) = res {
                            error!("Failed to read from netlink multicast socket {:?}", rx_packet);
                        }
                    },
                    Err(e) => error!("Failed to fetch data from multicast socket: {:?}", e),
                }
            }
        }
    }

    async fn recv_loop(sequence_number: u32, socket: &mut netlink_sys::TokioSocket, mut callback: impl FnMut(netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> ()) -> Result<(), Box<dyn Error>> {
        // Allocating 32k of memory each call.  This could be passed at the cost of locking.
        let mut receive_buffer = vec![0; (2 as usize).pow(16)];
        
        // It is possible that we filled the buffer but there is more to read.
        loop {
            let mut offset = 0;
            let size = match tokio::time::timeout(std::time::Duration::from_millis(1000), socket.recv(&mut receive_buffer[..])).await? {
                Ok(size) => size,
                Err(_) => 0 as usize,
            };

            // It is possible that we have many messages in our buffer.  Process all of them.
            loop {
                let bytes = &receive_buffer[offset..];
                let rx_packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = netlink_packet_route::NetlinkMessage::deserialize(bytes)?;
                let packet_length = rx_packet.header.length as usize;
                let packet_sequence_number = rx_packet.header.sequence_number;
    
                if packet_sequence_number == sequence_number {
                    if rx_packet.payload == netlink_packet_core::NetlinkPayload::Done {
                        return Ok(());
                    }
                    callback(rx_packet);
                }
    
                offset += packet_length;
                if offset == size || packet_length == 0 {
                    break;
                } else if offset > size {
                    warn!("Netlink offset exceeds the packet size.");
                }
            }
        }
    }
}
#[async_trait]
impl NetlinkSocket for DefaultNetlinkSocket {
    async fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error>> {
        let sequence_number = message.header.sequence_number;
        let socket = self.socket.clone();
        let mut buf = vec![0; message.header.length as usize];
        message.serialize(&mut buf[..]);
        let mut socket = socket.lock().await;
        socket.send(&buf[..]).await?;
    
        let mut received_messages = Vec::new();
        DefaultNetlinkSocket::recv_loop(sequence_number, &mut socket, |rx_packet| {
             &received_messages.push(rx_packet);
        }).await?;
        Ok(received_messages)
    }
}
impl Drop for DefaultNetlinkSocket {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

pub fn build_default_packet(message: netlink_packet_route::RtnlMessage) -> netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> {
    let mut packet = netlink_packet_core::NetlinkMessage {
        header: netlink_packet_core::NetlinkHeader::default(),
        payload: netlink_packet_core::NetlinkPayload::from(message),
    };
    packet.header.flags = constants::NLM_F_DUMP | constants::NLM_F_REQUEST;
    packet.header.sequence_number = rand::thread_rng().gen();
    packet.finalize();

    return packet;
}
