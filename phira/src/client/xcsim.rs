//! Http client for the XC-SIM chart server (csvr).
//! The server protocol is documented in `Document/read.md`, `Document/login.md` and
//! `Document/token.md`. It is a separate account/login system from Phira, so it keeps its own
//! client and access token and is unaffected by the Phira account.

use super::basic_client_builder;
use crate::{get_data, get_data_mut, save_data};
use anyhow::{bail, Context, Result};
use arc_swap::ArcSwap;
use once_cell::sync::Lazy;
use reqwest::{header, Method, RequestBuilder, Response, StatusCode};
use serde::Deserialize;
use std::sync::Arc;

/// XC-SIM API base URL. Overridable via the `xcsimServer` config.
pub const XC_SIM_API_URL: &str = "http://xcapi-dchk.hotanyan.net:20003";

/// XC-SIM file/download host (serves `/files/...`).
pub const XC_SIM_DOWNLOAD_URL: &str = "http://xcapi-dchk.hotanyan.net:20004";

/// The XC-SIM access token (bare token string, without the `Bearer ` prefix).
pub static XC_SIM_CLIENT_TOKEN: Lazy<ArcSwap<Option<String>>> = Lazy::new(|| ArcSwap::from_pointee(None));

static XC_SIM_CLIENT: Lazy<ArcSwap<reqwest::Client>> = Lazy::new(|| ArcSwap::from_pointee(basic_client_builder().build().unwrap()));

/// The base URL the XC-SIM client talks to.
pub fn xc_sim_base() -> String {
    let custom = get_data().config.xcsim_server.trim().trim_end_matches('/').to_owned();
    if custom.is_empty() {
        XC_SIM_API_URL.to_owned()
    } else {
        custom
    }
}

/// The base URL XC-SIM file assets are served from.
pub fn xc_sim_download_base() -> String {
    let api = xc_sim_base();
    // `http://host:20003` -> `http://host:20004`
    if let Some(idx) = api.rfind(':') {
        if api[idx + 1..].parse::<u16>().is_ok() {
            return format!("{}:20004", &api[..idx]);
        }
    }
    api
}

/// The server stamps its own (often unreachable) bind address into asset URLs, e.g.
/// `http://0.0.0.0:20004/files/...`. Re-point every `/files/` asset onto the public
/// download host so it can actually be fetched.
fn rehost_file(url: &str) -> String {
    if let Some((_, name)) = url.split_once("/files/") {
        format!("{}/files/{name}", xc_sim_download_base())
    } else {
        url.to_owned()
    }
}

fn rehost_chart(mut chart: super::Chart) -> super::Chart {
    chart.file.url = rehost_file(&chart.file.url);
    chart.illustration.url = rehost_file(&chart.illustration.url);
    chart.preview.url = rehost_file(&chart.preview.url);
    chart
}

fn xc_sim_build_client(token: Option<&str>) -> Result<Arc<reqwest::Client>> {
    XC_SIM_CLIENT_TOKEN.store(token.map(str::to_owned).into());
    let mut headers = header::HeaderMap::new();
    if let Some(token) = token {
        let mut auth = header::HeaderValue::from_str(&format!("Bearer {token}"))?;
        auth.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, auth);
    }
    Ok(basic_client_builder().default_headers(headers).build()?.into())
}

/// Restore the saved XC-SIM token on startup (no-op when there is none).
pub fn set_token_sync(token: Option<&str>) -> Result<()> {
    XC_SIM_CLIENT.store(xc_sim_build_client(token)?);
    Ok(())
}

/// Whether an XC-SIM account is currently logged in.
pub fn is_logged_in() -> bool {
    XC_SIM_CLIENT_TOKEN.load().as_ref().is_some()
}

/// Clear the XC-SIM login state.
pub fn logout() {
    XC_SIM_CLIENT.store(xc_sim_build_client(None).unwrap());
    let data = get_data_mut();
    data.xcsim_token = None;
    data.xcsim_name = None;
    let _ = save_data();
}

fn request(method: Method, path: impl AsRef<str>) -> RequestBuilder {
    XC_SIM_CLIENT.load().request(method, xc_sim_base() + path.as_ref())
}

