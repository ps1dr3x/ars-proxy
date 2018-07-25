extern crate futures;
extern crate native_tls;
extern crate tokio;
extern crate tokio_tls;
extern crate hyper;
extern crate hyper_tls;

mod utils;

use std::{
    path::Path,
    fs::File,
    io::Read,
    sync::mpsc::{
        Sender,
        Receiver,
        channel
    }
};
use futures::{
    future,
    stream::Stream
};
use native_tls::{ Identity, TlsAcceptor };
use tokio::net::TcpListener;
use tokio_tls::TlsAcceptorExt;
use hyper::{
    Uri,
    Body,
    Request,
    Response,
    Client,
    Server,
    server::conn::Http,
    service::service_fn,
    header::HeaderValue,
    rt::{
        self,
        Future
    }
};
use hyper_tls::HttpsConnector;
use utils::Conf;

type BoxFut = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;

pub const USAGE: &'static str = "\nUsage:\nars-proxy <local_port> <remote_url> <remote_port> [--cert <crt_path> --pass-file <pass_file_path>] [--to-https]\n";

fn main() {
    println!("\nars-proxy v0.1.0");

    let conf = utils::get_cli_params();
    if conf.is_err() {
        println!("\nError: {}\n{}", conf.err().unwrap(), USAGE);
        ::std::process::exit(1);
    }
    let conf = conf.unwrap();

    let service_conf = conf.clone();
    let service = move || {
        let conf = service_conf.clone();

        service_fn(move |req| {
            proxy(conf.clone(), req)
        })
    };

    let local_addr = ([127, 0, 0, 1], conf.local_port).into();

    if conf.https_crt.is_some() {
        let crt_path = conf.https_crt.unwrap();
        let mut crt_file = File::open(
            Path::new(&crt_path)
        ).expect(&format!("Certificate file \"{}\" not found (or not accessible)", crt_path));
        let mut identity = vec![];
        crt_file.read_to_end(&mut identity).unwrap();

        let mut pass = vec![];
        if conf.https_crt_pass_file.is_some() {
            let crt_pass_file_path = conf.https_crt_pass_file.unwrap();
            let mut crt_pass_file = File::open(
                Path::new(&crt_pass_file_path)
            ).expect(&format!("Certificate pass file \"{}\" not found (or not accessible)", crt_pass_file_path));
            crt_pass_file.read_to_end(&mut pass).unwrap();
        }

        let cert = Identity::from_pkcs12(&identity, &String::from_utf8(pass).unwrap())
            .expect("Error while opening certificate file (maybe wrong password?)");
        let tls_cx = TlsAcceptor::builder(cert).build().unwrap();

        let srv = TcpListener::bind(&local_addr)
            .expect(&format!("Error binding local port: {}", conf.local_port));

        let http_proto = Http::new();
        let http_server = http_proto
            .serve_incoming(
                srv.incoming().and_then(move |socket| {
                    tls_cx
                        .accept_async(socket)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                }),
                service,
            )
            .then(|res| {
                match res {
                    Ok(conn) => Ok(Some(conn)),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        Ok(None)
                    },
                }
            })
            .for_each(|conn_opt| {
                if let Some(conn) = conn_opt {
                    hyper::rt::spawn(
                        conn.and_then(|c| c.map_err(|e| panic!("Hyper error {}", e)))
                            .map_err(|e| eprintln!("Connection error {}", e)),
                    );
                }
                Ok(())
            });

        println!(
            "\nListening on https://{}\nProxying to https://{}:{}",
            local_addr,
            conf.remote_url,
            conf.remote_port
        );

        rt::run(http_server);
    } else {
        let server = Server::bind(&local_addr)
            .serve(service)
            .map_err(|e| eprintln!("Server error: {}", e));

        println!(
            "\nListening on http://{}\nProxying to {}://{}:{}",
            local_addr,
            if conf.to_https { "https" } else { "http" },
            conf.remote_url,
            conf.remote_port
        );

        rt::run(server);
    }
}

fn proxy(conf: Conf, req: Request<Body>) -> BoxFut {
    let (tx, rx): (Sender<Response<Body>>, Receiver<Response<Body>>) = channel();
    let tx_err = tx.clone();

    let url = format!(
        "{}://{}:{}{}",
        if conf.to_https || conf.https_crt.is_some() {
            "https"
        } else {
            "http"
        },
        conf.remote_url,
        conf.remote_port,
        req.uri()
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
