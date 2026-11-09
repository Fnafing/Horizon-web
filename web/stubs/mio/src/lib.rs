//! Browser build: no OS sockets, so Poll has nothing to wait on.
use std::io;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Token(pub usize);
impl From<Token> for usize { fn from(t: Token) -> usize { t.0 } }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interest(u8);
impl Interest {
    pub const READABLE: Interest = Interest(1);
    pub const WRITABLE: Interest = Interest(2);
    pub const fn add(self, o: Interest) -> Interest { Interest(self.0 | o.0) }
    pub const fn is_readable(self) -> bool { self.0 & 1 != 0 }
    pub const fn is_writable(self) -> bool { self.0 & 2 != 0 }
}
impl std::ops::BitOr for Interest { type Output = Interest; fn bitor(self, o: Interest) -> Interest { self.add(o) } }

pub mod event {
    use super::*;
    #[derive(Debug)]
    pub struct Event(Token);
    impl Event {
        pub fn token(&self) -> Token { self.0 }
        pub fn is_readable(&self) -> bool { false }
        pub fn is_writable(&self) -> bool { false }
        pub fn is_error(&self) -> bool { false }
        pub fn is_read_closed(&self) -> bool { false }
        pub fn is_write_closed(&self) -> bool { false }
    }
    pub trait Source {}
    impl Source for crate::net::TcpStream {}
}
pub use event::Event;

#[derive(Debug, Default)]
pub struct Events(Vec<Event>);
impl Events {
    pub fn with_capacity(_: usize) -> Events { Events(Vec::new()) }
    pub fn iter(&self) -> std::slice::Iter<'_, Event> { self.0.iter() }
    pub fn is_empty(&self) -> bool { true }
    pub fn clear(&mut self) {}
}
impl<'a> IntoIterator for &'a Events {
    type Item = &'a Event;
    type IntoIter = std::slice::Iter<'a, Event>;
    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}

#[derive(Debug)]
pub struct Registry(());
impl Registry {
    pub fn register<S: event::Source + ?Sized>(&self, _: &mut S, _: Token, _: Interest) -> io::Result<()> { Ok(()) }
    pub fn reregister<S: event::Source + ?Sized>(&self, _: &mut S, _: Token, _: Interest) -> io::Result<()> { Ok(()) }
    pub fn deregister<S: event::Source + ?Sized>(&self, _: &mut S) -> io::Result<()> { Ok(()) }
    pub fn try_clone(&self) -> io::Result<Registry> { Ok(Registry(())) }
}

#[derive(Debug)]
pub struct Poll(Registry);
impl Poll {
    pub fn new() -> io::Result<Poll> { Ok(Poll(Registry(()))) }
    pub fn registry(&self) -> &Registry { &self.0 }
    /// Nothing can become ready; sleeps for the timeout like a real poll would.
    pub fn poll(&mut self, events: &mut Events, timeout: Option<Duration>) -> io::Result<()> {
        events.clear();
        std::thread::sleep(timeout.unwrap_or(Duration::from_millis(50)).min(Duration::from_millis(50)));
        Ok(())
    }
}

#[derive(Debug)]
pub struct Waker(());
impl Waker {
    pub fn new(_: &Registry, _: Token) -> io::Result<Waker> { Ok(Waker(())) }
    pub fn wake(&self) -> io::Result<()> { Ok(()) }
}

pub mod net {
    use std::io;
    /// Never constructed in the browser: no std TcpStream can be opened there.
    #[derive(Debug)]
    pub struct TcpStream(std::net::TcpStream);
    impl TcpStream {
        pub fn from_std(s: std::net::TcpStream) -> TcpStream { TcpStream(s) }
        pub fn peer_addr(&self) -> io::Result<std::net::SocketAddr> { self.0.peer_addr() }
        pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> { self.0.shutdown(how) }
        pub fn set_nodelay(&self, v: bool) -> io::Result<()> { self.0.set_nodelay(v) }
    }
    impl io::Read for TcpStream { fn read(&mut self, b: &mut [u8]) -> io::Result<usize> { self.0.read(b) } }
    impl io::Write for TcpStream {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> { self.0.write(b) }
        fn flush(&mut self) -> io::Result<()> { self.0.flush() }
    }
    impl io::Read for &TcpStream { fn read(&mut self, b: &mut [u8]) -> io::Result<usize> { (&self.0).read(b) } }
    impl io::Write for &TcpStream {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> { (&self.0).write(b) }
        fn flush(&mut self) -> io::Result<()> { (&self.0).flush() }
    }
}
