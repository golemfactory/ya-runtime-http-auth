//! `hyper_rustls::stream::MaybeHttpsStream` w/ a source socket address

use std::io;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use hyper::client::connect::{Connected, Connection};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

pub type HttpStream = HttpStreamKind<TcpStream>;

#[allow(clippy::large_enum_variant)]
pub enum HttpStreamKind<T> {
    Plain {
        inner: T,
        remote_addr: SocketAddr,
    },
    Tls {
        inner: TlsStream<T>,
        remote_addr: SocketAddr,
    },
}

impl<T> HttpStreamKind<T> {
    pub fn plain(inner: T, addr: SocketAddr) -> Self {
        HttpStreamKind::Plain {
            inner,
            remote_addr: addr,
        }
    }

    pub fn tls(inner: TlsStream<T>, addr: SocketAddr) -> Self {
        HttpStreamKind::Tls {
            inner,
            remote_addr: addr,
        }
    }

    #[inline]
    pub fn remote_addr(&self) -> SocketAddr {
        match self {
            Self::Plain { remote_addr, .. } => *remote_addr,
            Self::Tls { remote_addr, .. } => *remote_addr,
        }
    }
}

impl<T: AsyncRead + AsyncWrite + Connection + Unpin> Connection for HttpStreamKind<T> {
    fn connected(&self) -> Connected {
        match self {
            HttpStreamKind::Plain { inner, .. } => inner.connected(),
            HttpStreamKind::Tls { inner, .. } => {
                let (tcp, tls) = inner.get_ref();
                if tls.alpn_protocol() == Some(b"h2") {
                    tcp.connected().negotiated_h2()
                } else {
                    tcp.connected()
                }
            }
        }
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> AsyncRead for HttpStreamKind<T> {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match Pin::get_mut(self) {
            Self::Plain { inner, .. } => Pin::new(inner).poll_read(cx, buf),
            Self::Tls { inner, .. } => Pin::new(inner).poll_read(cx, buf),
        }
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> AsyncWrite for HttpStreamKind<T> {
    #[inline]
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::get_mut(self) {
            Self::Plain { inner, .. } => Pin::new(inner).poll_write(cx, buf),
            Self::Tls { inner, .. } => Pin::new(inner).poll_write(cx, buf),
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::get_mut(self) {
            Self::Plain { inner, .. } => Pin::new(inner).poll_flush(cx),
            Self::Tls { inner, .. } => Pin::new(inner).poll_flush(cx),
        }
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::get_mut(self) {
            Self::Plain { inner, .. } => Pin::new(inner).poll_shutdown(cx),
            Self::Tls { inner, .. } => Pin::new(inner).poll_shutdown(cx),
        }
    }

    #[inline]
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match Pin::get_mut(self) {
            Self::Plain { inner, .. } => Pin::new(inner).poll_write_vectored(cx, bufs),
            Self::Tls { inner, .. } => Pin::new(inner).poll_write_vectored(cx, bufs),
        }
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        match self {
            Self::Plain { inner, .. } => inner.is_write_vectored(),
            Self::Tls { inner, .. } => inner.is_write_vectored(),
        }
    }
}

#[cfg(unix)]
impl<T: AsRawFd> AsRawFd for HttpStreamKind<T> {
    #[inline]
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Self::Plain { inner, .. } => inner.as_raw_fd(),
            Self::Tls { inner, .. } => inner.as_raw_fd(),
        }
    }
}
