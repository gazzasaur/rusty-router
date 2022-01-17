use log::{error, warn};
use mockall::*;
use mockall::predicate::*;

use async_trait::async_trait;

use rand::Rng;
use std::sync::Arc;
use std::error::Error;
use tokio::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use netlink_sys;
use netlink_packet_core::{self, DecodeError};
use netlink_packet_route;
use netlink_sys::protocols;
use netlink_packet_route::constants;

// This package is the wrapper interface around the kernel.
// This should be kept as thin as possible as it it integ tested but not unit tested.
// The code used was provided from the Rust Netlink package.

#[derive(thiserror::Error, Debug)]
pub enum RecvLoopError {
    #[error("Timeout waiting on data from socket")]
    RecvLoopTimeout,
    #[error("Error while receiving netlink payload {0}")]
    DeserializeError(DecodeError),
}

#[automock]
#[async_trait]
pub trait NetlinkSocketListener {
    async fn message_received(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>);
}

#[automock]
#[async_trait]
pub trait NetlinkSocketFactory {
    async fn create_socket(&self, listener: Box<dyn NetlinkSocketListener + Send + Sync>) -> std::result::Result<Arc<dyn NetlinkSocket + Send + Sync>, Box<dyn Error + Send + Sync>>;
}

#[automock]
#[async_trait]
pub trait NetlinkSocket {
    async fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> std::result::Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error + Send + Sync>>;
}

pub struct LogOnlyNetlinkSocketListener {
}
impl LogOnlyNetlinkSocketListener {
    pub fn new() -> LogOnlyNetlinkSocketListener {
        LogOnlyNetlinkSocketListener { }
    }
}
#[async_trait]
impl NetlinkSocketListener for LogOnlyNetlinkSocketListener {
    async fn message_received(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) {
        warn!("{:?}", message);
    }
}

pub struct DefaultNetlinkSocketFactory {
}
impl DefaultNetlinkSocketFactory {
    pub fn new() -> DefaultNetlinkSocketFactory {
        DefaultNetlinkSocketFactory { }
    }
}
#[async_trait]
impl NetlinkSocketFactory for DefaultNetlinkSocketFactory {
    async fn create_socket(&self, listener: Box<dyn NetlinkSocketListener + Send + Sync>) -> std::result::Result<Arc<dyn NetlinkSocket + Send + Sync>, Box<dyn Error + Send + Sync>> {
        Ok(Arc::new(DefaultNetlinkSocket::new(listener)?))
    }
}

pub struct DefaultNetlinkSocket {
    running: Arc<AtomicBool>,
    socket: Arc<Mutex<netlink_sys::TokioSocket>>,
}
impl DefaultNetlinkSocket {
    pub fn new(listener: Box<dyn NetlinkSocketListener + Send + Sync>) -> Result<DefaultNetlinkSocket, Box<dyn Error + Send + Sync>> {
        let mut socket = netlink_sys::TokioSocket::new(protocols::NETLINK_ROUTE)?;
        socket.bind_auto()?;
        socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;

        let mut multicast_socket = netlink_sys::TokioSocket::new(protocols::NETLINK_ROUTE)?;
        multicast_socket.bind(&netlink_sys::SocketAddr::new(0, 0xFFFF))?;
        multicast_socket.connect(&netlink_sys::SocketAddr::new(0, 0))?;

        let running = Arc::new(AtomicBool::new(true));
        let task_running = running.clone();
        tokio::task::spawn(async move {
            DefaultNetlinkSocket::recv_multicast_messages(task_running, listener, Arc::new(Mutex::new(multicast_socket))).await;
        });
        Ok(DefaultNetlinkSocket { running, socket: Arc::new(Mutex::new(socket)) })
    }

