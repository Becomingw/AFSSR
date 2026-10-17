use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SUITE: &str = "offline-protocol-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Client {
    Codex,
    ClaudeCode,
    OpenCode,
    Generic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[serde(rename = "openai:responses")]
    Responses,
    #[serde(rename = "openai:chat")]
    Chat,
    #[serde(rename = "claude:messages")]
    Claude,
}
impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "openai:responses",
            Self::Chat => "openai:chat",
            Self::Claude => "claude:messages",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileConfig {
    pub client: Client,
    pub client_version: String,
    pub client_protocol: Protocol,
    pub model: String,
    pub upstream_format: Protocol,
    pub mapped_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<ChannelTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelTarget {
    pub provider_id: uuid::Uuid,
    pub provider_model_id: uuid::Uuid,
    pub endpoint_id: uuid::Uuid,
}
impl ProfileConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        fn label(v: &str, max: usize) -> bool {
            !v.is_empty()
                && v.len() <= max
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:/+-".contains(&b))
                && !v.to_ascii_lowercase().starts_with("sk-")
                && !v.to_ascii_lowercase().contains("bearer")
        }
        if !label(&self.client_version, 40)
            || !label(&self.model, 128)
            || !label(&self.mapped_model, 128)
        {
            return Err("Use bounded identifier labels only; no credentials, URLs or request text");
        }
        if self.model.contains("://")
            || self.mapped_model.contains("://")
            || self.client_version.contains("://")
        {
            return Err("URLs are not accepted");
        }
        if (self.client == Client::Codex && self.client_protocol != Protocol::Responses)
            || (self.client == Client::ClaudeCode && self.client_protocol != Protocol::Claude)
        {
            return Err("Codex requires Responses; Claude Code requires Messages in this suite");
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(SUITE);
        hash.update(env!("AFSSR_COMPAT_CONVERTER_DIGEST"));
        hash.update(serde_json::to_vec(self).expect("fixed profile serialization"));
        format!("{:x}", hash.finalize())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Passed,
    Failed,
    NotTested,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseResult {
    pub case: String,
    pub status: Status,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: uuid::Uuid,
    pub config: ProfileConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub id: uuid::Uuid,
    pub profile_id: uuid::Uuid,
    pub config: ProfileConfig,
    pub config_fingerprint: String,
    pub suite: String,
    pub evidence: Evidence,
    pub created_at: i64,
    pub cases: Vec<CaseResult>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    OfflineSynthetic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub revision: u64,
    pub profiles: Vec<Profile>,
    pub runs: Vec<Run>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            profiles: vec![],
            runs: vec![],
        }
    }
}
impl Snapshot {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 || self.profiles.len() > 64 || self.runs.len() > 200 {
            return Err("Unsupported or oversized compatibility snapshot");
        }
        let mut ids = std::collections::BTreeSet::new();
        for profile in &self.profiles {
            profile.config.validate()?;
            if !ids.insert(profile.id) {
                return Err("Duplicate profile");
            }
        }
        for run in &self.runs {
            run.config.validate()?;
            if !ids.contains(&run.profile_id)
                || run.cases.len() != 13
                || run.config_fingerprint.len() != 64
                || !run
                    .config_fingerprint
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit())
                || run.suite != SUITE
                || run.cases.iter().any(|c| !super::runner::valid_case(c))
            {
                return Err("Invalid compatibility result");
            }
            let unique: std::collections::BTreeSet<_> = run.cases.iter().map(|c| &c.case).collect();
            if unique.len() != 13 {
                return Err("Duplicate compatibility case");
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveProfile {
    pub expected_revision: u64,
    pub profile: Profile,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunInput {
    pub expected_revision: u64,
    pub profile_id: uuid::Uuid,
    pub expected_fingerprint: String,
}
