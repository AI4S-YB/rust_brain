pub use tokio_util::sync::CancellationToken;

/// Convenience: check token and return Cancelled error if tripped.
#[macro_export]
macro_rules! bail_if_cancelled {
    ($token:expr) => {
        if $token.is_cancelled() {
            return Err($crate::module::ModuleError::Cancelled);
        }
    };
}
