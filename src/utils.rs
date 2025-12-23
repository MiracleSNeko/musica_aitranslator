use anyhow::{Result, bail};

pub trait IntoAnyResult<T> {
    fn into_any_result(self) -> Result<T>;
}

impl<T> IntoAnyResult<T> for Option<T> {
    fn into_any_result(self) -> Result<T> {
        match self {
            Some(value) => Ok(value),
            None => bail!("called `Option::unwrap()` on a `None` value"),
        }
    }
}
