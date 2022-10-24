//! Daemon logic.

use crate::bootupd::ClientRequest;
use crate::component::ValidationResult;
use crate::ipc::DaemonToClientReply;
use crate::model::Status;
use crate::{bootupd, ipc};
use anyhow::{bail, Context, Result};
use libsystemd::activation::IsType;
use std::os::unix::net::UnixListener as StdUnixListener;
use std::os::unix::prelude::*;
use std::time::Duration;
use tokio::net::{UnixListener, UnixStream};

/// We run as a daemon for a few reasons.  The biggest is that
/// tihs acts as a natural "lock" to avoid concurrent changes
/// on the filesystem - we only process one request at a time.
/// There are a variety of smaller reasons, such as being
/// able to use systemd sandboxing.
/// But we don't want to have a process pointlessly hanging
/// around, so we time out and exit pretty quickly.
const DAEMON_TIMEOUT_SECS: u64 = 1;

async fn process_one_client(sock: UnixStream) -> Result<()> {
    // The tokio-unix-ipc bits want to own the FD, so convert back to
    // std.
    let sock = sock.into_std()?;
    let (send, recv) = tokio_unix_ipc::raw_channel_from_std(sock)?;
    let (contents, fds, creds) = recv.recv_with_credentials().await?;
    if creds.uid() != 0 {
        bail!("unauthorized pid:{} uid:{}", creds.pid(), creds.uid())
    }
    let hello = String::from_utf8_lossy(&contents);
    if hello != ipc::BOOTUPD_HELLO_MSG {
        bail!("Didn't receive correct hello message, found: {:?}", &hello);
    }

    // Process all requests from this client.
    process_client_requests(send, recv).await
}

async fn run_async(sock: StdUnixListener) -> Result<()> {
    let sock: UnixListener = sock.try_into()?;

    let timeout = tokio::time::sleep(Duration::from_secs(DAEMON_TIMEOUT_SECS));
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            clientres = sock.accept() => {
                let client = clientres?.0;
                if let Err(e) = process_one_client(client).await {
                    log::error!("failed to process client: {}", e);
                }
            },
            _ = &mut timeout => {
                return Ok(())
            }
        }
    }
}

/// Accept a single client and then exit; we don't want to
/// persistently run as a daemon.  The systemd unit is mostly
/// and implementation detail - it lets us use things like
/// systemd's built in sandboxing (ProtectHome=yes) etc. and also
/// ensures that only a single bootupd instance is running at
/// a time (i.e. don't support concurrent updates).
pub async fn run() -> Result<()> {
    let sockfd = systemd_activation().context("systemd service activation error")?;
    assert!(sockfd.is_unix());
    let sockfd = unsafe { StdUnixListener::from_raw_fd(sockfd.into_raw_fd()) };

    let runtime = tokio::runtime::Handle::current();
    runtime.block_on(async move { run_async(sockfd).await })
}

/// Perform initialization steps required by systemd service activation.
///
/// This ensures that the system is running under systemd, then receives the
/// socket-FD for main IPC logic, and notifies systemd about ready-state.
fn systemd_activation() -> Result<libsystemd::activation::FileDescriptor> {
    use libsystemd::daemon::{self, NotifyState};

    if !daemon::booted() {
        bail!("daemon is not running as a systemd service");
    }

    let srvsock_fd = {
        let mut fds = libsystemd::activation::receive_descriptors(true)
            .map_err(|e| anyhow::anyhow!("failed to receive file-descriptors: {}", e))?;
        fds.pop()
            .ok_or_else(|| anyhow::anyhow!("no socket-fd received on service activation"))?
    };

    let sent = daemon::notify(true, &[NotifyState::Ready])
        .map_err(|e| anyhow::anyhow!("failed to notify ready-state: {}", e))?;
    if !sent {
        log::warn!("failed to notify ready-state: service notifications not supported");
    }

    Ok(srvsock_fd)
}

/// Process all requests from a given client.
///
/// This sequentially processes all requests from a client, until it
/// disconnects or a connection error is encountered.
async fn process_client_requests(
    send: tokio_unix_ipc::RawSender,
    recv: tokio_unix_ipc::RawReceiver,
) -> Result<()> {
    let send: tokio_unix_ipc::Sender<DaemonToClientReply> = send.into();
    let recv: tokio_unix_ipc::Receiver<ClientRequest> = recv.into();
    loop {
        let msg = recv.recv().await?;
        log::trace!("processing request: {:?}", &msg);
        let r = match msg {
            ClientRequest::Update { component } => match bootupd::update(component.as_str()) {
                Ok(v) => DaemonToClientReply::Success(bincode::serialize(&v)?),
                Err(e) => DaemonToClientReply::Failure(format!("{:#}", e)),
            },
            ClientRequest::AdoptAndUpdate { component } => {
                match bootupd::adopt_and_update(component.as_str()) {
                    Ok(v) => DaemonToClientReply::Success(bincode::serialize(&v)?),
                    Err(e) => DaemonToClientReply::Failure(format!("{:#}", e)),
                }
            }
            ClientRequest::Validate { component } => match bootupd::validate(component.as_str()) {
                Ok(v) => DaemonToClientReply::Success(bincode::serialize(&v)?),
                Err(e) => DaemonToClientReply::Failure(format!("{:#}", e)),
            },
            ClientRequest::Status => match bootupd::status() {
                Ok(v) => DaemonToClientReply::Success(bincode::serialize(&v)?),
                Err(e) => DaemonToClientReply::Failure(format!("{:#}", e)),
            },
        };
        send.send(r).await?;
    }
}
