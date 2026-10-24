use hbb_common::{
    async_recursion::async_recursion,
    bail,
    config::{Config, Socks5Server},
    log::{self, info},
    proxy::{Proxy, ProxyScheme},
    tls::{
        get_cached_tls_accept_invalid_cert, get_cached_tls_type, is_plain, upsert_tls_cache,
        TlsType,
    },
    ResultType,
};
use reqwest::{blocking::Client as SyncClient, Client as AsyncClient};

macro_rules! configure_http_client {
    ($builder:expr, $tls_type:expr, $danger_accept_invalid_cert:expr, $Client:ty) => {{
        (|| -> ResultType<$Client> {
            let mut builder = $builder.no_proxy();
            match $tls_type {
                TlsType::Plain => {},
                TlsType::NativeTls => {
                    builder = builder.use_native_tls().danger_accept_invalid_certs($danger_accept_invalid_cert);
                },
                TlsType::Rustls => {
                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    { builder = builder.use_preconfigured_tls(hbb_common::verifier::client_config($danger_accept_invalid_cert)?); }
                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                    { builder = builder.use_rustls_tls().danger_accept_invalid_certs($danger_accept_invalid_cert); }
                },
            }
            if let Some(conf) = Config::get_socks() {
                let proxy = Proxy::from_conf(&conf, None)?;
                let address = match &proxy.intercept {
                    ProxyScheme::Http { host, .. } => format!("http://{}", host),
                    ProxyScheme::Https { host, .. } => format!("https://{}", host),
                    ProxyScheme::Socks5 { addr, .. } => format!("socks5://{}", addr),
                };
                let mut configured = reqwest::Proxy::all(address)?;
                if let Some(auth) = proxy.intercept.maybe_auth() {
                    if !auth.username().is_empty() && !auth.password().is_empty() {
                        configured = configured.basic_auth(auth.username(), auth.password());
                    }
                }
                builder = builder.proxy(configured);
            }
            Ok(builder.build()?)
        })()
    }};
}

/// Audit delivery must not bypass a configured proxy, redirect, or disable certificate checks.
pub fn create_audit_http_client(local_http: bool) -> ResultType<AsyncClient> {
    let builder = AsyncClient::builder().no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .https_only(!local_http)
        .timeout(std::time::Duration::from_secs(15));
    if local_http { return Ok(builder.build()?); }
    configure_http_client!(builder, TlsType::Rustls, false, AsyncClient)
}

pub fn create_http_client(tls_type: TlsType, danger_accept_invalid_cert: bool) -> SyncClient {
    let builder = SyncClient::builder();
    configure_http_client!(builder, tls_type, danger_accept_invalid_cert, SyncClient).unwrap_or_else(|e| {
        info!("Failed to configure HTTP client: {}", e);
        SyncClient::new()
    })
}

pub fn create_http_client_async(
    tls_type: TlsType,
    danger_accept_invalid_cert: bool,
) -> AsyncClient {
    let builder = AsyncClient::builder();
    configure_http_client!(builder, tls_type, danger_accept_invalid_cert, AsyncClient).unwrap_or_else(|e| {
        info!("Failed to configure HTTP client: {}", e);
        AsyncClient::new()
    })
}

pub fn get_url_for_tls<'a>(url: &'a str, proxy_conf: &'a Option<Socks5Server>) -> &'a str {
    if is_plain(url) {
        if let Some(conf) = proxy_conf {
            if conf.proxy.starts_with("https://") {
                return &conf.proxy;
            }
        }
    }
    url
}

pub fn create_http_client_with_url(url: &str) -> SyncClient {
    let proxy_conf = Config::get_socks();
    let tls_url = get_url_for_tls(url, &proxy_conf);
    let tls_type = get_cached_tls_type(tls_url);
    let is_tls_type_cached = tls_type.is_some();
    let tls_type = tls_type.unwrap_or(TlsType::Rustls);
    let tls_danger_accept_invalid_cert = get_cached_tls_accept_invalid_cert(tls_url);
    create_http_client_with_url_(
        url,
        tls_url,
        tls_type,
        is_tls_type_cached,
        tls_danger_accept_invalid_cert,
        tls_danger_accept_invalid_cert,
    )
}

