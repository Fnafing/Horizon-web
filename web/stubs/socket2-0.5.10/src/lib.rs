//! Browser build: no raw sockets. Everything returns io::ErrorKind::Unsupported.
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

fn no<T>() -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "sockets are not available in the browser build"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Domain(i32);
impl Domain {
    pub const IPV4: Domain = Domain(2);
    pub const IPV6: Domain = Domain(10);
    pub fn for_address(a: SocketAddr) -> Domain { if a.is_ipv6() { Self::IPV6 } else { Self::IPV4 } }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Type(i32);
impl Type {
    pub const STREAM: Type = Type(1);
    pub const DGRAM: Type = Type(2);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Protocol(i32);
impl Protocol {
    pub const TCP: Protocol = Protocol(6);
    pub const UDP: Protocol = Protocol(17);
}
#[derive(Clone, Debug)]
pub struct SockAddr(SocketAddr);
impl From<SocketAddr> for SockAddr { fn from(a: SocketAddr) -> Self { SockAddr(a) } }
impl SockAddr { pub fn as_socket(&self) -> Option<SocketAddr> { Some(self.0) } }

#[derive(Clone, Debug, Default)]
pub struct TcpKeepalive;
impl TcpKeepalive {
    pub const fn new() -> Self { TcpKeepalive }
    pub const fn with_time(self, _: Duration) -> Self { self }
    pub const fn with_interval(self, _: Duration) -> Self { self }
    pub const fn with_retries(self, _: u32) -> Self { self }
}

#[derive(Debug)]
pub struct Socket(());
impl Socket {
    pub fn new(_: Domain, _: Type, _: Option<Protocol>) -> io::Result<Socket> { no() }
    pub fn bind(&self, _: &SockAddr) -> io::Result<()> { no() }
    pub fn listen(&self, _: i32) -> io::Result<()> { no() }
    pub fn connect(&self, _: &SockAddr) -> io::Result<()> { no() }
    pub fn connect_timeout(&self, _: &SockAddr, _: Duration) -> io::Result<()> { no() }
    pub fn set_nonblocking(&self, _: bool) -> io::Result<()> { no() }
    pub fn set_reuse_address(&self, _: bool) -> io::Result<()> { no() }
    pub fn set_reuse_port(&self, _: bool) -> io::Result<()> { no() }
    pub fn set_only_v6(&self, _: bool) -> io::Result<()> { no() }
    pub fn set_nodelay(&self, _: bool) -> io::Result<()> { no() }
    pub fn set_tcp_keepalive(&self, _: &TcpKeepalive) -> io::Result<()> { no() }
    pub fn local_addr(&self) -> io::Result<SockAddr> { no() }
}
impl From<Socket> for TcpStream { fn from(_: Socket) -> TcpStream { unreachable!("no sockets in the browser build") } }
impl From<Socket> for TcpListener { fn from(_: Socket) -> TcpListener { unreachable!("no sockets in the browser build") } }

pub struct SockRef<'a>(std::marker::PhantomData<&'a ()>);
impl<'a> From<&'a TcpStream> for SockRef<'a> { fn from(_: &'a TcpStream) -> Self { SockRef(std::marker::PhantomData) } }
impl<'a> From<&'a TcpListener> for SockRef<'a> { fn from(_: &'a TcpListener) -> Self { SockRef(std::marker::PhantomData) } }
impl SockRef<'_> {
    pub fn set_tcp_keepalive(&self, _: &TcpKeepalive) -> io::Result<()> { Ok(()) }
    pub fn set_quickack(&self, _: bool) -> io::Result<()> { Ok(()) }
    pub fn set_nodelay(&self, _: bool) -> io::Result<()> { Ok(()) }
}
