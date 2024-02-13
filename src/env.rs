pub struct Env {
    pub url: String,
    pub username: String,
    pub password: String,
    pub skip_tls_cert_verify: bool,
}

pub fn as_bool(v: &str) -> Option<bool> {
    match v.to_lowercase().trim() {
        "1" | "true" | "t" | "yes" | "y" => Some(true),
        "0" | "false" | "f" | "no" | "n" => Some(false),
        _ => None,
    }
}

pub fn load_env() -> Result<Env, &'static str> {
    let url = std::env::var("ELASTIC_URL").map_err(|_| "ELASTIC_URL undefined")?;
    let username = std::env::var("ELASTIC_USERNAME").map_err(|_| "ELASTIC_USERNAME undefined")?;
    let password = std::env::var("ELASTIC_PASSWORD").map_err(|_| "ELASTIC_PASSWORD undefined")?;
    let skip_tls_cert_verify =
        match as_bool(&std::env::var("ELASTIC_SKIP_VERIFY undefined").unwrap_or("false".into())) {
            Some(v) => Ok(v),
            None => Err("ELASTIC_SKIP_VERIFY must be undefined, true or false."),
        }?;

    Ok(Env {
        url,
        username,
        password,
        skip_tls_cert_verify,
    })
}
