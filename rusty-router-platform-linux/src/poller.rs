use log::{warn, error};
use std::sync::Arc;
use nix::unistd::close;
use tokio::sync::RwLock;
use async_trait::async_trait;
use std::os::unix::io::RawFd;
use std::collections::HashMap;
use nix::sys::socket::MsgFlags;
use std::sync::atomic::{AtomicBool, Ordering};
use nix::sys::epoll::{EpollCreateFlags, EpollEvent, EpollFlags, EpollOp, epoll_create1, epoll_ctl, epoll_wait};

/**
 * Playing around with epoll (and other forms of IO) to find one that will fit this project.
 * I almost have the balance right.  Interesting points, the buffer is 65535 which is likely too large, the MTU size will do.
 * The error handling needs to be fixed up.
 */

struct AutoCloseFd {
    fd: RawFd,
}
impl AutoCloseFd {
    pub fn new(fd: RawFd) -> AutoCloseFd {
        AutoCloseFd { fd }
    }

    pub fn get(&self) -> &RawFd {
        &self.fd
    }
}
impl Drop for AutoCloseFd {
    fn drop(&mut self) {
        if let Err(error) = close(self.fd) {
            warn!("Failed to close epoll fd: {}", error);
        }
    }
}

#[async_trait]
pub trait SocketHandler {
    async fn on_recv(&self, data: Vec<u8>);
}

pub struct PollerItem {
    fd: Arc<AutoCloseFd>,
    socket_handler: Arc<Box<dyn SocketHandler + Send + Sync>>,
}
impl PollerItem {
    pub fn new(fd: RawFd, socket_handler: Box<dyn SocketHandler + Send + Sync>) -> PollerItem {
        PollerItem { fd: Arc::new(AutoCloseFd::new(fd)), socket_handler: Arc::new(socket_handler) }
    }

    pub fn get_fd(&self) -> &RawFd {
        self.fd.get()
    }

    pub fn send(&self, buffer: &Vec<u8>) -> Result<usize, Box<dyn std::error::Error>> {
        let fd = self.fd.clone();
        return Ok(nix::sys::socket::send(*fd.get(), &buffer[..], MsgFlags::empty())?);
    }

    pub async fn recv(&self) {
        let fd = *self.fd.get();
        let mut buffer = [0 as u8; 65535];
        loop {
            match nix::sys::socket::recv(fd, &mut buffer, MsgFlags::empty()) {
                Ok(size) => self.socket_handler.on_recv(buffer[0..size].to_vec()).await,
                Err(nix::Error::Sys(nix::errno::Errno::EAGAIN)) => return,
                Err(e) => error!("Failed to read from socket: {:?}", e),
            }
        }
    }
}

pub struct Poller {
    epoll_fd: Arc<AutoCloseFd>,
    poller_running: Arc<AtomicBool>,
    controller_running: Arc<AtomicBool>,
    handlers: Arc<RwLock<HashMap<u64, Arc<PollerItem>>>>,
}
impl Poller {
    pub fn new() -> Result<Poller, Box<dyn std::error::Error>> {
        let poller_running = Arc::new(AtomicBool::new(true));
        let controller_running = Arc::new(AtomicBool::new(true));
        let epoll_fd = Arc::new(AutoCloseFd::new(epoll_create1(EpollCreateFlags::empty())?));

        let handlers = Arc::new(RwLock::new(HashMap::new()));
        let task_handlers = handlers.clone();

        let poller = Poller {
            epoll_fd: epoll_fd.clone(),
            poller_running: poller_running.clone(),
            controller_running: controller_running.clone(),
            handlers,
        };
        tokio::task::spawn_blocking(move || {
            let epoll_fd = epoll_fd;
            let poller_running = poller_running;
            let controller_running = controller_running;
            Poller::poller_task(epoll_fd, poller_running, controller_running, task_handlers)
        });
        Ok(poller)
    }

    fn poller_task(epoll_fd: Arc<AutoCloseFd>, poller_running: Arc<AtomicBool>, controller_running: Arc<AtomicBool>, handlers: Arc<RwLock<HashMap<u64, Arc<PollerItem>>>>) {
        let epoll_fd = *epoll_fd.get();
        let mut buffer = [EpollEvent::empty(); 100];

        while poller_running.load(Ordering::SeqCst) && controller_running.load(Ordering::SeqCst) {
            if let Err(error) = Poller::poller_process_messages(epoll_fd, &mut buffer, &handlers) {
                error!("Poller task failed: {}", error);
                poller_running.store(false, Ordering::SeqCst);
            }
        }
        poller_running.store(false, Ordering::SeqCst);
    }

