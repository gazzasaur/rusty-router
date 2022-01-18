use log::{warn, error};
use std::{error::Error, ffi::OsString, net::Ipv4Addr, sync::Arc};
use nix::{errno::Errno, sys::socket::{InetAddr, IpAddr, IpMembershipRequest, SockAddr}, unistd::close};
use tokio::sync::RwLock;
use async_trait::async_trait;
use std::os::unix::io::RawFd;
use std::collections::HashMap;
use nix::sys::socket::MsgFlags;
use rusty_router_model::{InetPacketNetworkInterface, NetworkEventHandler};
use std::sync::atomic::{AtomicBool, Ordering};
use nix::sys::epoll::{EpollCreateFlags, EpollEvent, EpollFlags, EpollOp, epoll_create1, epoll_ctl, epoll_wait};

pub struct LinuxInetPacketNetworkInterface {
    _ticket: PollerTicket, // Need to store this as it must have the same scope as the container.
    sock: Arc<AutoCloseFd>,
}
impl LinuxInetPacketNetworkInterface {
    pub async fn new(network_device: String, protocol: i32, multicast_groups: Vec<Ipv4Addr>, handler: Box<dyn NetworkEventHandler + Send + Sync>, poller: &Poller) -> Result<LinuxInetPacketNetworkInterface, Box<dyn Error + Send + Sync>> {
        let sock = Arc::from(AutoCloseFd::new(Errno::result(unsafe { libc::socket(libc::AF_INET, libc::O_NONBLOCK | libc::SOCK_RAW, protocol) })?));
        for multicast_group in multicast_groups {
            nix::sys::socket::setsockopt(sock.get(), nix::sys::socket::sockopt::IpAddMembership, &IpMembershipRequest::new(nix::sys::socket::Ipv4Addr::from_std(&multicast_group), None))?;
        }
        nix::sys::socket::setsockopt(sock.get(), nix::sys::socket::sockopt::BindToDevice, &OsString::from(network_device))?;

        let real: Arc<Box<dyn PollerListener + Send + Sync>> = Arc::new(Box::new(LinuxInetPacketNetworkInterfacePollerListener::new(sock.clone(), handler).await?));
        let poller_ticket = poller.add_item(sock.get(), real).await?;

        Ok(LinuxInetPacketNetworkInterface { sock: sock, _ticket: poller_ticket })
    }
}
#[async_trait]
impl InetPacketNetworkInterface for LinuxInetPacketNetworkInterface {
    async fn send(&self, to: std::net::Ipv4Addr, data: Vec<u8>) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        Ok(nix::sys::socket::sendto(self.sock.get(), &data[..], &SockAddr::Inet(InetAddr::new(IpAddr::from_std(&std::net::IpAddr::V4(to)), 0)), MsgFlags::empty())?)
    }
}
pub struct LinuxInetPacketNetworkInterfacePollerListener {
    sock: Arc<AutoCloseFd>,
    handler: Arc<Box<dyn NetworkEventHandler + Send + Sync>>,
}
impl LinuxInetPacketNetworkInterfacePollerListener {
    pub async fn new(sock: Arc<AutoCloseFd>, handler: Box<dyn NetworkEventHandler + Send + Sync>) -> Result<LinuxInetPacketNetworkInterfacePollerListener, Box<dyn Error + Send + Sync>> {
        let handler = Arc::new(handler);
        Ok(LinuxInetPacketNetworkInterfacePollerListener { sock, handler })
    }
}
#[async_trait]
impl PollerListener for LinuxInetPacketNetworkInterfacePollerListener {
    async fn recv(&self) {
        let mut buffer = [0 as u8; 65535];
        loop {
            match nix::sys::socket::recv(self.sock.get(), &mut buffer, MsgFlags::empty()) {
                Ok(size) => self.handler.on_recv(buffer[0..size].to_vec()).await,
                Err(nix::Error::Sys(nix::errno::Errno::EAGAIN)) => return,
                Err(nix::Error::Sys(errno)) => self.handler.on_error(format!("Failed to read from socket: {}", errno)).await,
                Err(e) => {
                    error!("Failed to read from socket: {:?}", e)
                },
            }
        }
    }
}

