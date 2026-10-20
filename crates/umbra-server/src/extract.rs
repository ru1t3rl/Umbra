//! Extractors that answer with problem details instead of axum's bare rejections (ADR 0026, m5,
//! m7): [`ApiJson`], [`ApiPath`] and [`ApiQuery`].
//!
//! | Failure | Answer |
//! |---|---|
//! | JSON syntax error, or an empty body | `400 invalid_body` |
//! | JSON data error: unknown field, wrong type, a value out of range | `422 validation_failed` |
//! | missing or wrong `Content-Type` | `415 unsupported_media_type` |
//! | body over the limit | `413 payload_too_large` |
//! | path or query parameter that does not parse | `400 bad_request` |
//! | `limit` that parses but is out of range | `422 validation_failed` |

use std::str::FromStr;

use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{FromRequest, FromRequestParts, Path, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::de::DeserializeOwned;
use umbra_api_types::ProblemCode;
use umbra_engine::handle::EntityQuery;

use crate::problem::ApiError;

/// A JSON request body; a failure is a problem (see the module table).
#[derive(Debug)]
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        match axum::Json::<T>::from_request(request, state).await {
            Ok(axum::Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(json_rejection(&rejection)),
        }
    }
}

fn json_rejection(rejection: &JsonRejection) -> ApiError {
    let code = match rejection {
        JsonRejection::JsonDataError(_) => ProblemCode::ValidationFailed,
        JsonRejection::MissingJsonContentType(_) => ProblemCode::UnsupportedMediaType,
        // A body that could not be read: too large, or a broken connection.
        JsonRejection::BytesRejection(bytes) if bytes.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            ProblemCode::PayloadTooLarge
        }
        // Syntax errors, and any rejection added later (the enum is non-exhaustive).
        _ => ProblemCode::InvalidBody,
    };
    ApiError::new(code, rejection.body_text())
}

/// A path parameter; a value that does not parse is `400 bad_request`.
#[derive(Debug)]
pub struct ApiPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        match Path::<T>::from_request_parts(parts, state).await {
            Ok(Path(value)) => Ok(Self(value)),
            Err(rejection) => Err(path_rejection(&rejection)),
        }
    }
}

fn path_rejection(rejection: &PathRejection) -> ApiError {
    ApiError::new(ProblemCode::BadRequest, rejection.body_text())
}

/// The parameters of a query string, parsed from the raw string with `form_urlencoded` so that an
/// array parameter may be repeated (`ids=1&ids=2`) or comma-separated (`ids=1,2`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApiQuery {
    pairs: Vec<(String, String)>,
}

impl ApiQuery {
    /// Parses a raw query string (without the `?`).
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        Self {
            pairs: form_urlencoded::parse(raw.as_bytes())
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect(),
        }
    }

    /// The value of a scalar parameter.
    ///
    /// # Errors
    /// `400 bad_request` if `key` is given more than once.
    pub fn text(&self, key: &str) -> Result<Option<&str>, ApiError> {
        let mut values = self.pairs.iter().filter(|(k, _)| k == key);
        let first = values.next().map(|(_, v)| v.as_str());
        if values.next().is_some() {
            return Err(bad_request(format!("`{key}` is given more than once")));
        }
        Ok(first)
    }

    /// The value of a scalar parameter, parsed.
    ///
    /// # Errors
    /// `400 bad_request`, naming what was written, if it does not parse or is repeated.
    pub fn number<T: FromStr>(&self, key: &str) -> Result<Option<T>, ApiError> {
        self.text(key)?
            .map(|text| {
                text.parse()
                    .map_err(|_| bad_request(format!("`{key}={text}` is not a valid number")))
            })
            .transpose()
    }

    /// The values of an array parameter: every occurrence, each split at commas, empty parts
    /// skipped.
    #[must_use]
    pub fn list(&self, key: &str) -> Vec<&str> {
        self.pairs
            .iter()
            .filter(|(k, _)| k == key)
            .flat_map(|(_, v)| v.split(','))
            .filter(|part| !part.is_empty())
            .collect()
    }

    /// The `limit` parameter: [`EntityQuery::DEFAULT_LIMIT`] if absent. The one rule of every paged
    /// listing (ASM-193 a, ASM-197).
    ///
    /// # Errors
    /// `400 bad_request` if it is not a number; `422 validation_failed` if it is 0 or above
    /// [`EntityQuery::MAX_LIMIT`] (the engine's clamp is only a backstop).
    pub fn limit(&self) -> Result<usize, ApiError> {
        // Parsed as `u64` so that the answer is the same on 32-bit targets.
        let Some(limit) = self.number::<u64>("limit")? else {
            return Ok(EntityQuery::DEFAULT_LIMIT);
        };
        let max = EntityQuery::MAX_LIMIT;
        match usize::try_from(limit) {
            Ok(limit) if (1..=max).contains(&limit) => Ok(limit),
            _ => Err(ApiError::new(
                ProblemCode::ValidationFailed,
                format!("`limit={limit}` is out of range: it must be from 1 to {max}"),
            )),
        }
    }
}

