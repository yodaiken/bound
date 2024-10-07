use std::io;

use thiserror::Error;

use crate::AuthorCodeownerMemberships;

#[derive(Error, Debug)]
pub enum GHCliError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("GitHub API error: {0}")]
    GithubApi(String),
}

pub fn get_token() -> Result<String, GHCliError> {
    let output = std::process::Command::new("gh")
        .arg("auth")
        .arg("token")
        .output()
        .map_err(|e| GHCliError::Io(e))?;

    if output.status.success() {
        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(token)
    } else {
        let error_message = String::from_utf8_lossy(&output.stderr);
        Err(GHCliError::GithubApi(format!(
            "Command `gh auth token` failed: {}",
            error_message
        )))
    }
}

pub struct GithubApi {
    token: String,
    client: reqwest::Client,
}

impl GithubApi {
    fn get_next_page_url(response: &reqwest::Response) -> Option<String> {
        response
            .headers()
            .get(reqwest::header::LINK)
            .and_then(|link| link.to_str().ok())
            .and_then(|link_str| {
                link_str
                    .split(',')
                    .find(|part| part.contains("rel=\"next\""))
                    .and_then(|next_part| {
                        next_part
                            .split(';')
                            .next()
                            .map(|url| url.trim().trim_matches('<').trim_matches('>').to_string())
                    })
            })
    }

    async fn request_ok_json_paginated(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<Vec<serde_json::Value>, GHCliError> {
        let mut all_results = Vec::new();
        let mut current_url = format!("https://api.github.com{}", path);

        loop {
            let response = self
                .client
                .request(method.clone(), &current_url)
                .header("Authorization", format!("token {}", self.token))
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "bound-cli")
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(GHCliError::GithubApi(format!(
                    "GitHub API request failed: {}",
                    response.status()
                )));
            }

            let next_url = Self::get_next_page_url(&response);

            let json: serde_json::Value = response.json().await?;
            if let Some(results) = json.as_array() {
                all_results.extend_from_slice(results);
            } else {
                return Err(GHCliError::GithubApi("Expected array".to_string()));
            }

            if let Some(next_url) = next_url {
                current_url = next_url;
            } else {
                break;
            }
        }

        Ok(all_results)
    }

    pub fn new() -> Result<Self, GHCliError> {
        let token = get_token()?;
        let client = reqwest::Client::new();
        Ok(GithubApi { token, client })
    }

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<reqwest::Response, GHCliError> {
        let url = format!("https://api.github.com{}", path);
        let response = self
            .client
            .request(method, &url)
            .header("Authorization", format!("token {}", self.token))
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "bound-cli")
            .send()
            .await?;

        Ok(response)
    }

    async fn request_ok_json(
        &self,
        method: reqwest::Method,
        path: &str,
    ) -> Result<serde_json::Value, GHCliError> {
        let response = self.request(method, path).await?;
        if !response.status().is_success() {
            return Err(GHCliError::GithubApi(format!(
                "GitHub API request failed: {}",
                response.status()
            )));
        }
        let json = response.json().await?;
        Ok(json)
    }
}

pub async fn get_github_org_logins(api: &GithubApi) -> Result<Vec<String>, GHCliError> {
    let json = api
        .request_ok_json_paginated(reqwest::Method::GET, "/user/orgs")
        .await?;
    let orgs = json
        .into_iter()
        .filter_map(|org| {
            org.as_object()
                .and_then(|org| org.get("login"))
                .and_then(|login| login.as_str())
                .map(|login| login.to_string())
        })
        .collect::<Vec<String>>();
    Ok(orgs)
}

pub async fn get_github_team_slugs(api: &GithubApi, org: &str) -> Result<Vec<String>, GHCliError> {
    let path = format!("/orgs/{}/teams", org);
    let json = api
        .request_ok_json_paginated(reqwest::Method::GET, &path)
        .await?;
    let slugs = json
        .into_iter()
        .filter_map(|team| {
            team.as_object()
                .and_then(|team| team.get("slug"))
                .and_then(|slug| slug.as_str())
                .map(|slug| slug.to_string())
        })
        .collect::<Vec<String>>();
    Ok(slugs)
}

pub async fn get_github_team_members(
    api: &GithubApi,
    org: &str,
    team_slug: &str,
) -> Result<Vec<String>, GHCliError> {
    let path = format!("/orgs/{}/teams/{}/members", org, team_slug);
    let json = api
        .request_ok_json_paginated(reqwest::Method::GET, &path)
        .await?;
    let usernames = json
        .into_iter()
        .filter_map(|member| {
            member
                .as_object()
                .and_then(|member| member.get("login"))
                .and_then(|login| login.as_str())
                .map(|login| login.to_string())
        })
        .collect::<Vec<String>>();
    Ok(usernames)
}

pub async fn get_all_org_members(
    api: &GithubApi,
    org: &str,
) -> Result<Vec<AuthorCodeownerMemberships>, GHCliError> {
    let teams = get_github_team_slugs(api, org).await?;
    let mut all_members = Vec::new();

    for team in teams {
        let members = get_github_team_members(api, org, &team).await?;
        for member in members {
            if let Some((name, email)) = get_user_info(api, &member).await? {
                all_members.push(AuthorCodeownerMemberships {
                    author_email: Some(email),
                    author_name: Some(name),
                    codeowner: format!("@{}/{}", org, team),
                });
            }
        }
    }

    Ok(all_members)
}

pub async fn get_user_info(
    api: &GithubApi,
    login: &str,
) -> Result<Option<(String, String)>, GHCliError> {
    let path = format!("/users/{}", login);
    let json = api.request_ok_json(reqwest::Method::GET, &path).await?;

    if let Some(user) = json.as_object() {
        let name = user
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(login)
            .to_string();
        let email = user
            .get("email")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(Some((name, email)))
    } else {
        Ok(None)
    }
}
