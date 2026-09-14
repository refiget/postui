use reqwest::Method;

pub const STANDARD_METHODS: &[Method] = &[
    Method::GET,
    Method::POST,
    Method::PUT,
    Method::PATCH,
    Method::DELETE,
    Method::HEAD,
    Method::OPTIONS,
    Method::TRACE,
    Method::CONNECT,
];

pub fn parse(value: &str) -> anyhow::Result<Method> {
    Ok(Method::from_bytes(
        value.trim().to_ascii_uppercase().as_bytes(),
    )?)
}