    async fn recv_multicast_messages(running: Arc<AtomicBool>, listener: Box<dyn NetlinkSocketListener + Send + Sync>, multicast_socket: Arc<Mutex<netlink_sys::TokioSocket>>) {
        // let mut receive_buffer = vec![0; (2 as usize).pow(16)];

        while running.load(Ordering::SeqCst) {
            let mut lock = multicast_socket.lock().await;
            // let data_future = lock.recv(&mut receive_buffer[..]);

            let mut messages = Vec::new();
            match DefaultNetlinkSocket::recv_loop(None, &false, &mut lock, |packet| {
                messages.push(packet);
            }).await {
                Ok(()) => (),
                Err(RecvLoopError::RecvLoopTimeout) => (),
                Err(e) => error!("Failed to fetch data from multicast socket: {:?}", e),
            };
            for message in messages {
                listener.message_received(message).await;
            }
        }
    }

    /// Wait until done should typically be true unless using multicast sockets.
    async fn recv_loop(sequence_number: Option<u32>, wait_until_done: &bool, socket: &mut netlink_sys::TokioSocket, mut callback: impl FnMut(netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> ()) -> Result<(), RecvLoopError> {
        // Allocating 32k of memory each call.  This could be passed at the cost of locking.
        let mut receive_buffer = vec![0; (2 as usize).pow(16)];
        
        // It is possible that we filled the buffer but there is more to read.
        loop {
            let mut offset = 0;
            let size = match tokio::time::timeout(std::time::Duration::from_millis(1000), socket.recv(&mut receive_buffer[..])).await.map_err(|_| RecvLoopError::RecvLoopTimeout)? {
                Ok(size) => size,
                Err(_) => 0 as usize,
            };

            // It is possible that we have many messages in our buffer.  Process all of them.
            loop {
                let bytes = &receive_buffer[offset..];
                let rx_packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = netlink_packet_route::NetlinkMessage::deserialize(bytes).map_err(|e| RecvLoopError::DeserializeError(e))?;
                let packet_length = rx_packet.header.length as usize;
                let packet_sequence_number = rx_packet.header.sequence_number;
    
                if sequence_number.map(|sequence_number| packet_sequence_number == sequence_number).unwrap_or(true) {
                    if rx_packet.payload == netlink_packet_core::NetlinkPayload::Done {
                        return Ok(());
                    }
                    callback(rx_packet);
                }
    
                offset += packet_length;
                if (offset == size || packet_length == 0) && !wait_until_done  {
                    return Ok(());
                } else if offset == size || packet_length == 0 {
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
    async fn send_message(&self, message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>, Box<dyn Error + Send + Sync>> {
        let sequence_number = message.header.sequence_number;
        let socket = self.socket.clone();
        let mut buf = vec![0; message.header.length as usize];
        message.serialize(&mut buf[..]);
        let mut socket = socket.lock().await;
        socket.send(&buf[..]).await?;
    
        let mut received_messages = Vec::new();
        DefaultNetlinkSocket::recv_loop(Some(sequence_number), &true, &mut socket, |rx_packet| {
            received_messages.push(rx_packet);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[tokio::test]
    async fn test_default_socket() -> Result<(), Box<dyn Error + Send + Sync>> {
        let factory = DefaultNetlinkSocketFactory::new();
        let socket = factory.create_socket(Box::new(LogOnlyNetlinkSocketListener::new())).await?;
        let link_message = netlink_packet_route::RtnlMessage::GetLink(netlink_packet_route::LinkMessage::default());
        let packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> = build_default_packet(link_message);
        let messages = socket.send_message(packet).await?;

        let mut loopback_found = false;
        for message in messages {
            if let netlink_packet_core::NetlinkPayload::InnerMessage(netlink_packet_route::RtnlMessage::NewLink(msg)) = message.payload {
                for attribute in msg.nlas {
                    if let netlink_packet_route::rtnl::link::nlas::Nla::IfName(name) = attribute {
                        if name == "lo" {
                            loopback_found = true;
                        }
                    }
                }
            }
        }
        assert!(loopback_found);
        Ok(())
    }
}