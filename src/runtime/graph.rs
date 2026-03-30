use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::skill::manifest::{AgentFile, AgentPattern};
use crate::skill::parser::parse_agent;

pub struct AgentNode {
    pub name: String,
    pub dir: PathBuf,
    pub agent: AgentFile,
}

pub struct AgentGraph {
    pub nodes: HashMap<String, AgentNode>,
    pub root: String,
}

impl AgentGraph {
    /// Parse all agents reachable from `root_name`, building a static graph.
    pub fn build(root_name: &str) -> Result<Self> {
        let mut graph = AgentGraph {
            nodes: HashMap::new(),
            root: root_name.to_string(),
        };
        graph.load_node(root_name)?;
        Ok(graph)
    }

    fn load_node(&mut self, name: &str) -> Result<()> {
        if self.nodes.contains_key(name) {
            return Ok(());
        }
        let dir = find_agent_dir(name)?;
        let agent = parse_agent(&dir.join("AGENT.md"))
            .with_context(|| format!("failed to load agent '{name}'"))?;

        // Collect refs before inserting (borrow checker)
        let refs = agent_refs(&agent.pattern);

        // Validate step files before any LLM work starts (reuse lint logic).
        crate::skill::lint::lint_agent(&dir)
            .with_context(|| format!("agent '{name}' failed validation"))?;

        self.nodes.insert(
            name.to_string(),
            AgentNode {
                name: name.to_string(),
                dir,
                agent,
            },
        );

        for ref_name in refs {
            self.load_node(&ref_name)?;
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&AgentNode> {
        self.nodes.get(name)
    }

    pub fn root_node(&self) -> &AgentNode {
        &self.nodes[&self.root]
    }
}

/// Collect agent names directly referenced by a pattern.
fn agent_refs(pattern: &AgentPattern) -> Vec<String> {
    match pattern {
        AgentPattern::Parallel { workers } => workers.clone(),
        AgentPattern::Scatter { worker } => vec![worker.clone()],
        // Every non-terminal state in the map IS an agent to load.
        // Terminal states (None) are routing markers with no agent file.
        AgentPattern::Fsm { states, .. } => states
            .iter()
            .filter(|&(_name, next)| next.is_some())
            .map(|(name, _next)| name.clone())
            .collect(),
        AgentPattern::Debate { agents, judge, .. } => {
            let mut refs = agents.clone();
            refs.push(judge.clone());
            refs
        }
        _ => vec![],
    }
}

pub fn find_agent_dir(name: &str) -> Result<PathBuf> {
    let direct = PathBuf::from("agents").join(name);
    if direct.join("AGENT.md").exists() {
        return Ok(direct);
    }
    anyhow::bail!("agent '{name}' not found (checked agents/{name}/)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::manifest::*;
    use std::collections::HashMap;

    fn react() -> AgentPattern {
        AgentPattern::React
    }
    fn critic() -> AgentPattern {
        AgentPattern::Critic
    }
    fn constitutional() -> AgentPattern {
        AgentPattern::Constitutional
    }

    #[test]
    fn refs_react_empty() {
        assert!(agent_refs(&react()).is_empty());
    }

    #[test]
    fn refs_critic_empty() {
        assert!(agent_refs(&critic()).is_empty());
    }

    #[test]
    fn refs_constitutional_empty() {
        assert!(agent_refs(&constitutional()).is_empty());
    }

    #[test]
    fn refs_scatter_returns_worker() {
        let p = AgentPattern::Scatter {
            worker: "my-worker".to_string(),
        };
        assert_eq!(agent_refs(&p), vec!["my-worker"]);
    }

    #[test]
    fn refs_parallel_returns_all_workers() {
        let p = AgentPattern::Parallel {
            workers: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        };
        let refs = agent_refs(&p);
        assert_eq!(refs, vec!["a", "b", "c"]);
    }

    #[test]
    fn refs_debate_returns_agents_and_judge() {
        let p = AgentPattern::Debate {
            agents: vec!["pro".to_string(), "con".to_string()],
            rounds: 2,
            judge: "judge".to_string(),
        };
        let refs = agent_refs(&p);
        assert!(refs.contains(&"pro".to_string()));
        assert!(refs.contains(&"con".to_string()));
        assert!(refs.contains(&"judge".to_string()));
        assert_eq!(refs.len(), 3);
    }

    #[test]
    fn refs_fsm_unconditional_transitions() {
        let mut states = HashMap::new();
        states.insert(
            "draft".to_string(),
            Some(FsmNext::Unconditional("critique".to_string())),
        );
        states.insert(
            "critique".to_string(),
            Some(FsmNext::Unconditional("refine".to_string())),
        );
        states.insert("refine".to_string(), None); // terminal
        let p = AgentPattern::Fsm {
            initial: "draft".to_string(),
            states,
        };
        let refs = agent_refs(&p);
        // Non-terminal state keys are agents.
        assert!(refs.contains(&"draft".to_string()));
        assert!(refs.contains(&"critique".to_string()));
        assert!(
            !refs.contains(&"refine".to_string()),
            "terminal must not be in refs"
        );
    }

    #[test]
    fn refs_fsm_conditional_transitions() {
        let mut states = HashMap::new();
        states.insert(
            "draft".to_string(),
            Some(FsmNext::Conditional(vec![
                HashMap::from([("good-enough".to_string(), Some("done".to_string()))]),
                HashMap::from([("needs-work".to_string(), Some("critique".to_string()))]),
            ])),
        );
        states.insert("done".to_string(), None); // terminal
        states.insert("critique".to_string(), None); // terminal
        let p = AgentPattern::Fsm {
            initial: "draft".to_string(),
            states,
        };
        let refs = agent_refs(&p);
        // Only non-terminal states are agents.
        assert!(refs.contains(&"draft".to_string()));
        assert!(
            !refs.contains(&"done".to_string()),
            "terminal must not be in refs"
        );
        assert!(
            !refs.contains(&"critique".to_string()),
            "terminal must not be in refs"
        );
    }

    #[test]
    fn refs_fsm_mixed_terminal_and_real_agents() {
        let mut states = HashMap::new();
        states.insert(
            "draft".to_string(),
            Some(FsmNext::Conditional(vec![
                HashMap::from([("needs-work".to_string(), Some("critique".to_string()))]),
                HashMap::from([("good-enough".to_string(), Some("done".to_string()))]),
            ])),
        );
        states.insert(
            "critique".to_string(),
            Some(FsmNext::Unconditional("draft".to_string())),
        );
        states.insert("done".to_string(), None); // terminal
        let p = AgentPattern::Fsm {
            initial: "draft".to_string(),
            states,
        };
        let refs = agent_refs(&p);
        assert!(refs.contains(&"draft".to_string()));
        assert!(refs.contains(&"critique".to_string()));
        assert!(
            !refs.contains(&"done".to_string()),
            "terminal must not be in refs"
        );
    }

    #[test]
    fn refs_fsm_terminal_state_no_refs() {
        let mut states = HashMap::new();
        states.insert("terminal".to_string(), None);
        let p = AgentPattern::Fsm {
            initial: "terminal".to_string(),
            states,
        };
        assert!(agent_refs(&p).is_empty());
    }
}
