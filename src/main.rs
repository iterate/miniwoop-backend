#![feature(slice_patterns)]

extern crate futures;
extern crate hyper;
extern crate pretty_env_logger;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate unicase;
extern crate uuid;
extern crate chrono;

use std::sync::{Arc, Mutex};
use std::vec::Vec;
use std::collections::HashMap;


use futures::future;
use futures::{Stream, Future, BoxFuture};
use hyper::{Get, Post, StatusCode};
use hyper::Method;
use hyper::mime;
use hyper::header;
use hyper::Headers;
use hyper::server::{Http, Service, Request, Response};
use serde::ser::Serialize;
use uuid::Uuid;
use chrono::{DateTime, UTC};

#[derive(Serialize, Clone)]
struct Message {
    id: Uuid,
    text: String,
    woops: i32,
    created: DateTime<UTC>,
    user: String,
}

impl From<MessageIncoming> for Message {
    fn from(msg: MessageIncoming) -> Message {
        Message {
            text: msg.text,
            woops: 0,
            id: Uuid::new_v4(),
            created: UTC::now(),
            user: msg.user.unwrap_or_else(|| "Anonym".to_string()),
        }
    }
}

#[derive(Deserialize)]
struct MessageIncoming {
    text: String,
    user: Option<String>,
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
    messages: Arc<Mutex<HashMap<Uuid, Message>>>,
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
                    .with_body("Try /messages")).boxed()
            },
            (&Get, &["messages"]) => {
                let messages = self.messages.lock().unwrap();
                let mut messages_vec: Vec<_> = messages.values().collect();
                messages_vec.sort_by_key(|m| m.created);
                future::ok(
                    json_response(&messages_vec)
                        .with_headers(default_headers)
                ).boxed()
            },
            (&Post, &["messages"]) => {
                let messages = self.messages.clone();
                req.body().concat2().map(move |body| {
                    let body: MessageIncoming = match serde_json::from_slice(&body) {
                        Ok(body) => body,
                        Err(_) => {
                            return Response::new()
                                .with_status(StatusCode::BadRequest)
                                .with_headers(default_headers)
                                .with_body("Could not decode JSON");
                        }
                    };

                    let message: Message = body.into();

                    messages.lock().unwrap().insert(message.id, message.clone());

                    json_response(&message)
                        .with_headers(default_headers)
                }).boxed()
            },
            (&Post, &["messages", message_id, "woop"]) => {
                let messages = self.messages.clone();
                let message_id:Uuid = match message_id.parse() {
                    Ok(id) => id,
                    Err(_) => {
                        return futures::future::ok(Response::new()
                            .with_headers(default_headers)
                            .with_status(StatusCode::NotFound)).boxed()
                    }
                };

                let mut messages = messages.lock().unwrap();

                let message = match messages.get_mut(&message_id) {
                    Some(message) => message,
                    None => return futures::future::ok(Response::new()
                        .with_headers(default_headers)
                        .with_status(StatusCode::NotFound)).boxed(),
                };

                message.woops += 1;

                future::ok(json_response(&message)
                        .with_headers(default_headers)).boxed()
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

    let messages = Arc::new(Mutex::new(HashMap::new()));

    let server = Http::new().bind(&addr, move || Ok(MiniWoopServer {
        messages: messages.clone(),
    })).unwrap();
    println!("Listening on http://{} with 1 thread.", server.local_addr().unwrap());
    server.run().unwrap();
}