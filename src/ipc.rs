/*
 * Copyright (C) 2020 Red Hat, Inc.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{bail, Context, Result};
use fn_error_context::context;
use nix::sys::socket as nixsocket;

use std::os::unix::net::UnixStream as StdUnixStream;

pub(crate) const BOOTUPD_SOCKET: &str = "/run/bootupd.sock";
pub(crate) const MSGSIZE: usize = 1_048_576;

pub(crate) struct ClientToDaemonConnection {
    fd: i32,
}

impl Drop for ClientToDaemonConnection {
    fn drop(&mut self) {
        if self.fd != -1 {
            nix::unistd::close(self.fd).expect("close");
        }
    }
}

impl ClientToDaemonConnection {
    pub(crate) fn new() -> Self {
        Self { fd: -1 }
    }

    #[context("connecting to {}", BOOTUPD_SOCKET)]
    pub(crate) fn connect(&mut self) -> Result<()> {
        use nix::sys::uio::IoVec;
        self.fd = nixsocket::socket(
            nixsocket::AddressFamily::Unix,
            nixsocket::SockType::SeqPacket,
            nixsocket::SockFlag::SOCK_CLOEXEC,
            None,
        )?;
        let addr = nixsocket::SockAddr::new_unix(BOOTUPD_SOCKET)?;
        nixsocket::connect(self.fd, &addr)?;
        let creds = libc::ucred {
            pid: nix::unistd::getpid().as_raw(),
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
        };
        let creds = nixsocket::UnixCredentials::from(creds);
        let creds = nixsocket::ControlMessage::ScmCredentials(&creds);
        let _ = nixsocket::sendmsg(
            self.fd,
            &[IoVec::from_slice(BOOTUPD_HELLO_MSG.as_bytes())],
            &[creds],
            nixsocket::MsgFlags::MSG_CMSG_CLOEXEC,
            None,
        )?;
        Ok(())
    }

    pub(crate) fn send<S: serde::ser::Serialize, T: serde::de::DeserializeOwned>(
        &mut self,
        msg: &S,
    ) -> Result<T> {
        {
            let serialized = bincode::serialize(msg)?;
            let _ = nixsocket::send(self.fd, &serialized, nixsocket::MsgFlags::MSG_CMSG_CLOEXEC)
                .context("client sending request")?;
        }
        let reply: DaemonToClientReply<T> = {
            let mut buf = [0u8; MSGSIZE];
            let n = nixsocket::recv(self.fd, &mut buf, nixsocket::MsgFlags::MSG_CMSG_CLOEXEC)
                .context("client recv")?;
            let buf = &buf[0..n];
            if buf.is_empty() {
                bail!("Server sent an empty reply");
            }
            bincode::deserialize(buf).context("client parsing reply")?
        };
        match reply {
            DaemonToClientReply::Success::<T>(r) => Ok(r),
            DaemonToClientReply::Failure(buf) => {
                // For now we just prefix server
                anyhow::bail!("internal error: {}", buf);
            }
        }
    }

    pub(crate) fn shutdown(&mut self) -> Result<()> {
        nixsocket::shutdown(self.fd, nixsocket::Shutdown::Both)?;
        Ok(())
    }
}
