use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use actix_service::{Service, Transform};
use actix_web::{dev::ServiceRequest, dev::ServiceResponse, web::Data, Error, HttpResponse};
use bb8_postgres::{bb8::Pool, tokio_postgres::NoTls, PostgresConnectionManager};
use futures::future::{ok, Ready};
use futures::Future;

type PgPool = Pool<PostgresConnectionManager<NoTls>>;

pub struct RequiresAuth;

// Middleware factory is `Transform` trait from actix-service crate
// `S` - type of the next service
// `B` - type of response's body
impl<S: 'static, B> Transform<S> for RequiresAuth
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = RequiresAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(RequiresAuthMiddleware {
            service: Rc::new(RefCell::new(service)),
        })
    }
}

pub struct RequiresAuthMiddleware<S> {
    service: Rc<RefCell<S>>,
}

impl<S, B> Service for RequiresAuthMiddleware<S>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        let mut svc = self.service.clone();

        Box::pin(async move {
            let header = req.headers().get("Authorization");
            let is_valid = match header {
                Some(header_val) => match header_val.to_str() {
                    Ok(val) => {
                        let pool = req.app_data::<Data<PgPool>>().unwrap();
                        let conn = pool.get().await.unwrap();
                        let results = conn
                            .query(r#"SELECT "id" FROM users WHERE "key"=$1;"#, &[&val])
                            .await
                            .unwrap();
                        results.is_empty()
                    }
                    Err(_) => false,
                },
                None => false,
            };

            if is_valid {
                Ok(svc.call(req).await.unwrap())
            } else {
                Ok(req.into_response(HttpResponse::Forbidden().finish().into_body()))
            }
        })
    }
}

pub struct RequiresOwnership;

// Middleware factory is `Transform` trait from actix-service crate
// `S` - type of the next service
// `B` - type of response's body
impl<S: 'static, B> Transform<S> for RequiresOwnership
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = RequiresOwnershipMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(RequiresOwnershipMiddleware {
            service: Rc::new(RefCell::new(service)),
        })
    }
}

pub struct RequiresOwnershipMiddleware<S> {
    service: Rc<RefCell<S>>,
}

impl<S, B> Service for RequiresOwnershipMiddleware<S>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        let mut svc = self.service.clone();

        Box::pin(async move {
            let header = req.headers().get("Authorization");
            let request_path = &req.path()[1..];
            let is_valid = match header {
                Some(header_val) => match header_val.to_str() {
                    Ok(val) => {
                        let pool = req.app_data::<Data<PgPool>>().unwrap();
                        let conn = pool.get().await.unwrap();
                        let results =
                            conn.query(
                                r#"SELECT "id" FROM users JOIN uploads ON uploads."uploader"=users."id" WHERE uploads."file_path"=$1 AND users."key"=$2;"#,
                                &[&request_path, &val]).await.unwrap();
                        results.is_empty()
                    }
                    Err(_) => false,
                },
                None => false,
            };

            if is_valid {
                Ok(svc.call(req).await.unwrap())
            } else {
                Ok(req.into_response(HttpResponse::Forbidden().finish().into_body()))
            }
        })
    }
}
