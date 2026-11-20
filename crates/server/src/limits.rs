/// macOS shells commonly give child processes only 256 descriptors. SQLite WAL
/// files, HTTP connections and WebSockets all share that per-process budget.
pub fn configure() {
    #[cfg(unix)]
    {
        let mut limits = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // SAFETY: getrlimit writes to a valid rlimit; setrlimit borrows that value.
        if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limits) } != 0 {
            tracing::warn!(error = %std::io::Error::last_os_error(), "cannot read open-file limit");
            return;
        }
        let target = limits.rlim_max.min(8192);
        if limits.rlim_cur < target {
            limits.rlim_cur = target;
            if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limits) } != 0 {
                tracing::warn!(error = %std::io::Error::last_os_error(), "cannot raise open-file limit; increase the shell or service limit");
                return;
            }
        }
        tracing::info!(
            soft = limits.rlim_cur,
            hard = limits.rlim_max,
            "open-file limit"
        );
        if limits.rlim_cur < 8192 {
            tracing::warn!(
                "open-file hard limit is below 8192; increase the shell or service limit for more concurrent clients"
            );
        }
    }
}
