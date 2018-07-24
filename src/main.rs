extern crate futures;
extern crate hyper;
extern crate hyper_tls;

use std::{
    env,
    sync::mpsc::{
        Sender,
        Receiver,
        channel
    }
};
use futures::future;
use hyper::{
    Uri,
    Body,
    Request,
    Response,
    Client,
    Server,
    service::service_fn,
    header::HeaderValue,
    rt::{
        self,
        Future
    }
};
use hyper_tls::HttpsConnector;

type BoxFut = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;

#[derive(Debug, Clone)]
pub struct Conf {
    pub local_port: u16,
    pub remote_url: String,
    pub remote_port: u16,
    pub to_https: bool
}

const USAGE: &'static str = "\nUsage:\nars-proxy <local_port> <remote_url> <remote_port> [--to-https]\n";

fn main() {
    let mut args = env::args();
    let conf = Conf {
        local_port: args
            .nth(1)
            .expect(USAGE)
            .parse()
            .expect(&format!("Error while parsing local_port argument {}", USAGE)),
        remote_url: args
            .next()
            .expect(USAGE),
        remote_port: args
            .next()
            .expect(USAGE)
            .parse()
            .expect(&format!("Error while parsing remote_port argument {}", USAGE)),
        to_https: {
            let arg = args.next();
            if arg.is_none() {
                false
            } else {
                let arg = arg.unwrap();
                if arg != "--to-https" {
                    println!("Invalid parameter: {}", arg);
                    panic!(USAGE);
                }
                true
            }
        }
    };

    let service_conf = conf.clone();
    let service = move || {
        let conf = service_conf.clone();

        service_fn(move |req| {
            proxy(conf.clone(), req)
        })
    };

    let local_addr = ([127, 0, 0, 1], conf.local_port).into();
    let server = Server::bind(&local_addr)
        .serve(service)
        .map_err(|e| eprintln!("Server error: {}", e));

    println!(
        "Listening on http://{}\nProxying to {}://{}:{}",
        local_addr,
        if conf.to_https { "https" } else { "http" },
        conf.remote_url,
        conf.remote_port
    );

    rt::run(server);
}

fn proxy(conf: Conf, req: Request<Body>) -> BoxFut {
    let (tx, rx): (Sender<Response<Body>>, Receiver<Response<Body>>) = channel();
    let tx_err = tx.clone();

    let url = format!(
        "{}://{}:{}",
        if conf.to_https { "https" } else { "http" },
        conf.remote_url,
        conf.remote_port
    ).parse().unwrap();

    let req = request(req, url)
        .map(move |res| {
            let (parts, body) = res.into_parts();
            tx.send(Response::from_parts(parts, body)).unwrap();
        })
        .map_err(move |e| {
            eprintln!("Proxied request error: {}", e);
            tx_err.send(Response::new(Body::from(e.to_string()))).unwrap();
        });

    rt::spawn(req);

    let response = rx.recv().unwrap();
    Box::new(future::ok(response))
}

fn request(req: Request<Body>, url: Uri) -> impl Future<Item=Response<Body>, Error=hyper::Error> {
    let (mut parts, body) = req.into_parts();

    if parts.headers.contains_key("host") {
        parts.headers.remove("host");
        let new_host = &[
            url.host().unwrap(),
            &url.port().unwrap().to_string()
        ].join(":");
        parts.headers.insert("host", HeaderValue::from_str(new_host).unwrap());
    }

    let mut proxied_req = Request::new(body);
    *proxied_req.method_mut() = parts.method;
    *proxied_req.uri_mut() = url;
    *proxied_req.headers_mut() = parts.headers;

    let https = HttpsConnector::new(4).expect("TLS initialization failed");
    let client = Client::builder()
        .build::<_, hyper::Body>(https);
    client.request(proxied_req)
}
