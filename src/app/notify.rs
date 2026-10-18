//! System notifications (Notification Center, Windows toasts, the Linux
//! desktop's), for things that happen while Ferrodock isn't in front.

/// Shows `body` as a notification from Ferrodock. It runs on its own
/// thread, since some systems answer slowly, and failures are ignored:
/// the same news also shows in the window.
pub fn send(body: String) {
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .summary("Ferrodock")
            .body(&body)
            .appname("Ferrodock")
            .show();
    });
}