async fn recv_raw(request: RequestBuilder) -> Result<Response> {
    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status().as_str().to_owned();
        let text = response.text().await.context("failed to receive text")?;
        if let Ok(what) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(detail) = what["error"].as_str() {
                bail!("request failed ({status}): {detail}");
            }
        }
        bail!("request failed ({status}): {text}");
    }
    Ok(response)
}

/// `POST /register` — create an XC-SIM account (does not log in).
pub async fn register(email: &str, username: &str, password: &str) -> Result<()> {
    recv_raw(
        request(Method::POST, "/register").json(&serde_json::json!({
            "email": email,
            "name": username,
            "password": password,
        })),
    )
    .await?;
    Ok(())
}

/// `POST /login` — password login, stores the returned access/refresh token locally.
pub async fn login(email: &str, password: &str) -> Result<()> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Resp {
        id: i32,
        token: String,
        refresh_token: String,
    }
    let resp: Resp = recv_raw(request(Method::POST, "/login").json(&serde_json::json!({ "email": email, "password": password })))
        .await?
        .json()
        .await?;
    XC_SIM_CLIENT.store(xc_sim_build_client(Some(&resp.token))?);
    // Best-effort: fetch the player name from `/me` so the UI can show it on the login button.
    let name = get_me().await.ok().map(|u| u.name);
    let data = get_data_mut();
    data.xcsim_token = Some((resp.token, resp.refresh_token));
    data.xcsim_name = name;
    save_data()?;
    Ok(())
}

/// `GET /me` — the current XC-SIM account (requires login).
pub async fn get_me() -> Result<super::User> {
    Ok(recv_raw(request(Method::GET, "/me")).await?.json().await?)
}

/// `GET /chart` — fetch a page of XC-SIM charts. Mirrors Phira's `/chart` query shape
/// (`{ count, results }`).
pub async fn query_charts(search: &str, order: &str, division: &str, page: u64) -> Result<(Vec<super::Chart>, u64)> {
    #[derive(Deserialize)]
    struct PagedResult<T> {
        count: u64,
        results: Vec<T>,
    }
    let mut queries: Vec<(&str, String)> = vec![("page", (page + 1).to_string())];
    if !search.is_empty() {
        queries.push(("search", search.to_owned()));
    }
    if !order.is_empty() {
        queries.push(("order", order.to_owned()));
    }
    if !division.is_empty() {
        queries.push(("division", division.to_owned()));
    }
    let res: PagedResult<super::Chart> = recv_raw(request(Method::GET, "/chart").query(&queries)).await?.json().await?;
    let results = res.results.into_iter().map(rehost_chart).collect();
    Ok((results, res.count))
}

/// `GET /chart/{id}` — fetch a single XC-SIM chart (for the song scene).
pub async fn get_chart(id: i32) -> Result<Option<Arc<super::Chart>>> {
    let resp = request(Method::GET, format!("/chart/{id}")).send().await?;
    if resp.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let status = resp.status().as_str().to_owned();
        let text = resp.text().await.context("failed to receive text")?;
        if let Ok(what) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(detail) = what["error"].as_str() {
                bail!("request failed ({status}): {detail}");
            }
        }
        bail!("request failed ({status}): {text}");
    }
    let chart: super::Chart = resp.json().await?;
    Ok(Some(Arc::new(rehost_chart(chart))))
}

/// `GET /user/{id}` — fetch an XC-SIM user (public, no login required).
pub async fn get_user(id: i32) -> Result<Option<super::User>> {
    let resp = request(Method::GET, format!("/user/{id}")).send().await?;
    if resp.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        let status = resp.status().as_str().to_owned();
        let text = resp.text().await.context("failed to receive text")?;
        if let Ok(what) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(detail) = what["error"].as_str() {
                bail!("request failed ({status}): {detail}");
            }
        }
        bail!("request failed ({status}): {text}");
    }
    let mut user: super::User = resp.json().await?;
    // 头像 URL 同样可能是服务器内网地址，需重指向公开下载主机。
    if let Some(avatar) = &mut user.avatar {
        avatar.url = rehost_file(&avatar.url);
    }
    Ok(Some(user))
}
