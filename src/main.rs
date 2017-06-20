#![feature(slice_patterns)]

extern crate futures;
extern crate hyper;
extern crate pretty_env_logger;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate unicase;

use std::sync::{Arc, Mutex};
use std::vec::Vec;


use futures::future;
use futures::{Stream, Future, BoxFuture};
use hyper::{Get, Post, StatusCode};
use hyper::Method;
use hyper::mime;
use hyper::header;
use hyper::Headers;
use hyper::server::{Http, Service, Request, Response};
use serde::ser::Serialize;

#[derive(Serialize, Deserialize)]
struct Message {
    text: String,
}

fn json_response<T: Serialize>(data: &T) -> Response {
    match serde_json::to_vec(data) {
        Ok(serialized) => Response::new()
            .with_body(serialized),
        Err(_) => Response::new()
            .with_header(header::ContentType(mime::APPLICATION_JSON))
            .with_status(StatusCode::InternalServerError)
            .with_body("Could not encode JSON")
    }
}

struct MiniWoopServer {
    messages: Arc<Mutex<Vec<Message>>>,
}

impl Service for MiniWoopServer {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = BoxFuture<Response, hyper::Error>;

    fn call(&self, req: Request) -> Self::Future {
        let parts:Vec<_> = req.path().split_terminator("/")
            .skip(1)
            .map(|s| s.to_string()).collect();
        let parts_str:Vec<_> = parts.iter().map(|s| s.as_str()).collect();
        let parts_slice = parts_str.as_slice();

        let mut default_headers = Headers::new();
        default_headers.set(header::AccessControlAllowOrigin::Any);
        default_headers.set(header::AccessControlAllowHeaders(vec![unicase::Ascii::new("content-type".to_owned())]));
        default_headers.set(header::AccessControlMaxAge(3600));
        default_headers.set(header::AccessControlAllowMethods(vec![Method::Get, Method::Post, Method::Put]));

        if req.method() == &Method::Options {
            return futures::future::ok(Response::new()
                .with_headers(default_headers)).boxed();
        }


        match (req.method(), parts_slice) {
            (&Get, &[]) => {
                futures::future::ok(Response::new()
                    .with_body("POST / GET /messages")).boxed()
            },
            (&Get, &["messages"]) => {
                future::ok(
                    json_response(&self.messages.lock().unwrap() as &Vec<Message>)
                        .with_headers(default_headers)
                ).boxed()
            },
            (&Post, &["messages"]) => {
                let messages = self.messages.clone();
                req.body().concat2().map(move |body| {
                    let body: Message = match serde_json::from_slice(&body) {
                        Ok(body) => body,
                        Err(_) => {
                            return Response::new()
                                .with_status(StatusCode::BadRequest)
                                .with_headers(default_headers)
                                .with_body("Could not decode JSON");
                        }
                    };

                    messages.lock().unwrap().push(body);
                    let msgs = &messages.lock().unwrap();

                    match serde_json::to_vec(msgs as &Vec<Message>) {
                        Ok(serialized) => Response::new()
                            .with_body(serialized.clone()),
                        Err(_) => Response::new()
                            .with_header(header::ContentType(mime::APPLICATION_JSON))
                            .with_status(StatusCode::InternalServerError)
                            .with_body("Could not encode JSON")
                    }.with_headers(default_headers)
                }).boxed()
            },
            _ => {
                futures::future::ok(Response::new()
                    .with_headers(default_headers)
                    .with_status(StatusCode::NotFound)).boxed()
            }
        }
    }

}


fn main() {
    pretty_env_logger::init().unwrap();
    let addr = "0.0.0.0:5000".parse().unwrap();

    let messages = Arc::new(Mutex::new(Vec::new()));

    let server = Http::new().bind(&addr, move || Ok(MiniWoopServer {
        messages: messages.clone(),
    })).unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    server.run().unwrap();
}