pub struct AutoCloseFd {
    fd: RawFd
}
impl AutoCloseFd {
    fn new(fd: RawFd) -> AutoCloseFd {
        AutoCloseFd { fd }
    }

    fn get(&self) -> i32 {
        self.fd
    }
}
impl Drop for AutoCloseFd {
    fn drop(&mut self) {
        if let Err(error) = close(self.fd) {
            warn!("Failed to close fd: {}", error);
        }
    }
}

#[async_trait]
pub trait PollerListener {
    async fn recv(&self);
}

pub struct PollerTicket {
    handler_id: u64,
    handlers: Arc<RwLock<HashMap<u64, Arc<Box<dyn PollerListener + Send + Sync>>>>>,
}
impl PollerTicket {
    pub fn new(handler_id: u64, handlers: Arc<RwLock<HashMap<u64, Arc<Box<dyn PollerListener + Send + Sync>>>>>) -> PollerTicket {
        PollerTicket { handler_id, handlers }
    }
}
impl Drop for PollerTicket {
    fn drop(&mut self) {
        let handler_id = self.handler_id;
        let handlers = self.handlers.clone();
        tokio::task::spawn(async move {
            handlers.write().await.remove(&handler_id);
        });
    }
}

pub struct Poller {
    epoll_fd: RawFd,
    poller_running: Arc<AtomicBool>,
    controller_running: Arc<AtomicBool>,
    handlers: Arc<RwLock<HashMap<u64, Arc<Box<dyn PollerListener + Send + Sync>>>>>,
}
impl Poller {
    pub fn new() -> Result<Poller, Box<dyn std::error::Error + Send + Sync>> {
        let poller_running = Arc::new(AtomicBool::new(true));
        let controller_running = Arc::new(AtomicBool::new(true));

        let handlers = Arc::new(RwLock::new(HashMap::new()));
        let task_handlers = handlers.clone();
        let epoll_fd = epoll_create1(EpollCreateFlags::empty())?;

        let poller = Poller {
            epoll_fd,
            poller_running: poller_running.clone(),
            controller_running: controller_running.clone(),
            handlers,
        };

        tokio::task::spawn_blocking(move || {
            let epoll_fd = epoll_fd;
            let poller_running = poller_running;
            let controller_running = controller_running;
            Poller::poller_task(epoll_fd, poller_running, controller_running, task_handlers);
        });
        Ok(poller)
    }

    fn poller_task(epoll_fd: RawFd, poller_running: Arc<AtomicBool>, controller_running: Arc<AtomicBool>, handlers: Arc<RwLock<HashMap<u64, Arc<Box<dyn PollerListener + Send + Sync>>>>>) {
        // The buffer size was set to 100 after testing on machines that have between 3 and 16 cores with ~1000 sockets.
        let mut buffer = [EpollEvent::empty(); 100];

        while poller_running.load(Ordering::SeqCst) && controller_running.load(Ordering::SeqCst) {
            if let Err(error) = Poller::poller_process_messages(epoll_fd, &mut buffer, &handlers) {
                error!("Poller task failed: {}", error);
                poller_running.store(false, Ordering::SeqCst);
            }
        }
        poller_running.store(false, Ordering::SeqCst);
    }

    fn poller_process_messages(epoll_fd: RawFd, buffer: &mut [EpollEvent; 100], handlers: &Arc<RwLock<HashMap<u64, Arc<Box<dyn PollerListener + Send + Sync>>>>>) -> Result<(), Box<dyn std::error::Error>> {
        let size = epoll_wait(epoll_fd, buffer, 100)?;
        if size != 0 {
            // Cloning this to allow epoll to continue.  Edge triggering will prevent a socket from being read across multiple threads.
            // The handler may choose to implement it's own synchronization strategy.
            let buffer = buffer.clone();
            let handlers = handlers.clone();
            tokio::task::spawn(async move {
                let handlers = handlers.read().await;
                for i in 0..size {
                    if buffer[i].events() & EpollFlags::EPOLLIN != EpollFlags::empty() {
                        if let Some(handler) = handlers.get(&buffer[i].data()) {
                            let handler = handler.clone();
                            tokio::task::spawn(async move { handler.recv().await });
                        };
                    };
                };
            });
        }
        Ok(())
    }