pub fn create_http_client_with_url_strict(url: &str) -> ResultType<SyncClient> {
    let parsed_url = url::Url::parse(url)?;
    if parsed_url.scheme() != "https" {
        bail!("Strict HTTP client requires HTTPS: {}", url);
    }
    let proxy_conf = Config::get_socks();
    let tls_url = get_url_for_tls(url, &proxy_conf);
    let cached_tls_type = get_cached_tls_type(tls_url);
    let cached_danger_accept_invalid_cert = get_cached_tls_accept_invalid_cert(tls_url);
    let can_reuse_cached_probe =
        cached_tls_type.is_some() && cached_danger_accept_invalid_cert == Some(false);
    let tls_type = if can_reuse_cached_probe {
        cached_tls_type.unwrap_or(TlsType::Rustls)
    } else {
        TlsType::Rustls
    };
    Ok(create_http_client_with_url_(
        url,
        tls_url,
        tls_type,
        can_reuse_cached_probe,
        Some(false),
        Some(false),
    ))
}

fn create_http_client_with_url_(
    url: &str,
    tls_url: &str,
    tls_type: TlsType,
    is_tls_type_cached: bool,
    danger_accept_invalid_cert: Option<bool>,
    original_danger_accept_invalid_cert: Option<bool>,
) -> SyncClient {
    let mut client = create_http_client(tls_type, danger_accept_invalid_cert.unwrap_or(false));
    if is_tls_type_cached && original_danger_accept_invalid_cert.is_some() {
        return client;
    }
    if let Err(e) = client.head(url).send() {
        if e.is_request() {
            match (tls_type, is_tls_type_cached, danger_accept_invalid_cert) {
                (TlsType::Rustls, _, None) => {
                    log::warn!(
                        "Failed to connect to server {} with rustls-tls: {:?}, trying accept invalid cert",
                        tls_url,
                        e
                    );
                    client = create_http_client_with_url_(
                        url,
                        tls_url,
                        tls_type,
                        is_tls_type_cached,
                        Some(true),
                        original_danger_accept_invalid_cert,
                    );
                }
                (TlsType::Rustls, false, Some(_)) => {
                    log::warn!(
                        "Failed to connect to server {} with rustls-tls: {:?}, trying native-tls",
                        tls_url,
                        e
                    );
                    client = create_http_client_with_url_(
                        url,
                        tls_url,
                        TlsType::NativeTls,
                        is_tls_type_cached,
                        original_danger_accept_invalid_cert,
                        original_danger_accept_invalid_cert,
                    );
                }
                (TlsType::NativeTls, _, None) => {
                    log::warn!(
                        "Failed to connect to server {} with native-tls: {:?}, trying accept invalid cert",
                        tls_url,
                        e
                    );
                    client = create_http_client_with_url_(
                        url,
                        tls_url,
                        tls_type,
                        is_tls_type_cached,
                        Some(true),
                        original_danger_accept_invalid_cert,
                    );
                }
                _ => {
                    log::error!(
                        "Failed to connect to server {} with {:?}, err: {:?}.",
                        tls_url,
                        tls_type,
                        e
                    );
                }
            }
        } else {
            log::warn!(
                "Failed to connect to server {} with {:?}, err: {}.",
                tls_url,
                tls_type,
                e
            );
        }
    } else {
        log::info!(
            "Successfully connected to server {} with {:?}",
            tls_url,
            tls_type
        );
        upsert_tls_cache(
            tls_url,
            tls_type,
            danger_accept_invalid_cert.unwrap_or(false),
        );
    }
    client
}

