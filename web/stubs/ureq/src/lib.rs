//! Offline ureq 2 for the browser build. Requests compile and fail at runtime with
//! `Error::Transport`, which every caller already treats as "no connection".
use std::fmt;
use std::io::Read;
use std::time::Duration;

pub fn agent() -> Agent { Agent }
pub fn get(path: &str) -> Request { Agent.get(path) }
pub fn post(path: &str) -> Request { Agent.post(path) }
pub fn put(path: &str) -> Request { Agent.put(path) }
pub fn delete(path: &str) -> Request { Agent.delete(path) }
pub fn head(path: &str) -> Request { Agent.head(path) }
pub fn patch(path: &str) -> Request { Agent.patch(path) }
pub fn request(method: &str, path: &str) -> Request { Agent.request(method, path) }

#[derive(Clone, Debug, Default)]
pub struct Agent;
impl Agent {
    pub fn new() -> Agent { Agent }
    pub fn request(&self, method: &str, path: &str) -> Request {
        Request { method: method.into(), url: path.into() }
    }
    pub fn get(&self, path: &str) -> Request { self.request("GET", path) }
    pub fn post(&self, path: &str) -> Request { self.request("POST", path) }
    pub fn put(&self, path: &str) -> Request { self.request("PUT", path) }
    pub fn delete(&self, path: &str) -> Request { self.request("DELETE", path) }
    pub fn head(&self, path: &str) -> Request { self.request("HEAD", path) }
    pub fn patch(&self, path: &str) -> Request { self.request("PATCH", path) }
}

#[derive(Clone, Debug, Default)]
pub struct AgentBuilder;
impl AgentBuilder {
    pub fn new() -> Self { AgentBuilder }
    pub fn build(self) -> Agent { Agent }
    pub fn timeout(self, _: Duration) -> Self { self }
    pub fn timeout_connect(self, _: Duration) -> Self { self }
    pub fn timeout_read(self, _: Duration) -> Self { self }
    pub fn timeout_write(self, _: Duration) -> Self { self }
    pub fn user_agent(self, _: &str) -> Self { self }
    pub fn redirects(self, _: u32) -> Self { self }
    pub fn proxy(self, _: Proxy) -> Self { self }
    pub fn try_proxy_from_env(self, _: bool) -> Self { self }
    pub fn max_idle_connections(self, _: usize) -> Self { self }
    pub fn max_idle_connections_per_host(self, _: usize) -> Self { self }
    pub fn no_delay(self, _: bool) -> Self { self }
    pub fn https_only(self, _: bool) -> Self { self }
}

#[derive(Clone, Debug)]
pub struct Proxy;
impl Proxy {
    pub fn new<S: AsRef<str>>(_: S) -> Result<Proxy, Error> { Ok(Proxy) }
}

#[derive(Clone, Debug)]
pub struct Request {
    method: String,
    url: String,
}
impl Request {
    fn fail(&self) -> Result<Response, Error> {
        Err(Error::Transport(Transport { url: self.url.clone() }))
    }
    pub fn set(self, _: &str, _: &str) -> Self { self }
    pub fn query(self, _: &str, _: &str) -> Self { self }
    pub fn query_pairs<'a, P: IntoIterator<Item = (&'a str, &'a str)>>(self, _: P) -> Self { self }
    pub fn timeout(self, _: Duration) -> Self { self }
    pub fn method(&self) -> &str { &self.method }
    pub fn url(&self) -> &str { &self.url }
    pub fn header(&self, _: &str) -> Option<&str> { None }
    pub fn call(self) -> Result<Response, Error> { self.fail() }
    pub fn send(self, _: impl Read) -> Result<Response, Error> { self.fail() }
    pub fn send_bytes(self, _: &[u8]) -> Result<Response, Error> { self.fail() }
    pub fn send_string(self, _: &str) -> Result<Response, Error> { self.fail() }
    pub fn send_json(self, _: impl serde::Serialize) -> Result<Response, Error> { self.fail() }
    pub fn send_form(self, _: &[(&str, &str)]) -> Result<Response, Error> { self.fail() }
}

/// Never constructed: no request ever succeeds offline.
#[derive(Debug)]
pub struct Response {
    status: u16,
}
impl Response {
    pub fn status(&self) -> u16 { self.status }
    pub fn status_text(&self) -> &str { "" }
    pub fn get_url(&self) -> &str { "" }
    pub fn header(&self, _: &str) -> Option<&str> { None }
    pub fn headers_names(&self) -> Vec<String> { Vec::new() }
    pub fn all(&self, _: &str) -> Vec<&str> { Vec::new() }
    pub fn content_type(&self) -> &str { "" }
    pub fn charset(&self) -> &str { "" }
    pub fn into_reader(self) -> Box<dyn Read + Send + Sync + 'static> { Box::new(std::io::empty()) }
    pub fn into_string(self) -> std::io::Result<String> { Ok(String::new()) }
    pub fn into_json<T: serde::de::DeserializeOwned>(self) -> std::io::Result<T> {
        serde_json::from_str("null").map_err(std::io::Error::other)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind { InvalidUrl, UnknownScheme, Dns, InsecureRequestHttpsOnly, ConnectionFailed, TooManyRedirects, BadStatus, BadHeader, Io, InvalidProxyUrl, ProxyConnect, ProxyUnauthorized, HTTP }
impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("Network Error: offline (browser build)") }
}

#[derive(Debug)]
pub struct Transport { url: String }
impl Transport {
    pub fn kind(&self) -> ErrorKind { ErrorKind::ConnectionFailed }
    pub fn message(&self) -> Option<&str> { Some("offline (browser build)") }
    pub fn url(&self) -> Option<&str> { Some(&self.url) }
}
impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}: offline (browser build)", self.url) }
}
impl std::error::Error for Transport {}

#[derive(Debug)]
pub enum Error {
    Status(u16, Response),
    Transport(Transport),
}
impl Error {
    pub fn kind(&self) -> ErrorKind { ErrorKind::ConnectionFailed }
    pub fn into_response(self) -> Option<Response> { match self { Error::Status(_, r) => Some(r), _ => None } }
    pub fn into_transport(self) -> Option<Transport> { match self { Error::Transport(t) => Some(t), _ => None } }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self { Error::Status(c, _) => write!(f, "status {c}"), Error::Transport(t) => t.fmt(f) }
    }
}
impl std::error::Error for Error {}
impl From<Error> for std::io::Error {
    fn from(e: Error) -> Self { std::io::Error::other(e.to_string()) }
}
