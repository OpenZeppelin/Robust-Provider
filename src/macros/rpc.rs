//! Macros for generating robust RPC method wrappers with retry and failover logic.

/// Generates a robust RPC method wrapper with retry and failover.
///
/// # Variants
///
/// ## No arguments
/// ```ignore
/// robust_rpc!(
///     /// Doc comment
///     fn method_name() -> ReturnType
/// );
/// ```
///
/// ## Copy arguments
/// ```ignore
/// robust_rpc!(
///     /// Doc comment
///     fn method_name(arg: ArgType) -> ReturnType
/// );
/// ```
///
/// ## Clone arguments (specify which args to clone)
/// ```ignore
/// robust_rpc!(
///     /// Doc comment
///     @clone [arg]
///     fn method_name(arg: ArgType) -> ReturnType
/// );
///
/// robust_rpc!(
///     /// Doc comment
///     @clone [arg2]
///     fn method_name(arg1: CopyType, arg2: CloneType) -> ReturnType
/// );
///
/// robust_rpc!(
///     /// Doc comment
///     @clone [arg1, arg2]
///     fn method_name(arg1: CloneType, arg2: CloneType) -> ReturnType
/// );
/// ```
///
/// ## With Option unwrapping (errors on None)
/// ```ignore
/// robust_rpc!(
///     /// Doc comment
///     fn method_name(arg: ArgType) -> ReturnType; or ErrorVariant
/// );
/// ```
///
/// ## With generics
/// ```ignore
/// robust_rpc!(
///     /// Doc comment
///     fn method_name<T: SomeTrait>(arg: T) -> ReturnType
/// );
/// ```
#[allow(unused_macros)]
macro_rules! robust_rpc {
    // Main pattern: zero or more args, optional error variant
    (
        $(#[$meta:meta])*
        fn $method:ident $(<$generic:ident: $bound:path>)? ($($($arg:ident: $arg_ty:ty),+)?) -> $ret:ty $(; or $err:ident)?
    ) => {
        $(#[$meta])*
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
        $(#[$meta:meta])*
        @clone [$($clone_arg:ident),+]
        fn $method:ident $(<$generic:ident: $bound:path>)? (
            $($arg:ident: $arg_ty:ty),+
        ) -> $ret:ty $(; or $err:ident)?
    ) => {
        $(#[$meta])*
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
