/*
 * Copyright (C) 2020 Red Hat, Inc.
 *
 * SPDX-License-Identifier: Apache-2.0
 */

use anyhow::{bail, Context, Result};
use fn_error_context::context;
use nix::sys::socket as nixsocket;
use serde::{Deserialize, Serialize};
use std::os::unix::net::UnixStream as StdUnixStream;

use crate::bootupd::ClientRequest;

pub(crate) const BOOTUPD_SOCKET: &str = "/run/bootupd.sock";
/// Sent between processes along with SCM credentials
pub(crate) const BOOTUPD_HELLO_MSG: &str = "bootupd-hello\n";

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum DaemonToClientReply {
    Success(Vec<u8>),
    Failure(String),
}

pub(crate) struct ClientToDaemonConnection {
    send: tokio_unix_ipc::Sender<crate::bootupd::ClientRequest>,
    recv: tokio_unix_ipc::Receiver<DaemonToClientReply>,
}

impl ClientToDaemonConnection {
    #[context("connecting to {}", BOOTUPD_SOCKET)]
    pub(crate) async fn new() -> Result<Self> {
        let sock = StdUnixStream::connect(BOOTUPD_SOCKET)?;
        let (send, recv) = tokio_unix_ipc::raw_channel_from_std(sock)?;
        send.send_with_credentials(BOOTUPD_HELLO_MSG.as_bytes(), &[])
            .await?;

        Ok(Self {
            send: send.into(),
            recv: recv.into(),
        })
    }

    pub(crate) async fn send<T: serde::de::DeserializeOwned>(
        &mut self,
        msg: ClientRequest,
    ) -> Result<T> {
        self.send.send(msg).await?;
        let reply = self.recv.recv().await?;
        match reply {
            DaemonToClientReply::Success(r) => Ok(bincode::deserialize(&r)?),
            DaemonToClientReply::Failure(buf) => {
                // For now we just prefix server
                anyhow::bail!("internal error: {}", buf);
            }
        }
    }

    pub(crate) fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}
