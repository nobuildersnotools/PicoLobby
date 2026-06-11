use tokio_util::sync::CancellationToken;

/// Resolves when the server should stop accepting new connections.
///
/// When a [`CancellationToken`] is supplied (the Java-wrapper embedding path),
/// the token controls shutdown so the host process can stop the server cleanly.
/// Otherwise the standalone path waits on the platform interrupt/terminate signal.
pub async fn shutdown_signal(token: Option<&CancellationToken>) {
    match token {
        Some(token) => token.cancelled().await,
        None => platform_signal().await,
    }
}

async fn platform_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = sigint.recv() => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        use tokio::signal;
        // On Windows, tokio::signal::ctrl_c is the best we can do.
        let _ = signal::ctrl_c().await;
    }
}
