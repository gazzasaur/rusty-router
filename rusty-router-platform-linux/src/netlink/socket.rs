use async_trait::async_trait;
use log::{error, warn};

use rand::Rng;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use netlink_packet_core::{self, NetlinkMessage, NLM_F_DUMP, NLM_F_REQUEST};
use netlink_packet_route;

use netlink_sys::{protocols::NETLINK_ROUTE, AsyncSocket, AsyncSocketExt, SocketAddr, TokioSocket};

use rusty_router_common::prelude::*;

#[cfg(test)]
use mockall::predicate::*;
#[cfg(test)]
use mockall::*;

// This package is the wrapper interface around the kernel.
// This should be kept as thin as possible as it it integ tested but not unit tested.
// The code used was provided from the Rust Netlink package.

#[derive(thiserror::Error, Debug)]
pub enum RecvLoopError {
    #[error("Timeout waiting on data from socket")]
    RecvLoopTimeout,

    #[error("Io Error: {0} - {1}")]
    Io(#[source] std::io::Error, String),
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait NetlinkSocketListener {
    async fn message_received(
        &self,
        message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>,
    );
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait NetlinkSocketFactory {
    async fn create_socket(
        &self,
        listener: Box<dyn NetlinkSocketListener + Send + Sync>,
    ) -> Result<Arc<dyn NetlinkSocket + Send + Sync>>;
}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait NetlinkSocket {
    async fn send_message(
        &self,
        message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>,
    ) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>>;
}

pub struct LogOnlyNetlinkSocketListener {}
impl LogOnlyNetlinkSocketListener {
    pub fn new() -> LogOnlyNetlinkSocketListener {
        LogOnlyNetlinkSocketListener {}
    }
}
#[async_trait]
impl NetlinkSocketListener for LogOnlyNetlinkSocketListener {
    async fn message_received(
        &self,
        message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>,
    ) {
        warn!("{:?}", message);
    }
}

pub struct DefaultNetlinkSocketFactory {}
impl DefaultNetlinkSocketFactory {
    pub fn new() -> DefaultNetlinkSocketFactory {
        DefaultNetlinkSocketFactory {}
    }
}
#[async_trait]
impl NetlinkSocketFactory for DefaultNetlinkSocketFactory {
    async fn create_socket(
        &self,
        listener: Box<dyn NetlinkSocketListener + Send + Sync>,
    ) -> Result<Arc<dyn NetlinkSocket + Send + Sync>> {
        Ok(Arc::new(DefaultNetlinkSocket::new(listener)?))
    }
}

pub enum RecvState {
    Received,
    Timeout,
}

pub struct DefaultNetlinkSocket {
    running: Arc<AtomicBool>,
    socket: Arc<Mutex<TokioSocket>>,
}
impl DefaultNetlinkSocket {
    pub fn new(
        listener: Box<dyn NetlinkSocketListener + Send + Sync>,
    ) -> Result<DefaultNetlinkSocket> {
        let mut socket = TokioSocket::new(NETLINK_ROUTE)
            .map_err(|e| Error::Io(e, "New NETLINK_ROUTE".into()))?;
        socket
            .socket_mut()
            .bind_auto()
            .map_err(|e| Error::Io(e, "Bind NETLINK_ROUTE".into()))?;
        socket
            .socket_ref()
            .connect(&SocketAddr::new(0, 0))
            .map_err(|e| Error::Io(e, "Connect NETLINK_ROUTE".into()))?;

        let mut multicast_socket = TokioSocket::new(NETLINK_ROUTE)
            .map_err(|e| Error::Io(e, "New NETLINK_ROUTE".into()))?;
        multicast_socket
            .socket_mut()
            .bind(&SocketAddr::new(0, 0xFFFF))
            .map_err(|e| Error::Io(e, "Bind NETLINK_ROUTE".into()))?;
        multicast_socket
            .socket_ref()
            .connect(&SocketAddr::new(0, 0))
            .map_err(|e| Error::Io(e, "Connect NETLINK_ROUTE".into()))?;

        let running = Arc::new(AtomicBool::new(true));
        let task_running = running.clone();
        tokio::task::spawn(async move {
            DefaultNetlinkSocket::recv_multicast_messages(
                task_running,
                listener,
                Arc::new(Mutex::new(multicast_socket)),
            )
            .await;
        });
        Ok(DefaultNetlinkSocket {
            running,
            socket: Arc::new(Mutex::new(socket)),
        })
    }

    async fn recv_multicast_messages(
        running: Arc<AtomicBool>,
        listener: Box<dyn NetlinkSocketListener + Send + Sync>,
        multicast_socket: Arc<Mutex<TokioSocket>>,
    ) {
        // let mut receive_buffer = vec![0; (2 as usize).pow(16)];

        while running.load(Ordering::SeqCst) {
            let mut lock = multicast_socket.lock().await;
            // let data_future = lock.recv(&mut receive_buffer[..]);

            let mut messages = Vec::new();
            match DefaultNetlinkSocket::recv_loop(None, &mut lock, |packet| {
                messages.push(packet);
            })
            .await
            {
                Ok(_) => (),
                Err(e) => error!("Failed to fetch data from multicast socket: {:?}", e),
            };
            for message in messages {
                listener.message_received(message).await;
            }
        }
    }

     // TODO This is a little dangerous. Need to prevent infinite loops.
    async fn recv_loop(
        sequence_number: Option<u32>,
        socket: &mut TokioSocket,
        mut callback: impl FnMut(
            netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>,
        ) -> (),
    ) -> Result<RecvState> {
        loop {
            let recv_result = if let Ok(recv_result) = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                socket.recv_from_full(),
            )
            .await
            {
                recv_result
            } else {
                return Ok(RecvState::Timeout);
            };

            let (receive_buffer, _) = if let Ok(recv_result) = recv_result {
                recv_result
            } else {
                continue;
            };

            let mut offset = 0u32;
            loop {
                let rx_packet: netlink_packet_core::NetlinkMessage<
                    netlink_packet_route::RtnlMessage,
                > = NetlinkMessage::deserialize(&receive_buffer[(offset as usize)..]).map_err(|e| {
                    Error::Communication(format!("Failed to deserialize netlink message: {:?}", e))
                })?;
                let packet_length = rx_packet.header.length;
                let packet_sequence_number = rx_packet.header.sequence_number;

                if sequence_number
                    .map(|sequence_number| packet_sequence_number == sequence_number)
                    .unwrap_or(true)
                {
                    if let netlink_packet_core::NetlinkPayload::Done(_) = rx_packet.payload {
                        return Ok(RecvState::Received);
                    }
                    callback(rx_packet);
                }

                offset += packet_length;
                if (offset as usize) == receive_buffer.len() {
                    break;
                }
            }
        }
    }
}
#[async_trait]
impl NetlinkSocket for DefaultNetlinkSocket {
    async fn send_message(
        &self,
        message: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>,
    ) -> Result<Vec<netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage>>> {
        let sequence_number = message.header.sequence_number;
        let socket = self.socket.clone();
        let mut buf = vec![0; message.header.length as usize];
        message.serialize(&mut buf[..]);
        let mut socket = socket.lock().await;
        socket
            .send(&buf[..])
            .await
            .map_err(|e| Error::Io(e, "Netlink Socket Send".into()))?;

        let mut received_messages = Vec::new();
        DefaultNetlinkSocket::recv_loop(Some(sequence_number), &mut socket, |rx_packet| {
            received_messages.push(rx_packet);
        })
        .await?;
        Ok(received_messages)
    }
}
impl Drop for DefaultNetlinkSocket {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

pub fn build_default_packet(
    message: netlink_packet_route::RtnlMessage,
) -> netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> {
    let mut packet = netlink_packet_core::NetlinkMessage::from(message);
    packet.header.flags = NLM_F_DUMP | NLM_F_REQUEST;
    packet.header.sequence_number = rand::thread_rng().gen();
    packet.finalize();

    return packet;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_socket() -> Result<()> {
        let factory = DefaultNetlinkSocketFactory::new();
        let socket = factory
            .create_socket(Box::new(LogOnlyNetlinkSocketListener::new()))
            .await?;
        let link_message = netlink_packet_route::RtnlMessage::GetLink(
            netlink_packet_route::LinkMessage::default(),
        );
        let packet: netlink_packet_core::NetlinkMessage<netlink_packet_route::RtnlMessage> =
            build_default_packet(link_message);
        let messages = socket.send_message(packet).await?;

        let mut loopback_found = false;
        for message in messages {
            if let netlink_packet_core::NetlinkPayload::InnerMessage(
                netlink_packet_route::RtnlMessage::NewLink(msg),
            ) = message.payload
            {
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
