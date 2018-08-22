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
    time::{
        SystemTime,
        UNIX_EPOCH
    }
};
use futures::stream::Stream;
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
    println!("\nars-proxy v0.1.0\n");

    let conf = utils::get_cli_params();
    if conf.is_err() {
        println!("\nError: {}\n{}", conf.err().unwrap(), USAGE);
        ::std::process::exit(1);
    }
    let conf = conf.unwrap();

    println!(
        "Starting server on {}://127.0.0.1:{}\nProxying to {}://{}:{}\n",
        if conf.https_crt.is_some() {
            "https"
        } else {
            "http"
        },
        conf.local_port,
        if conf.to_https || conf.https_crt.is_some() {
            "https"
        } else {
            "http"
        },
        conf.remote_url,
        conf.remote_port
    );

    loop { server(conf.clone()) }
}

fn server(conf: Conf) {
    let local_addr = ([127, 0, 0, 1], conf.local_port).into();
    let listener = TcpListener::bind(&local_addr)
        .expect(&format!("Error binding local port: {}", conf.local_port));

    let ps_conf = conf.clone();
    let proxy_service = move || {
        let conf = ps_conf.clone();
        service_fn(move |req| proxy(conf.clone(), req))
    };

    if conf.https_crt.is_some() {
        let conf = conf.clone();
        let tls_stream = listener.incoming()
            .and_then(move |socket| {
                let server_conf = conf.clone();

                let crt_path = server_conf.https_crt.unwrap();
                let mut crt_file = File::open(
                    Path::new(&crt_path)
                ).expect(&format!("Certificate file \"{}\" not found (or not accessible)", crt_path));
                let mut identity = vec![];
                crt_file.read_to_end(&mut identity).unwrap();

                let mut pass = vec![];
                if server_conf.https_crt_pass_file.is_some() {
                    let crt_pass_file_path = server_conf.https_crt_pass_file.unwrap();
                    let mut crt_pass_file = File::open(
                        Path::new(&crt_pass_file_path)
                    ).expect(&format!("Certificate pass file \"{}\" not found (or not accessible)", crt_pass_file_path));
                    crt_pass_file.read_to_end(&mut pass).unwrap();
                }

                let cert = Identity::from_pkcs12(&identity, &String::from_utf8(pass).unwrap())
                    .expect("Error while opening certificate file (maybe wrong password?)");
                let tls_cx = TlsAcceptor::builder(cert).build().unwrap();
                tls_cx
                    .accept_async(socket)
                    .map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::Other, e)
                    })
            });

        let server = Server::builder(tls_stream).serve(proxy_service)
            .map_err(|e| eprintln!("Server error: {}", e));
        rt::run(server);
    } else {
        let server = Server::builder(listener.incoming()).serve(proxy_service)
            .map_err(|e| eprintln!("Server error: {}", e));
        rt::run(server);
    };
}

fn proxy(conf: Conf, req: Request<Body>) -> BoxFut {
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

    println!(
        "[{:?}] {} {}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        req.method(),
        req.uri()
    );

    Box::new(
        request(req, url)
            .map(move |res| {
                let (parts, body) = res.into_parts();
                Response::from_parts(parts, body)
            })
    )
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