    fn poller_process_messages(epoll_fd: RawFd, buffer: &mut [EpollEvent; 100], handlers: &Arc<RwLock<HashMap<u64, Arc<PollerItem>>>>) -> Result<(), Box<dyn std::error::Error>> {
        let size = epoll_wait(epoll_fd, buffer, 100)?;
        if size != 0 {
            // TODO: Should not need to clone this.
            let buffer = buffer.clone();
            let handlers = handlers.clone();
            tokio::task::spawn(async move {
                let handlers = handlers.read().await;
                for i in 0..size {
                    if buffer[i].events() & EpollFlags::EPOLLIN != EpollFlags::empty() {
                        if let Some(handler) = handlers.get(&buffer[i].data()) {
                            handler.recv().await;
                        };
                    };
                };
            });
        }
        Ok(())
    }

    /* Once called, the fd belongs to this class and will be closed by it */
    pub async fn add_item(&self, fd: RawFd, socket_handler: Box<dyn SocketHandler + Send + Sync>) -> Result<Arc<PollerItem>, Box<dyn std::error::Error>> {
        let epoll_fd = *self.epoll_fd.get();
        let handlers = self.handlers.clone();

        let handle_id: u64 = rand::random();
        let poller_item = Arc::new(PollerItem::new(fd, socket_handler));
//        let mut event = EpollEvent::new(EpollFlags::EPOLLIN | EpollFlags::EPOLLOUT | EpollFlags::EPOLLHUP | EpollFlags::EPOLLET, handle_id);
        let mut event = EpollEvent::new(EpollFlags::EPOLLET | EpollFlags::EPOLLIN, handle_id);
        epoll_ctl(epoll_fd, EpollOp::EpollCtlAdd, fd, Some(&mut event))?;

        let mut handlers = handlers.write().await;
        handlers.insert(handle_id, poller_item.clone());
        Ok(poller_item)
    }
}
impl Drop for Poller {
    fn drop(&mut self) {
        self.poller_running.store(false, Ordering::SeqCst);
        self.controller_running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::Ordering;

    use async_trait::async_trait;
    use nix::sys::socket;
    use rand::Rng;

    use super::SocketHandler;

    #[tokio::test]
    async fn test_autoclose_fd() -> Result<(), Box<dyn std::error::Error>> {
        match nix::fcntl::fcntl({
            let subject = super::AutoCloseFd::new(nix::fcntl::open("/tmp", nix::fcntl::OFlag::O_TMPFILE | nix::fcntl::OFlag::O_RDWR, nix::sys::stat::Mode::empty())?);
            nix::fcntl::fcntl(*subject.get(), nix::fcntl::FcntlArg::F_GETFD)?;
            *subject.get()
        }, nix::fcntl::FcntlArg::F_GETFD) {
            Ok(_) => assert!(false, "Expected fd to be invalid"),
            Err(_) => (),
        };
        Ok(())
    }

    struct EchoCallback {
        fd: super::RawFd,
        count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl EchoCallback {
        pub fn new(fd: super::RawFd, count: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> EchoCallback {
            EchoCallback {
                fd,
                count
            }
        }
    }
    #[async_trait]
    impl SocketHandler for EchoCallback {
        async fn on_recv(&self, _data: Vec<u8>) {
            if self.count.load(std::sync::atomic::Ordering::SeqCst) > 100000000 {
                return;
            }
            self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut rng = rand::thread_rng();
            let data: String = std::iter::repeat(()).map(|()| rng.sample(rand::distributions::Alphanumeric)).map(char::from).take(128).collect();
            nix::sys::socket::send(self.fd, data.as_bytes(), super::MsgFlags::empty()).expect("Failed to send data.");
            nix::sys::socket::send(self.fd, data.as_bytes(), super::MsgFlags::empty()).expect("Failed to send data.");
        }
    }

    #[tokio::test]
    async fn test_poll() -> Result<(), Box<dyn std::error::Error>> {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let subject = std::sync::Arc::new(tokio::sync::Mutex::new(super::Poller::new()?));
        let items = std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new()));
        println!("Creating Sockets");
        for _ in (0 as i32)..(1000 as i32) {
            let sockets = socket::socketpair(socket::AddressFamily::Unix, socket::SockType::Stream, None, socket::SockFlag::SOCK_NONBLOCK)?;

            let subject = subject.lock().await;
            let mut items = items.write().await;
            items.push(subject.add_item(sockets.0, Box::new(EchoCallback::new(sockets.0, count.clone()))).await);
            items.push(subject.add_item(sockets.1, Box::new(EchoCallback::new(sockets.1, count.clone()))).await);

            nix::sys::socket::send(sockets.0, "Hello".to_string().as_bytes(), super::MsgFlags::empty()).expect("Failed to send data.");
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        println!("Sinking Data");

        tokio::time::sleep(tokio::time::Duration::from_millis(10000)).await;
        println!("Creating Data");
        println!("{:?}", chrono::offset::Utc::now());

        println!("Waiting");
        println!("{:?}", chrono::offset::Utc::now());
        
        tokio::time::sleep(tokio::time::Duration::from_millis(30000)).await;
        println!("Send Executed: {}", count.load(Ordering::SeqCst));
        println!("{:?}", chrono::offset::Utc::now());

        Ok(())
    }
}
