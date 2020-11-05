use std::future::{ready, Ready};
use std::sync::Arc;
use std::{fmt, ops};

use actix_web::{dev::Payload, http::StatusCode, web::HttpRequest, FromRequest, ResponseError};
use derive_more::{Display, From};
use serde::de;

/// Extract information from the request's query using `queryst`.
///
/// **Note**: This extractor doesn't support anything beside strings as values ex: numbers
///
/// [**QueryStConfig**](struct.QueryStConfig.html) allows to configure extraction process.
///
/// ## Example
///
/// ```rust
/// use actix_web::{web, App};
/// use actix_web_queryst::{QuerySt};
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// pub enum ResponseType {
///    Token,
///    Code
/// }
///
/// #[derive(Deserialize)]
/// pub struct AuthRequest {
///    id: u64,
///    response_type: ResponseType,
/// }
///
/// // Use `QuerySt` extractor for query information (and destructure it within the signature).
/// // This handler gets called only if the request's query string contains an `id` field.
/// // The correct request for this handler would be `/index.html?id=64&response_type=Code"`.
/// async fn index(QuerySt(info): QuerySt<AuthRequest>) -> String {
///     format!("Authorization request for client with id={} and type={:?}!", info.id, info.response_type)
/// }
///
/// fn main() {
///     let app = App::new().service(
///        web::resource("/index.html").route(web::get().to(index)));
/// }
/// ```
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct QuerySt<T>(pub T);

impl<T> QuerySt<T> {
    /// Deconstruct to a inner value
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Get query parameters from the path
    pub fn from_query(query_str: &str) -> Result<Self, QueryStPayloadError>
    where
        T: de::DeserializeOwned,
    {
        let value = queryst::parse(query_str).map_err(QueryStPayloadError::DeserializeValue)?;
        serde_json::from_value(value)
            .map_err(QueryStPayloadError::DeserializeType)
            .map(QuerySt)
    }
}

impl<T> ops::Deref for QuerySt<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::DerefMut for QuerySt<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Debug> fmt::Debug for QuerySt<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: fmt::Display> fmt::Display for QuerySt<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Extract information from the request's query using `queryst`.
///
/// **Note**: This extractor doesn't support anything beside strings as values ex: numbers
///
/// ## Example
///
/// ```rust
/// use actix_web::{web, App};
/// use actix_web_queryst::QuerySt;
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// pub enum ResponseType {
///    Token,
///    Code
/// }
///
/// #[derive(Deserialize)]
/// pub struct AuthRequest {
///    id: u64,
///    response_type: ResponseType,
/// }
///
/// // Use `QuerySt` extractor for query information.
/// // This handler get called only if request's query contains `id` field
/// // The correct request for this handler would be `/index.html?id=64&response_type=Code"`
/// async fn index(info: QuerySt<AuthRequest>) -> String {
///     format!("Authorization request for client with id={} and type={:?}!", info.id, info.response_type)
/// }
///
/// fn main() {
///     let app = App::new().service(
///        web::resource("/index.html")
///            .route(web::get().to(index))); // <- use `Query` extractor
/// }
/// ```
impl<T> FromRequest for QuerySt<T>
where
    T: de::DeserializeOwned,
{
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;
    type Config = QueryStConfig;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let error_handler = req
            .app_data::<Self::Config>()
            .map(|c| c.ehandler.clone())
            .unwrap_or(None);
        let r = Self::from_query(req.query_string()).map_err(|e| {
            log::debug!(
                "Failed during QuerySt extractor deserialization. \
                     Request path: {:?}",
                req.path()
            );
            if let Some(error_handler) = error_handler {
                (error_handler)(e, req)
            } else {
                e.into()
            }
        });
        ready(r)
    }
}

/// QuerySt extractor configuration
///
/// **Note**: This extractor doesn't support anything beside strings as values ex: numbers
///
/// ## Example
///
/// ```rust
/// use actix_web::{error, web, App, FromRequest, HttpResponse};
/// use actix_web_queryst::{QuerySt, QueryStConfig};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     username: String,
/// }
///
/// /// deserialize `Info` from request's query
/// async fn index(info: QuerySt<Info>) -> String {
///     format!("Welcome {}!", info.username)
/// }
///
/// fn main() {
///     let app = App::new().service(
///         web::resource("/index.html").app_data(
///             // change QuerySt extractor configuration
///             QueryStConfig::default()
///                 .error_handler(|err, req| {  // <- create custom error response
///                     error::InternalError::from_response(
///                         err, HttpResponse::Conflict().finish()).into()
///                 })
///             )
///             .route(web::post().to(index))
///     );
/// }
/// ```
#[derive(Clone)]
pub struct QueryStConfig {
    ehandler:
        Option<Arc<dyn Fn(QueryStPayloadError, &HttpRequest) -> actix_web::Error + Send + Sync>>,
}

