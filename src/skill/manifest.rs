use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;

// ── Top-level file structs ────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SkillFile {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub tama: TamaSkillMeta,
}

#[derive(Debug)]
pub struct AgentFile {
    pub name: String,
    pub description: String,
    pub version: String,
    pub pattern: AgentPattern,
    pub env: Option<Vec<String>>,
    pub call: Option<CallConfig>,
    pub max_iter: Option<u32>,
    pub body: String,
}

/// Parsed step file (draft.md, critique.md, reflect.md, etc.).
/// Frontmatter is optional — if absent, defaults to oneshot with no tools.
/// Set `pattern: react` in frontmatter to run as a react loop instead.
pub struct StepConfig {
    pub react: bool,
    pub call: Option<CallConfig>,
    pub body: String,
}

impl StepConfig {
    pub fn uses(&self) -> &[String] {
        self.call.as_ref().map(|c| c.uses.as_slice()).unwrap_or(&[])
    }
    pub fn max_iter(&self) -> u32 {
        self.call.as_ref().and_then(|c| c.max_iter).unwrap_or(10)
    }
}

// ── tama: block for SKILL.md ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TamaSkillMeta {
    pub version: String,
    pub pattern: SkillPattern,
    pub depends: Option<Depends>,
    pub env: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SkillPattern {
    Tool { tool: String },
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Depends {
    #[serde(default)]
    pub uv: Vec<String>,
    #[serde(default)]
    pub apt: Vec<String>,
    #[serde(default)]
    pub bins: Vec<String>,
}

// ── call: block ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default, Clone)]
pub struct CallConfig {
    pub model: Option<ModelConfig>,
    #[serde(default)]
    pub uses: Vec<String>,
    pub max_iter: Option<u32>,
}

// ── Agent pattern ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "pattern", rename_all = "kebab-case")]
pub enum AgentPattern {
    /// Run a fixed list of agents in parallel and collect results.
    Parallel { workers: Vec<String> },
    /// Native tool use API loop until `finish` or max_iter.
    React,
    /// React with a built-in `parallel_run` tool; worker declared statically.
    Scatter { worker: String },
    /// Finite state machine: states + transitions routing by `finish` word.
    Fsm {
        initial: String,
        states: HashMap<String, Option<FsmNext>>,
    },
    /// draft → critique → refine.
    Critic,
    /// act → evaluate → reflect → loop.
    /// max_iter comes from AgentFile.max_iter (defaults to 4 if not set).
    Reflexion,
    /// generate → check against principles → revise.
    Constitutional,
    /// generate → verify claims → revise.
    ChainOfVerification,
    /// plan → execute → verify → loop.
    PlanExecute,
    /// position-a → position-b → judge synthesizes.
    Debate {
        agents: Vec<String>,
        rounds: u32,
        judge: String,
    },
    /// Scatter N variants → judge picks best.
    BestOfN { n: u32 },
    /// Two-phase react with human-in-the-loop pause between phases.
    Human,
    /// Single LLM call: system prompt from AGENT.md body, input as user message.
    Oneshot,
}

/// FSM transition value for a single state.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FsmNext {
    /// Unconditional: always go to this agent regardless of finish word.
    Unconditional(String),
    /// Conditional: list of `{word: agent}` maps; first match wins.
    /// Use `"*"` as a catch-all default.
    /// Target can be `~` (null) to stop without transitioning to another agent.
    Conditional(Vec<HashMap<String, Option<String>>>),
}

// ── Model config ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct ModelConfig {
    /// Role-based selection: reads TAMA_MODEL_{ROLE} from env.
    /// e.g. role: thinker → reads TAMA_MODEL_THINKER
    /// Hyphens in role names are converted to underscores for env lookup.
    pub role: Option<String>,

    /// Direct override: provider:model-name
    /// e.g. name: anthropic:claude-opus-4-6
    pub name: Option<String>,

    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

impl ModelConfig {
    /// Resolve to a concrete ModelRef.
    /// `name` takes priority over `role`.
    pub fn resolve(&self) -> Result<ModelRef> {
        if let Some(name) = &self.name {
            return ModelRef::parse(name);
        }
        if let Some(role) = &self.role {
            // "my-fast" → "TAMA_MODEL_MY_FAST"
            let env_key = format!("TAMA_MODEL_{}", role.to_uppercase().replace('-', "_"));
            let val = std::env::var(&env_key)
                .with_context(|| format!("env var {} is not set", env_key))?;
            return ModelRef::parse(&val);
        }
        bail!("model config requires either `role:` or `name:`")
    }
}

// ── ModelRef ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub provider: Provider,
    pub model: String,
}

impl ModelRef {
    /// Parse "provider:model-name" format.
    pub fn parse(s: &str) -> Result<Self> {
        let (provider_str, model) = s
            .split_once(':')
            .with_context(|| format!("invalid model format '{}': expected 'provider:model'", s))?;

        let provider =
            Provider::parse(provider_str).with_context(|| format!("in model spec '{}'", s))?;

        if model.is_empty() {
            bail!("model name cannot be empty in '{}'", s);
        }

        Ok(ModelRef {
            provider,
            model: model.to_string(),
        })
    }
}