    pub async fn add_item(&self, fd: RawFd, socket_handler: Arc<Box<dyn PollerListener + Send + Sync>>) -> Result<PollerTicket, Box<dyn std::error::Error + Send + Sync>> {
        let epoll_fd = self.epoll_fd;
        let handlers = self.handlers.clone();

        let handle_id: u64 = rand::random();
        let poller_item = PollerTicket::new(handle_id, handlers.clone());
        let mut event = EpollEvent::new(EpollFlags::EPOLLIN | EpollFlags::EPOLLHUP | EpollFlags::EPOLLET, handle_id);
        epoll_ctl(epoll_fd, EpollOp::EpollCtlAdd, fd, Some(&mut event))?;

        let mut handlers = handlers.write().await;
        handlers.insert(handle_id, socket_handler);
        Ok(poller_item)
    }
}
impl Drop for Poller {
    fn drop(&mut self) {
        if let Err(error) = close(self.epoll_fd) {
            warn!("Failed to close epoll fd: {}", error);
        }
        self.poller_running.store(false, Ordering::SeqCst);
        self.controller_running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    
    use rand::Rng;
    use nix::sys::socket;
    use async_trait::async_trait;

    struct EchoCallback {
        fd: super::RawFd,
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl EchoCallback {
        pub fn new(fd: super::RawFd, count: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> EchoCallback { EchoCallback { fd, count } }
    }
    #[async_trait]
    impl PollerListener for EchoCallback {
        async fn recv(&self) {
            let fd = self.fd;
            let mut buffer = [0 as u8; 65535];
            loop {
                match nix::sys::socket::recv(fd, &mut buffer, MsgFlags::empty()) {
                    Ok(_) => {
                        if self.count.load(std::sync::atomic::Ordering::SeqCst) > 100000000 {
                            return;
                        }
                        let mut rng = rand::thread_rng();
                        self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let data: String = std::iter::repeat(()).map(|()| rng.sample(rand::distributions::Alphanumeric)).map(char::from).take(128).collect();
                        nix::sys::socket::send(self.fd, data.as_bytes(), super::MsgFlags::empty()).expect("Failed to send data.");

                        // self.socket_handler.on_recv(buffer[0..size].to_vec()).await
                    },
                    Err(nix::Error::Sys(nix::errno::Errno::EAGAIN)) => return,
                    Err(nix::Error::Sys(errno)) => println!("Failed to read from socket: {}", errno),
                    Err(e) => {
                        error!("Failed to read from socket: {:?}", e)
                    },
                }
            }    
        }
    }

    #[tokio::test]
    async fn test_poll() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let subject = std::sync::Arc::new(tokio::sync::Mutex::new(super::Poller::new()?));
        let items = std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new()));
        println!("Creating Sockets");
        for _ in (0 as i32)..(100 as i32) {
            let sockets = socket::socketpair(socket::AddressFamily::Unix, socket::SockType::Stream, None, socket::SockFlag::SOCK_NONBLOCK)?;

            let subject = subject.lock().await;
            let mut items = items.write().await;
            items.push(subject.add_item(sockets.0, Arc::new(Box::new(EchoCallback::new(sockets.0, count.clone())))).await);
            items.push(subject.add_item(sockets.1, Arc::new(Box::new(EchoCallback::new(sockets.1, count.clone())))).await);

            nix::sys::socket::send(sockets.0, "Hello".to_string().as_bytes(), super::MsgFlags::empty()).expect("Failed to send data.");
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        println!("Sinking Data");

        tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;
        println!("Creating Data");
        println!("{:?}", chrono::offset::Utc::now());

        println!("Waiting");
        println!("{:?}", chrono::offset::Utc::now());
        
        tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
        println!("Send Executed: {}", count.load(Ordering::SeqCst));
        println!("{:?}", chrono::offset::Utc::now());

        Ok(())
    }
}
