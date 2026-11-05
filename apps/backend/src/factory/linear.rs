//! Read-only Linear intake. Fixed upstream origin; tokens never reach a runner.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub url: String,
    pub priority: u32,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}
impl Issue {
    pub fn source_key(&self) -> String {
        format!("linear:{}", self.id)
    }
    pub fn task(&self) -> Value {
        json!({"title":self.title,"description":self.description,"source":{"kind":"linear","ref":self.id,"url":self.url},"identifier":self.identifier,"priority":self.priority,"origin_trust":"untrusted"})
    }
}
pub fn prioritize(issues: &mut [Issue]) {
    // Linear: 0 = no priority; 1 = urgent, 2 = high, 3 = normal, 4 = low.
    issues.sort_by(|a, b| {
        let rank = |p: u32| if p == 0 { 5 } else { p };
        (rank(a.priority), &a.created_at, &a.id).cmp(&(rank(b.priority), &b.created_at, &b.id))
    });
}
async fn graphql(
    client: &reqwest::Client,
    token: &str,
    query: &str,
    variables: Value,
) -> Result<Value> {
    let response = client
        .post("https://api.linear.app/graphql")
        .header(reqwest::header::AUTHORIZATION, token)
        .json(&json!({"query":query,"variables":variables}))
        .send()
        .await
        .context("linear_connection_failed")?;
    if !response.status().is_success() {
        bail!("linear_upstream_rejected");
    }
    // Bound the untrusted response before parsing it.
    let mut response = response;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.context("linear_response_failed")? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            bail!("linear_response_too_large");
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).context("linear_invalid_response")?;
    if value.get("errors").is_some() || value.get("data").is_none() {
        bail!("linear_graphql_failed");
    }
    Ok(value["data"].clone())
}

/// Fetch all pages (bounded), then prioritize. Never pick from an incomplete
/// first page: it could hide the highest-priority issue.
pub async fn assigned_issues(
    token: &str,
    team_id: Option<&str>,
    project_id: Option<&str>,
) -> Result<Vec<Issue>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let viewer = graphql(&client, token, "query { viewer { id } }", json!({})).await?;
    let user = viewer["viewer"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("linear_viewer_missing"))?;
    let mut filter =
        json!({"assignee":{"id":{"eq":user}},"state":{"type":{"nin":["completed","canceled"]}}});
    if let Some(team) = team_id {
        filter["team"] = json!({"id":{"eq":team}});
    }
    if let Some(project) = project_id {
        filter["project"] = json!({"id":{"eq":project}});
    }
    let mut after = Value::Null;
    let mut issues = Vec::new();
    for _ in 0..10 {
        let data=graphql(&client,token,
            "query Assigned($filter: IssueFilter!, $after: String) { issues(first: 100, after: $after, filter: $filter) { nodes { id identifier title description url priority createdAt inverseRelations(first: 100) { nodes { type issue { state { type } } } pageInfo { hasNextPage } } } pageInfo { hasNextPage endCursor } } }",
            json!({"filter":filter,"after":after})).await?;
        let mut page = Vec::new();
        for node in data["issues"]["nodes"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("linear_invalid_issues"))?
        {
            if !unblocked(node)? {
                continue;
            }
            let mut node = node.clone();
            node.as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("linear_invalid_issue"))?
                .remove("inverseRelations");
            page.push(serde_json::from_value::<Issue>(node).context("linear_invalid_issues")?);
        }
        if page.iter().any(|i| {
            i.id.is_empty()
                || i.title.is_empty()
                || i.priority > 4
                || i.task().to_string().len() > 65536
        }) {
            bail!("linear_invalid_issue");
        }
        issues.extend(page);
        match data["issues"]["pageInfo"]["hasNextPage"].as_bool() {
            Some(false) => {
                prioritize(&mut issues);
                return Ok(issues);
            }
            Some(true) => {
                let cursor = &data["issues"]["pageInfo"]["endCursor"];
                if !cursor.is_string() || cursor == &after {
                    bail!("linear_invalid_cursor");
                }
                after = cursor.clone();
            }
            None => bail!("linear_invalid_pagination"),
        }
    }
    bail!("linear_issue_limit_exceeded")
}

fn unblocked(node: &Value) -> Result<bool> {
    let relations = &node["inverseRelations"];
    if relations["pageInfo"]["hasNextPage"].as_bool() != Some(false) {
        return Ok(false);
    }
    let nodes = relations["nodes"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("linear_invalid_relations"))?;
    Ok(!nodes.iter().any(|r| {
        r["type"] == "blocks"
            && !matches!(
                r["issue"]["state"]["type"].as_str(),
                Some("completed" | "canceled")
            )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn issue(id: &str, p: u32, date: &str) -> Issue {
        Issue {
            id: id.into(),
            identifier: id.into(),
            title: "Fix".into(),
            description: None,
            url: "https://linear.app/issue".into(),
            priority: p,
            created_at: date.into(),
        }
    }
    #[test]
    fn unresolved_or_unreadable_blockers_are_not_eligible() {
        assert!(!unblocked(&json!({})).unwrap());
        assert!(!unblocked(&json!({"inverseRelations":{"nodes":[{"type":"blocks","issue":{"state":{"type":"started"}}}],"pageInfo":{"hasNextPage":false}}})).unwrap());
        assert!(unblocked(&json!({"inverseRelations":{"nodes":[{"type":"blocks","issue":{"state":{"type":"completed"}}}],"pageInfo":{"hasNextPage":false}}})).unwrap());
    }
    #[test]
    fn urgent_first_unprioritized_last_and_stable_ties() {
        let mut issues = vec![
            issue("z", 0, "2020"),
            issue("b", 1, "2022"),
            issue("a", 1, "2022"),
            issue("c", 2, "2021"),
        ];
        prioritize(&mut issues);
        assert_eq!(
            issues.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c", "z"]
        );
        assert_eq!(issues[0].source_key(), "linear:a");
        assert_eq!(issues[0].task()["origin_trust"], "untrusted");
    }
}
