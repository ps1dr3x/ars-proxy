use std::env;

#[derive(Debug, Clone)]
pub struct Conf {
    pub local_port: u16,
    pub remote_url: String,
    pub remote_port: u16,
    pub https_crt: Option<String>,
    pub https_crt_pass_file: Option<String>,
    pub to_https: bool,
}

pub fn get_cli_params() -> Result<Conf, String> {
    let mut args = env::args();

    if args.len() < 4 {
        return Err("Missing mandatory parameters".to_string());
    }

    let mut conf = Conf {
        local_port: {
            let arg = args.nth(1).unwrap().parse();
            if arg.is_err() {
                return Err("Error while parsing local_port argument".to_string());
            }
            arg.unwrap()
        },
        remote_url: args.next().unwrap(),
        remote_port: {
            let arg = args.next().unwrap().parse();
            if arg.is_err() {
                return Err("Error while parsing remote_port argument".to_string());
            }
            arg.unwrap()
        },
        https_crt: None,
        https_crt_pass_file: None,
        to_https: false,
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cert" => {
                let arg = args.next();
                if arg.is_none() {
                    return Err("Missing value for parameter --cert".to_string());
                } else {
                    conf.https_crt = arg;
                }
            },
            "--pass-file" => {
                let arg = args.next();
                if arg.is_none() {
                    return Err("Missing value for parameter --pass-file".to_string());
                } else {
                    conf.https_crt_pass_file = arg;
                }
            }
            "--to-https" => {
                conf.to_https = true;
            }
            _ => {
                return Err(format!("\nInvalid parameter: {}", arg));
            }
        }
    }

    if conf.https_crt.is_some() && conf.to_https {
        println!("\nWarning: --to-https parameter is useless when the server is listening on https");
    }

    Ok(conf)
}
