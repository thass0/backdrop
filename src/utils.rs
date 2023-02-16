pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current =  cause.source();
    }
    Ok(())
}

macro_rules! derive_error_chain_fmt {
    ($name:ident) => {
        // Debug message displaying the sources of the error.
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                use crate::utils::error_chain_fmt;
                error_chain_fmt(self, f)
            }
        }
    };
}
pub(crate) use derive_error_chain_fmt;

pub fn e500<T>(e: T) -> actix_web::Error
where
    T: std::fmt::Debug + std::fmt::Display + 'static
{
    actix_web::error::ErrorInternalServerError(e)
}
