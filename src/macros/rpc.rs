//! Macros for generating robust RPC method wrappers with retry and failover logic.

/// Generates a robust RPC method wrapper with retry and failover, with automatic documentation.
///
/// # Variants
///
/// ## Basic (auto-generates full documentation)
/// ```ignore
/// robust_rpc!(
///     /// Short description of the method.
///     fn method_name() -> ReturnType
/// );
/// ```
///
/// ## With arguments (pass argument docs manually)
/// ```ignore
/// robust_rpc!(
///     /// Short description of the method.
///     /// # Arguments
///     ///
///     /// * `block_id` - The block identifier to fetch.
///     fn method_name(block_id: BlockId) -> ReturnType
/// );
/// ```
///
/// ## With additional error documentation
/// ```ignore
/// robust_rpc!(
///     error = "[`Error::BlockNotFound`] - if the block is not available."
///     /// Short description of the method.
///     fn method_name(block_id: BlockId) -> ReturnType; or BlockNotFound
/// );
/// ```
///
/// ## Clone arguments (specify which args to clone)
/// ```ignore
/// robust_rpc!(
///     /// Short description of the method.
///     @clone [arg]
///     fn method_name(arg: ArgType) -> ReturnType
/// );
/// ```
///
/// ## With generics
/// ```ignore
/// robust_rpc!(
///     /// Short description of the method.
///     fn method_name<T: SomeTrait>(arg: T) -> ReturnType
/// );
/// ```
#[allow(unused_macros)]
macro_rules! robust_rpc {
    // Main pattern with optional error doc: zero or more args, optional error variant
    (
        $(error = $error_doc:literal)?
        $(#[$meta:meta])*
        fn $method:ident $(<$generic:ident: $bound:path>)? ($($($arg:ident: $arg_ty:ty),+)?) -> $ret:ty $(; or $err:ident)?
    ) => {
        #[doc = concat!("This is a wrapper function for [`Provider::", stringify!($method), "`].")]
        ///
        $(#[$meta])*
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        $(
        #[doc = concat!("* ", $error_doc)]
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
        $(error = $error_doc:literal)?
        $(#[$meta:meta])*
        @clone [$($clone_arg:ident),+]
        fn $method:ident $(<$generic:ident: $bound:path>)? (
            $($arg:ident: $arg_ty:ty),+
        ) -> $ret:ty $(; or $err:ident)?
    ) => {
        $(#[$meta])*
        ///
        #[doc = concat!("This is a wrapper function for [`Provider::", stringify!($method), "`].")]
        ///
        /// # Errors
        ///
        /// * [`Error::RpcError`] - if no fallback providers succeeded; contains the last error returned
        ///   by the last provider attempted on the last retry.
        /// * [`Error::Timeout`] - if the overall operation timeout elapses (i.e. exceeds
        ///   `call_timeout`).
        $(
        #[doc = concat!("* ", $error_doc)]
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
