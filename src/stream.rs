use std::net::{TcpStream, Shutdown};
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::time::Duration;

use openssl::ssl::{self, SslMethod, SSL_VERIFY_NONE, SSL_VERIFY_PEER};
use openssl::x509::X509_FILETYPE_PEM;

use mqtt311::{MqttWrite, MqttRead};
pub type SslStream = ssl::SslStream<TcpStream>;

use error::Result;

pub struct SslContext {
    pub inner: Arc<ssl::SslConnector>,
}

impl SslContext {
    pub fn new<CA, C, K>(ca: CA, client_pair: Option<(C, K)>, should_verify_ca: bool) -> Result<Self>
    where
        CA: AsRef<Path>,
        C: AsRef<Path>,
        K: AsRef<Path>,
    {
        let mut ctx_builder = ssl::SslConnectorBuilder::new(SslMethod::tls())?;
        ctx_builder.builder_mut().set_ca_file(ca.as_ref())?;

        if let Some((cert, key)) = client_pair {
            ctx_builder.builder_mut().set_certificate_file(cert, X509_FILETYPE_PEM)?;
            ctx_builder.builder_mut().set_private_key_file(key, X509_FILETYPE_PEM)?;
        }

        if should_verify_ca {
            ctx_builder.builder_mut().set_verify(SSL_VERIFY_PEER);
        } else {
            ctx_builder.builder_mut().set_verify(SSL_VERIFY_NONE);
        }

        Ok(SslContext { inner: Arc::new(ctx_builder.build()) })
    }

    pub fn connect(&self, domain: &str, stream: TcpStream) -> Result<SslStream> {
        let ssl_stream = ssl::SslConnector::connect(&*self.inner, domain, stream)?;
        Ok(ssl_stream)
    }
}

pub enum NetworkStream {
    Tcp(TcpStream),
    Tls(SslStream),
    None,
}

impl NetworkStream {
    pub fn connect(addr: &str, ca: Option<PathBuf>, certs: Option<(PathBuf, PathBuf)>) -> Result<NetworkStream> {
        let domain = addr.split(":")
                         .map(str::to_string)
                         .next()
                         .unwrap_or_default();
        let stream = TcpStream::connect(addr)?;

        if let Some(ca) = ca {
            if let Some((ref crt, ref key)) = certs {
                let ssl_ctx: SslContext = SslContext::new(ca, Some((crt, key)), true)?;
                Ok(NetworkStream::Tls(ssl_ctx.connect(&domain, stream)?))
            } else {
                let ssl_ctx: SslContext = SslContext::new(ca, None::<(String, String)>, true)?;
                Ok(NetworkStream::Tls(ssl_ctx.connect(&domain, stream)?))
            }
        } else {
            Ok(NetworkStream::Tcp(stream))
        }

    }


    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        match *self {
            NetworkStream::Tcp(ref s) => s.shutdown(how),
            NetworkStream::Tls(ref s) => s.get_ref().shutdown(how),
            NetworkStream::None => Err(io::Error::new(io::ErrorKind::Other, "No stream!")),
        }
    }

    pub fn set_read_timeout(&mut self, dur: Option<Duration>) -> io::Result<()> {
        match *self {
            NetworkStream::Tcp(ref s) => s.set_read_timeout(dur),
            NetworkStream::Tls(ref s) => s.get_ref().set_read_timeout(dur),
            NetworkStream::None => Err(io::Error::new(io::ErrorKind::Other, "No stream!")),
        }
    }

    pub fn set_write_timeout(&mut self, dur: Option<Duration>) -> io::Result<()> {
        match *self {
            NetworkStream::Tcp(ref s) => s.set_write_timeout(dur),
            NetworkStream::Tls(ref s) => s.get_ref().set_write_timeout(dur),
            NetworkStream::None => Err(io::Error::new(io::ErrorKind::Other, "No stream!")),
        }
    }
}

impl Read for NetworkStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match *self {
            NetworkStream::Tcp(ref mut s) => s.read(buf),
            NetworkStream::Tls(ref mut s) => s.read(buf),
            NetworkStream::None => Err(io::Error::new(io::ErrorKind::Other, "No stream!")),
        }
    }
}

impl Write for NetworkStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match *self {
            NetworkStream::Tcp(ref mut s) => s.write(buf),
            NetworkStream::Tls(ref mut s) => s.write(buf),
            NetworkStream::None => Err(io::Error::new(io::ErrorKind::Other, "No stream!")),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match *self {
            NetworkStream::Tcp(ref mut s) => s.flush(),
            NetworkStream::Tls(ref mut s) => s.flush(),
            NetworkStream::None => Err(io::Error::new(io::ErrorKind::Other, "No stream!")),
        }
    }
}

impl MqttRead for NetworkStream {}
impl MqttWrite for NetworkStream {}
