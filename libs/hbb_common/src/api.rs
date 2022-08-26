use async_once::AsyncOnce;
use lazy_static::lazy_static;

const API_URI: &'static str = "https://api.hoptodesk.com/";

#[derive(Debug, Clone)]
pub struct ApiError(String);

impl<E: std::error::Error> From<E> for ApiError {
    fn from(e: E) -> Self {
        Self(e.to_string())
    }
}

pub async fn call_api() -> Result<serde_json::Value, ApiError> {
    lazy_static! {
        static ref RESPONSE: AsyncOnce<Result<serde_json::Value, ApiError>> =
            AsyncOnce::new(async {
                let body = reqwest::get(API_URI).await?.text().await?;
                let body = serde_json::from_str(&body)?;

                Ok(body)
            });
    }

    (*RESPONSE.get().await).clone()
}