impl fmt::Display for ModelRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.model)
    }
}

// ── Provider ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    OpenAi,
    Google,
    Ollama,
}

impl Provider {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "anthropic" => Ok(Provider::Anthropic),
            "openai" => Ok(Provider::OpenAi),
            "google" => Ok(Provider::Google),
            "ollama" => Ok(Provider::Ollama),
            other => bail!(
                "unknown provider '{}': supported providers: anthropic, openai, google, ollama",
                other
            ),
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Provider::Anthropic => write!(f, "anthropic"),
            Provider::OpenAi => write!(f, "openai"),
            Provider::Google => write!(f, "google"),
            Provider::Ollama => write!(f, "ollama"),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_anthropic_model() {
        let m = ModelRef::parse("anthropic:claude-opus-4-6").unwrap();
        assert_eq!(m.provider, Provider::Anthropic);
        assert_eq!(m.model, "claude-opus-4-6");
    }

    #[test]
    fn parse_openai_model() {
        let m = ModelRef::parse("openai:gpt-4o").unwrap();
        assert_eq!(m.provider, Provider::OpenAi);
        assert_eq!(m.model, "gpt-4o");
    }

    #[test]
    fn parse_google_model() {
        let m = ModelRef::parse("google:gemini-2.0-flash").unwrap();
        assert_eq!(m.provider, Provider::Google);
        assert_eq!(m.model, "gemini-2.0-flash");
    }

    #[test]
    fn parse_missing_colon() {
        assert!(ModelRef::parse("claude-opus-4-6").is_err());
    }

    #[test]
    fn parse_unknown_provider() {
        assert!(ModelRef::parse("mistral:mixtral-8x7b").is_err());
    }

    #[test]
    fn display_roundtrip() {
        let s = "anthropic:claude-sonnet-4-6";
        assert_eq!(ModelRef::parse(s).unwrap().to_string(), s);
    }

    #[test]
    fn resolve_via_role_env() {
        std::env::set_var("TAMA_MODEL_THINKER", "anthropic:claude-opus-4-6");
        let cfg = ModelConfig {
            role: Some("thinker".into()),
            name: None,
            max_tokens: None,
            temperature: None,
        };
        let m = cfg.resolve().unwrap();
        assert_eq!(m.provider, Provider::Anthropic);
        assert_eq!(m.model, "claude-opus-4-6");
    }

    #[test]
    fn resolve_hyphen_role_maps_to_underscore_env() {
        std::env::set_var("TAMA_MODEL_MY_FAST", "openai:gpt-4o-mini");
        let cfg = ModelConfig {
            role: Some("my-fast".into()),
            name: None,
            max_tokens: None,
            temperature: None,
        };
        let m = cfg.resolve().unwrap();
        assert_eq!(m.provider, Provider::OpenAi);
        assert_eq!(m.model, "gpt-4o-mini");
    }

    #[test]
    fn resolve_name_overrides_role() {
        std::env::set_var("TAMA_MODEL_WORKER", "anthropic:claude-sonnet-4-6");
        let cfg = ModelConfig {
            role: Some("worker".into()),
            name: Some("google:gemini-2.0-flash".into()),
            max_tokens: None,
            temperature: None,
        };
        let m = cfg.resolve().unwrap();
        assert_eq!(m.provider, Provider::Google); // name wins
    }

    #[test]
    fn fsm_unconditional_transition() {
        let yaml = r#"
pattern: fsm
initial: draft
states:
  draft: critique
  critique:
"#;
        // Deserialize directly using flat format (no tama: wrapper)
        #[derive(Deserialize)]
        struct FlatMeta {
            version: String,
            #[serde(flatten)]
            pattern: AgentPattern,
        }
        let meta: FlatMeta = serde_yaml::from_str(&format!("version: \"1.0.0\"\n{yaml}")).unwrap();
        if let AgentPattern::Fsm { initial, states } = meta.pattern {
            assert_eq!(initial, "draft");
            assert!(matches!(states["draft"], Some(FsmNext::Unconditional(_))));
            assert!(states["critique"].is_none());
        } else {
            panic!("expected Fsm pattern");
        }
    }

    #[test]
    fn fsm_conditional_transition() {
        let yaml = r#"
version: "1.0.0"
pattern: fsm
initial: draft
states:
  draft:
    - good-enough: done
    - needs-work: critique
  critique: refine
  refine:
  done:
"#;
        #[derive(Deserialize)]
        struct FlatMeta {
            version: String,
            #[serde(flatten)]
            pattern: AgentPattern,
        }
        let meta: FlatMeta = serde_yaml::from_str(yaml).unwrap();
        if let AgentPattern::Fsm { states, .. } = meta.pattern {
            assert!(matches!(states["draft"], Some(FsmNext::Conditional(_))));
            assert!(matches!(
                states["critique"],
                Some(FsmNext::Unconditional(_))
            ));
            assert!(states["refine"].is_none());
        } else {
            panic!("expected Fsm pattern");
        }
    }
}