fn bad_request(detail: String) -> ApiError {
    ApiError::new(ProblemCode::BadRequest, detail)
}

impl<S: Send + Sync> FromRequestParts<S> for ApiQuery {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, ApiError>> + Send {
        std::future::ready(Ok(Self::parse(parts.uri.query().unwrap_or(""))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_defaults_when_absent_and_accepts_the_bounds() {
        assert_eq!(
            ApiQuery::parse("").limit().ok(),
            Some(EntityQuery::DEFAULT_LIMIT)
        );
        assert_eq!(ApiQuery::parse("limit=1").limit().ok(), Some(1));
        let max = EntityQuery::MAX_LIMIT;
        assert_eq!(
            ApiQuery::parse(&format!("limit={max}")).limit().ok(),
            Some(max)
        );
    }

    #[test]
    fn limit_out_of_range_is_422_and_not_clamped() {
        let too_big = format!("limit={}", EntityQuery::MAX_LIMIT + 1);
        for raw in ["limit=0", too_big.as_str(), "limit=99999999999"] {
            let error = ApiQuery::parse(raw).limit().expect_err(raw);
            assert_eq!(error.code(), ProblemCode::ValidationFailed, "{raw}");
        }
    }

    #[test]
    fn limit_that_is_not_a_number_is_400() {
        for raw in [
            "limit=abc",
            "limit=-1",
            "limit=1.5",
            "limit=",
            "limit=1&limit=2",
        ] {
            let error = ApiQuery::parse(raw).limit().expect_err(raw);
            assert_eq!(error.code(), ProblemCode::BadRequest, "{raw}");
        }
    }

    #[test]
    fn number_names_what_the_client_wrote() {
        let error = ApiQuery::parse("after_id=-1")
            .number::<u64>("after_id")
            .expect_err("negative");
        assert_eq!(error.code(), ProblemCode::BadRequest);
        assert!(error.into_problem().detail.contains("after_id=-1"));
        assert_eq!(
            ApiQuery::parse("after_id=7").number::<u64>("after_id").ok(),
            Some(Some(7))
        );
        assert_eq!(
            ApiQuery::parse("x=1").number::<u64>("after_id").ok(),
            Some(None)
        );
    }

    #[test]
    fn arrays_may_be_repeated_or_comma_separated() {
        let repeated = ApiQuery::parse("ids=1&ids=2&other=9");
        let commas = ApiQuery::parse("ids=1,2");
        let mixed = ApiQuery::parse("ids=1,2&ids=3&ids=");
        assert_eq!(repeated.list("ids"), ["1", "2"]);
        assert_eq!(commas.list("ids"), ["1", "2"]);
        assert_eq!(mixed.list("ids"), ["1", "2", "3"]);
        assert_eq!(repeated.list("missing"), Vec::<&str>::new());
    }

    #[test]
    fn values_are_percent_decoded() {
        let query = ApiQuery::parse("name=a%20b&force=r%26d");
        assert_eq!(query.text("name").ok(), Some(Some("a b")));
        assert_eq!(query.text("force").ok(), Some(Some("r&d")));
    }
}