pub async fn create_http_client_async_with_url(url: &str) -> AsyncClient {
    let proxy_conf = Config::get_socks();
    let tls_url = get_url_for_tls(url, &proxy_conf);
    let tls_type = get_cached_tls_type(tls_url);
    let is_tls_type_cached = tls_type.is_some();
    let tls_type = tls_type.unwrap_or(TlsType::Rustls);
    let danger_accept_invalid_cert = get_cached_tls_accept_invalid_cert(tls_url);
    create_http_client_async_with_url_(
        url,
        tls_url,
        tls_type,
        is_tls_type_cached,
        danger_accept_invalid_cert,
        danger_accept_invalid_cert,
    )
    .await
}

pub async fn create_http_client_async_with_url_strict(url: &str) -> ResultType<AsyncClient> {
    let parsed_url = url::Url::parse(url)?;
    if parsed_url.scheme() != "https" {
        bail!("Strict HTTP client requires HTTPS: {}", url);
    }
    let proxy_conf = Config::get_socks();
    let tls_url = get_url_for_tls(url, &proxy_conf);
    let cached_tls_type = get_cached_tls_type(tls_url);
    let cached_danger_accept_invalid_cert = get_cached_tls_accept_invalid_cert(tls_url);
    let can_reuse_cached_probe =
        cached_tls_type.is_some() && cached_danger_accept_invalid_cert == Some(false);
    let tls_type = if can_reuse_cached_probe {
        cached_tls_type.unwrap_or(TlsType::Rustls)
    } else {
        TlsType::Rustls
    };
    Ok(create_http_client_async_with_url_(
        url,
        tls_url,
        tls_type,
        can_reuse_cached_probe,
        Some(false),
        Some(false),
    )
    .await)
}

#[async_recursion]
async fn create_http_client_async_with_url_(
    url: &str,
    tls_url: &str,
    tls_type: TlsType,
    is_tls_type_cached: bool,
    danger_accept_invalid_cert: Option<bool>,
    original_danger_accept_invalid_cert: Option<bool>,
) -> AsyncClient {
    let mut client =
        create_http_client_async(tls_type, danger_accept_invalid_cert.unwrap_or(false));
    if is_tls_type_cached && original_danger_accept_invalid_cert.is_some() {
        return client;
    }
    if let Err(e) = client.head(url).send().await {
        match (tls_type, is_tls_type_cached, danger_accept_invalid_cert) {
            (TlsType::Rustls, _, None) => {
                log::warn!(
                    "Failed to connect to server {} with rustls-tls: {:?}, trying accept invalid cert",
                    tls_url,
                    e
                );
                client = create_http_client_async_with_url_(
                    url,
                    tls_url,
                    tls_type,
                    is_tls_type_cached,
                    Some(true),
                    original_danger_accept_invalid_cert,
                )
                .await;
            }
            (TlsType::Rustls, false, Some(_)) => {
                log::warn!(
                    "Failed to connect to server {} with rustls-tls: {:?}, trying native-tls",
                    tls_url,
                    e
                );
                client = create_http_client_async_with_url_(
                    url,
                    tls_url,
                    TlsType::NativeTls,
                    is_tls_type_cached,
                    original_danger_accept_invalid_cert,
                    original_danger_accept_invalid_cert,
                )
                .await;
            }
            (TlsType::NativeTls, _, None) => {
                log::warn!(
                    "Failed to connect to server {} with native-tls: {:?}, trying accept invalid cert",
                    tls_url,
                    e
                );
                client = create_http_client_async_with_url_(
                    url,
                    tls_url,
                    tls_type,
                    is_tls_type_cached,
                    Some(true),
                    original_danger_accept_invalid_cert,
                )
                .await;
            }
            _ => {
                log::error!(
                    "Failed to connect to server {} with {:?}, err: {:?}.",
                    tls_url,
                    tls_type,
                    e
                );
            }
        }
    } else {
        log::info!(
            "Successfully connected to server {} with {:?}",
            tls_url,
            tls_type
        );
        upsert_tls_cache(
            tls_url,
            tls_type,
            danger_accept_invalid_cert.unwrap_or(false),
        );
    }
    client
}
