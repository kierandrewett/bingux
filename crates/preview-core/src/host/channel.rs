//! The socket between a viewer and the host: one [`Packet`] per datagram, with
//! file descriptors riding beside it as `SCM_RIGHTS`.
//!
//! A pair is a `socketpair(AF_UNIX, SOCK_SEQPACKET)`, so message boundaries
//! survive and nothing here frames anything. A message that is too long, that
//! brings more descriptors than a message may, or that does not decode is an
//! error and the session ends. Both ends are plain blocking descriptors; the
//! owner runs one reader thread and sends from any thread (a `sendmsg` of one
//! datagram is atomic).

use std::io;
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use super::protocol::{self, MAX_MESSAGE, Packet, ProtocolError};

/// The most descriptors one message carries: the file and its subtitles.
pub const MAX_FDS: usize = 2;

/// Space for the ancillary data of `MAX_FDS` descriptors, aligned for `cmsghdr`.
const CONTROL_WORDS: usize = 8;

/// A message and the descriptors that came with it.
#[derive(Debug)]
pub struct Received {
    pub packet: Packet,
    pub fds: Vec<OwnedFd>,
}

/// Why nothing was received.
#[derive(Debug)]
pub enum RecvError {
    /// The other end closed or died: a clean end of the session.
    Closed,
    /// The bytes were not a message.
    Protocol(ProtocolError),
    /// More descriptors than a message may carry, or the control data was cut.
    Descriptors,
    Io(io::Error),
}

impl std::fmt::Display for RecvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecvError::Closed => f.write_str("the other end closed"),
            RecvError::Protocol(error) => write!(f, "{error}"),
            RecvError::Descriptors => f.write_str("too many descriptors"),
            RecvError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RecvError {}

/// One end of the socket.
#[derive(Debug)]
pub struct Channel {
    fd: OwnedFd,
}

