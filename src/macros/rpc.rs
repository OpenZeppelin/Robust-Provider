//! Macros for generating robust RPC method wrappers with retry and failover logic.

/// Generates a robust RPC method wrapper with retry and failover.
///
/// # Macros allows for the following variants
///
/// ## Simple passthrough (no arguments)
/// ```ignore
/// robust_rpc!(
///     /// Doc comment
///     fn method_name() -> ReturnType
/// );
/// ```
///
/// ## One or more arguments passthrough (Copy types)
/// ```ignore
/// robust_rpc!(
///     /// Doc comment
///     fn method_name(arg: ArgType) -> ReturnType
/// );
/// ```
///
/// ## Single argument with clone (non-Copy types)
/// ```ignore
/// robust_rpc!(
///     /// Doc comment
///     fn method_name(arg: clone ArgType) -> ReturnType
/// );
/// ```
///
/// ## With Option unwrapping (for methods that return Option and should error on None)
/// ```ignore
/// robust_rpc!(
///     /// Doc comment
///     fn method_name(arg: ArgType) -> ReturnType; or BlockNotFound
/// );
/// ```
#[allow(unused_macros)]
macro_rules! robust_rpc {
    // Main pattern: zero or more args, optional error variant
    (
        $(#[$meta:meta])*
        fn $method:ident($($($arg:ident: $arg_ty:ty),+ )?) -> $ret:ty $(; or $err:ident)?
    ) => {
        $(#[$meta])*
        pub async fn $method(&self $(, $($arg: $arg_ty),+)?) -> Result<$ret, Error> {
            let result = self
                .try_operation_with_failover(
                    move |provider| async move { provider.$method($($($arg),+)?).await },
                    false,
                )
                .await;
            robust_rpc!(@unwrap result $(, $err)?)
        }
    };

    // Single arg with clone (non-Copy types)
    (
        $(#[$meta:meta])*
        fn $method:ident($arg:ident: clone $arg_ty:ty) -> $ret:ty
    ) => {
        $(#[$meta])*
        pub async fn $method(&self, $arg: $arg_ty) -> Result<$ret, Error> {
            self.try_operation_with_failover(
                move |provider| {
                    let $arg = $arg.clone();
                    async move { provider.$method($arg).await }
                },
                false,
            )
            .await
            .map_err(Error::from)
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