impl QueryStConfig {
    /// Set custom error handler
    pub fn error_handler<F>(mut self, f: F) -> Self
    where
        F: Fn(QueryStPayloadError, &HttpRequest) -> actix_web::Error + Send + Sync + 'static,
    {
        self.ehandler = Some(Arc::new(f));
        self
    }
}

impl Default for QueryStConfig {
    fn default() -> Self {
        QueryStConfig { ehandler: None }
    }
}

/// A set of errors that can occur during parsing query strings
#[derive(Debug, Display, From)]
pub enum QueryStPayloadError {
    /// Error in deserialization to json values
    #[display(fmt = "QuerySt invalid query provided: {:?}", _0)]
    DeserializeValue(queryst::ParseError),

    /// Error in deserialization from json values to the provided type
    #[display(fmt = "QuerySt error in deserializing to type: {}", _0)]
    DeserializeType(serde_json::Error),
}

impl std::error::Error for QueryStPayloadError {}

/// Return `BadRequest` for `QueryStPayloadError`
impl ResponseError for QueryStPayloadError {
    fn status_code(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use actix_web::http::StatusCode;
    use derive_more::Display;
    use serde::Deserialize;

    use super::*;
    use actix_web::error::InternalError;
    use actix_web::test::TestRequest;
    use actix_web::HttpResponse;

    #[derive(Deserialize, Debug, Display)]
    struct Id {
        id: String,
    }

    #[derive(Deserialize, Debug)]
    struct User {
        name: String,
        #[serde(rename = "sib")]
        siblings: Vec<String>,
        abblities: HashMap<String, String>,
    }

    impl std::fmt::Display for User {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{}, s:{} +:{}",
                self.name,
                self.siblings.len(),
                self.abblities.len()
            )
        }
    }

    #[actix_rt::test]
    async fn test_service_request_extract() {
        let req = TestRequest::with_uri("/name/user1/").to_srv_request();
        assert!(QuerySt::<Id>::from_query(&req.query_string()).is_err());

        let req = TestRequest::with_uri("/name/user1/?id=test").to_srv_request();
        let mut s = QuerySt::<Id>::from_query(&req.query_string()).unwrap();

        assert_eq!(s.id, "test");
        assert_eq!(format!("{}, {:?}", s, s), "test, Id { id: \"test\" }");

        s.id = "test1".to_string();
        let s = s.into_inner();
        assert_eq!(s.id, "test1");
    }

    #[actix_rt::test]
    async fn test_request_extract() {
        let req = TestRequest::with_uri("/name/user1/").to_srv_request();
        let (req, mut pl) = req.into_parts();
        assert!(QuerySt::<Id>::from_request(&req, &mut pl).await.is_err());

        let req = TestRequest::with_uri("/name/user1/?id=test").to_srv_request();
        let (req, mut pl) = req.into_parts();

        let mut s = QuerySt::<Id>::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(s.id, "test");
        assert_eq!(format!("{}, {:?}", s, s), "test, Id { id: \"test\" }");

        s.id = "test1".to_string();
        let s = s.into_inner();
        assert_eq!(s.id, "test1");
    }

    #[actix_rt::test]
    async fn test_complicated_request_extract() {
        let req = TestRequest::with_uri("/name/user1/").to_srv_request();
        let (req, mut pl) = req.into_parts();
        assert!(QuerySt::<User>::from_request(&req, &mut pl).await.is_err());

        let req = TestRequest::with_uri(
            "/name/user1/?name=test&sib[]=hasan&sib[]=ahmad&abblities[reads]=books",
        )
        .to_srv_request();
        let (req, mut pl) = req.into_parts();

        let mut s = QuerySt::<User>::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(s.name, "test");
        assert_eq!(
            format!("{}, {:?}", s, s),
             "test, s:2 +:1, \
             User { name: \"test\", siblings: [\"hasan\", \"ahmad\"], abblities: {\"reads\": \"books\"} }"
        );

        s.name = "test1".to_string();
        let s = s.into_inner();
        assert_eq!(s.name, "test1");
    }

    #[actix_rt::test]
    async fn test_custom_error_responder() {
        let req = TestRequest::with_uri("/name/user1/")
            .app_data(QueryStConfig::default().error_handler(|e, _| {
                let resp = HttpResponse::UnprocessableEntity().finish();
                InternalError::from_response(e, resp).into()
            }))
            .to_srv_request();

        let (req, mut pl) = req.into_parts();
        let query = QuerySt::<Id>::from_request(&req, &mut pl).await;

        assert!(query.is_err());
        assert_eq!(
            query
                .unwrap_err()
                .as_response_error()
                .error_response()
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
