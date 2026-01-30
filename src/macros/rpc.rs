//! Macros for generating robust RPC method wrappers with retry and failover logic.

/// Generates a robust RPC method wrapper with retry and failover, with automatic documentation.
///
/// # Variants
///
/// ## Basic (no arguments)
/// ```ignore
/// robust_rpc!(
///     fn method_name() -> ReturnType
/// );
/// ```
///
/// ## With argument documentation
/// ```ignore
/// robust_rpc!(
///     doc_args = [(block_id, "The block identifier to fetch.")]
///     fn method_name(block_id: BlockId) -> ReturnType
/// );
///
/// robust_rpc!(
///     doc_args = [
///         (address, "The address of the account."),
///         (keys, "A vector of storage keys to include in the proof.")
///     ]
///     fn method_name(address: Address, keys: Vec<StorageKey>) -> ReturnType
/// );
/// ```
///
/// ## With additional error documentation
/// ```ignore
/// robust_rpc!(
///     doc_include_error = ["[`Error::BlockNotFound`] - if the block is not available."]
///     doc_args = [(block_id, "The block identifier.")]
///     fn method_name(block_id: BlockId) -> ReturnType; or BlockNotFound
/// );
/// ```
///
/// ## Clone arguments (specify which args to clone)
/// ```ignore
/// robust_rpc!(
///     doc_args = [(tx, "The transaction request.")]
///     @clone [tx]
///     fn method_name(tx: TransactionRequest) -> ReturnType
/// );
/// ```
///
/// ## With generics
/// ```ignore
/// robust_rpc!(
///     doc_args = [(filter_id, "The filter ID.")]
///     fn method_name<T: SomeTrait>(filter_id: U256) -> Vec<T>
/// );
/// ```
macro_rules! robust_rpc {
    // Main pattern: optional doc_errors, optional doc_args, zero or more fn doc_args, optional error variant
    (
        $(doc_include_error = [$($error_doc:tt)+])?
        $(doc_args = [$(($arg_name:ident, $arg_desc:literal)),* $(,)?])?
        fn $method:ident $(<$generic:ident: $bound:path>)? ($($($arg:ident: $arg_ty:ty),+)?) -> $ret:ty $(; or $err:ident)?
    ) => {
        #[doc = concat!("This is a wrapper function for [`Provider::", stringify!($method), "`].")]
        $($(
        ///
        /// # Arguments
        ///
        #[doc = concat!("* `", stringify!($arg_name), "` - ", $arg_desc)]
        )*)?
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        $(
        #[doc = $($error_doc)+]
        )?
        pub async fn $method $(<$generic: $bound>)? (&self $(, $($arg: $arg_ty),+)?) -> Result<$ret, Error> {
            let result = self
                .try_operation_with_failover(
                    move |provider| async move {
                        provider.$method $(::<$generic>)? ($($($arg),+)?).await
                    },
                    false,
                )
                .await;
            robust_rpc!(@unwrap result $(, $err)?)
        }
    };

    // Arguments with cloning use with @clone
    (
        $(doc_include_error = [$($error_doc:tt)+])?
        $(doc_args = [$(($arg_name:ident, $arg_desc:literal)),* $(,)?])?
        @clone [$($clone_arg:ident),+]
        fn $method:ident $(<$generic:ident: $bound:path>)? (
            $($arg:ident: $arg_ty:ty),+
        ) -> $ret:ty $(; or $err:ident)?
    ) => {
        #[doc = concat!("This is a wrapper function for [`Provider::", stringify!($method), "`].")]
        $($(
        ///
        /// # Arguments
        ///
        #[doc = concat!("* `", stringify!($arg_name), "` - ", $arg_desc)]
        )*)?
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        $(
        #[doc = $($error_doc)+]
        )?
        pub async fn $method $(<$generic: $bound>)? (&self, $($arg: $arg_ty),+) -> Result<$ret, Error> {
            let result = self
                .try_operation_with_failover(
                    move |provider| {
                        $(let $clone_arg = $clone_arg.clone();)+
                        async move {
                            provider.$method $(::<$generic>)? ($($arg),+).await
                        }
                    },
                    false,
                )
                .await;
            robust_rpc!(@unwrap result $(, $err)?)
        }
    };

    // Internal helper for unwrapping
    (@unwrap $result:expr) => {
        $result.map_err(Error::from)
    };

    (@unwrap $result:expr, $err:ident) => {
        $result?.ok_or(Error::$err)
    };
}

/// Documentation string for `BlockNotFound` errors, for use in `robust_rpc!` macro calls.
///
/// Usage: `error = block_not_found_doc!()`
#[macro_export]
macro_rules! block_not_found_doc {
    () => { "* [`Error::BlockNotFound`] - if the block is not available. This is verified on Anvil, Reth, and Geth; other clients may surface this condition as [`Error::RpcError`]." };
}