impl Channel {
    /// Two connected ends. Neither survives an `exec`; clear `FD_CLOEXEC` on
    /// the one that is handed to a child.
    pub fn pair() -> io::Result<(Channel, Channel)> {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: `fds` has room for the two descriptors socketpair writes.
        let result = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0, fds.as_mut_ptr()) };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: socketpair returned two descriptors that nothing else owns.
        let (a, b) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        Ok((Channel { fd: a }, Channel { fd: b }))
    }

    /// The end of a socket this process was started with.
    ///
    /// # Safety
    /// `fd` must be an open `SOCK_SEQPACKET` descriptor that nothing else owns.
    pub unsafe fn from_raw_fd(fd: RawFd) -> Channel {
        // SAFETY: the caller owns `fd`.
        Channel { fd: unsafe { OwnedFd::from_raw_fd(fd) } }
    }

    pub fn into_fd(self) -> OwnedFd {
        self.fd
    }

    /// Another handle on the same end, for a reader thread beside a writer.
    pub fn try_clone(&self) -> io::Result<Channel> {
        Ok(Channel { fd: self.fd.try_clone()? })
    }

    /// Closes both directions: a thread blocked in [`Channel::recv`] on this
    /// end (or on the other) returns [`RecvError::Closed`].
    pub fn shutdown(&self) {
        // SAFETY: shutdown on an open descriptor; a failure leaves it open.
        unsafe {
            libc::shutdown(self.fd.as_raw_fd(), libc::SHUT_RDWR);
        }
    }

    /// Send `packet`, with `fds` beside it. Blocks only while the peer's
    /// receive queue is full.
    pub fn send(&self, packet: &Packet, fds: &[RawFd]) -> io::Result<()> {
        assert!(fds.len() <= MAX_FDS, "at most {MAX_FDS} descriptors");
        let bytes = protocol::encode(packet);
        let mut iov = libc::iovec { iov_base: bytes.as_ptr() as *mut libc::c_void, iov_len: bytes.len() };
        let mut control = [0u64; CONTROL_WORDS];
        // SAFETY: msghdr is plain data; zero is a valid empty value.
        let mut header: libc::msghdr = unsafe { mem::zeroed() };
        header.msg_iov = &mut iov;
        header.msg_iovlen = 1;
        if !fds.is_empty() {
            let payload = mem::size_of_val(fds);
            // SAFETY: CMSG_SPACE and CMSG_LEN only compute sizes.
            let space = unsafe { libc::CMSG_SPACE(payload as u32) } as usize;
            debug_assert!(space <= mem::size_of_val(&control));
            header.msg_control = control.as_mut_ptr().cast();
            header.msg_controllen = space as _;
            // SAFETY: the control buffer is aligned and large enough for one
            // header and `payload` bytes; CMSG_FIRSTHDR points inside it.
            unsafe {
                let cmsg = libc::CMSG_FIRSTHDR(&header);
                (*cmsg).cmsg_level = libc::SOL_SOCKET;
                (*cmsg).cmsg_type = libc::SCM_RIGHTS;
                (*cmsg).cmsg_len = libc::CMSG_LEN(payload as u32) as _;
                std::ptr::copy_nonoverlapping(fds.as_ptr().cast::<u8>(), libc::CMSG_DATA(cmsg), payload);
            }
        }
        loop {
            // SAFETY: `header` points at live buffers for the call.
            let sent = unsafe { libc::sendmsg(self.fd.as_raw_fd(), &header, libc::MSG_NOSIGNAL) };
            if sent >= 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    /// The next message, waiting for it.
    pub fn recv(&self) -> Result<Received, RecvError> {
        let mut bytes = [0u8; MAX_MESSAGE + 1];
        let mut iov = libc::iovec { iov_base: bytes.as_mut_ptr().cast(), iov_len: bytes.len() };
        let mut control = [0u64; CONTROL_WORDS];
        // SAFETY: msghdr is plain data; zero is a valid empty value.
        let mut header: libc::msghdr = unsafe { mem::zeroed() };
        header.msg_iov = &mut iov;
        header.msg_iovlen = 1;
        header.msg_control = control.as_mut_ptr().cast();
        header.msg_controllen = mem::size_of_val(&control) as _;
        let received = loop {
            // SAFETY: `header` points at live buffers for the call.
            let received = unsafe { libc::recvmsg(self.fd.as_raw_fd(), &mut header, libc::MSG_CMSG_CLOEXEC) };
            if received >= 0 {
                break received as usize;
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::ConnectionReset | io::ErrorKind::BrokenPipe => return Err(RecvError::Closed),
                _ => return Err(RecvError::Io(error)),
            }
        };
        // Take ownership of every descriptor first, so none leaks on an error.
        let mut fds = Vec::new();
        // SAFETY: the kernel filled `header` and `control`; the walk stays
        // inside `msg_controllen` as CMSG_NXTHDR guarantees.
        unsafe {
            let mut cmsg = libc::CMSG_FIRSTHDR(&header);
            while !cmsg.is_null() {
                if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                    let payload = (*cmsg).cmsg_len as usize - libc::CMSG_LEN(0) as usize;
                    let data = libc::CMSG_DATA(cmsg).cast::<RawFd>();
                    for index in 0..payload / mem::size_of::<RawFd>() {
                        fds.push(OwnedFd::from_raw_fd(data.add(index).read_unaligned()));
                    }
                }
                cmsg = libc::CMSG_NXTHDR(&header, cmsg);
            }
        }
        if header.msg_flags & libc::MSG_CTRUNC != 0 || fds.len() > MAX_FDS {
            return Err(RecvError::Descriptors);
        }
        if received == 0 && fds.is_empty() {
            return Err(RecvError::Closed);
        }
        if header.msg_flags & libc::MSG_TRUNC != 0 || received > MAX_MESSAGE {
            return Err(RecvError::Protocol(ProtocolError::TooLong));
        }
        let packet = protocol::decode(&bytes[..received]).map_err(RecvError::Protocol)?;
        Ok(Received { packet, fds })
    }

    /// Whether a message (or the end of the session) is waiting, within
    /// `timeout_ms`.
    pub fn readable(&self, timeout_ms: i32) -> bool {
        let mut poll = libc::pollfd { fd: self.fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd.
        let ready = unsafe { libc::poll(&mut poll, 1, timeout_ms) };
        ready > 0
    }
}

impl AsRawFd for Channel {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::super::protocol::Message;
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};

    fn packet(request: u32, message: Message) -> Packet {
        Packet { request, message }
    }

    #[test]
    fn a_message_crosses_with_its_boundaries() {
        let (a, b) = Channel::pair().unwrap();
        a.send(&packet(1, Message::Play), &[]).unwrap();
        a.send(&packet(2, Message::Seek { ns: 9, accurate: false }), &[]).unwrap();
        let first = b.recv().unwrap();
        let second = b.recv().unwrap();
        assert_eq!(first.packet, packet(1, Message::Play));
        assert_eq!(second.packet, packet(2, Message::Seek { ns: 9, accurate: false }));
        assert!(first.fds.is_empty());
    }

    #[test]
    fn a_descriptor_rides_beside_its_message() {
        let (a, b) = Channel::pair().unwrap();
        let mut file = std::fs::File::from(super::super::shm::Shared::create("test", 16).unwrap().into_fd());
        file.write_all(b"hello").unwrap();
        a.send(&packet(3, Message::OpenDocument), &[file.as_raw_fd()]).unwrap();
        let mut received = b.recv().unwrap();
        assert_eq!(received.fds.len(), 1);
        let mut copy = std::fs::File::from(received.fds.remove(0));
        copy.seek(SeekFrom::Start(0)).unwrap();
        let mut text = [0u8; 5];
        copy.read_exact(&mut text).unwrap();
        assert_eq!(&text, b"hello");
    }

    #[test]
    fn the_end_of_the_peer_is_closed_not_an_error_of_the_protocol() {
        let (a, b) = Channel::pair().unwrap();
        drop(a);
        assert!(matches!(b.recv(), Err(RecvError::Closed)));
    }

    #[test]
    fn shutdown_wakes_a_blocked_reader() {
        let (a, b) = Channel::pair().unwrap();
        let reader = b.try_clone().unwrap();
        let handle = std::thread::spawn(move || reader.recv());
        std::thread::sleep(std::time::Duration::from_millis(50));
        b.shutdown();
        assert!(matches!(handle.join().unwrap(), Err(RecvError::Closed)));
        drop(a);
    }

    #[test]
    fn garbage_is_a_protocol_error() {
        let (a, b) = Channel::pair().unwrap();
        let junk = [0xFFu8; 20];
        // SAFETY: a plain send of a valid buffer on an open socket.
        let sent = unsafe { libc::send(a.as_raw_fd(), junk.as_ptr().cast(), junk.len(), 0) };
        assert_eq!(sent, 20);
        assert!(matches!(b.recv(), Err(RecvError::Protocol(_))));
        let long = vec![0u8; MAX_MESSAGE + 1];
        // SAFETY: as above.
        unsafe { libc::send(a.as_raw_fd(), long.as_ptr().cast(), long.len(), 0) };
        assert!(matches!(b.recv(), Err(RecvError::Protocol(ProtocolError::TooLong))));
    }

    #[test]
    fn three_descriptors_are_refused_and_closed() {
        let (a, b) = Channel::pair().unwrap();
        let keep: Vec<OwnedFd> = (0..3).map(|_| super::super::shm::Shared::create("x", 4).unwrap().into_fd()).collect();
        let raw: Vec<RawFd> = keep.iter().map(|fd| fd.as_raw_fd()).collect();
        let bytes = protocol::encode(&packet(1, Message::Play));
        let mut iov = libc::iovec { iov_base: bytes.as_ptr() as *mut libc::c_void, iov_len: bytes.len() };
        let mut control = [0u64; CONTROL_WORDS];
        // SAFETY: a hand-built SCM_RIGHTS message with three descriptors into a
        // buffer big enough for them.
        unsafe {
            let mut header: libc::msghdr = mem::zeroed();
            header.msg_iov = &mut iov;
            header.msg_iovlen = 1;
            let payload = mem::size_of_val(raw.as_slice());
            header.msg_control = control.as_mut_ptr().cast();
            header.msg_controllen = libc::CMSG_SPACE(payload as u32) as _;
            let cmsg = libc::CMSG_FIRSTHDR(&header);
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(payload as u32) as _;
            std::ptr::copy_nonoverlapping(raw.as_ptr().cast::<u8>(), libc::CMSG_DATA(cmsg), payload);
            assert!(libc::sendmsg(a.as_raw_fd(), &header, 0) >= 0);
        }
        assert!(matches!(b.recv(), Err(RecvError::Descriptors)));
    }
}
