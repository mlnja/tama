use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::config::TomlConfig;
use crate::runtime::graph::{AgentGraph, AgentNode};
use crate::skill::manifest::{AgentPattern, FsmNext};

// ── constants ─────────────────────────────────────────────────────────────────

const SKILL_COLOR: &str = "#10b981"; // emerald
const TOOL_COLOR: &str = "#f43f5e"; // rose

// ── entry point ───────────────────────────────────────────────────────────────

pub async fn run() -> Result<()> {
    let cfg = TomlConfig::load()?;
    let entrypoint = cfg.project.entrypoint;
    if entrypoint.is_empty() {
        bail!("no entrypoint in tama.toml — set [project] entrypoint = \"agent-name\"");
    }

    let graph = AgentGraph::build(&entrypoint)?;
    let data = generate_graph_data(&graph);
    let html = Arc::new(HTML_TEMPLATE.replace("__GRAPH_DATA__", &serde_json::to_string(&data)?));

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let url = format!("http://localhost:{}", port);

    open_in_browser(&url)?;
    eprintln!("  → tama preview at {url}  (Ctrl+C to stop)");

    loop {
        let (mut stream, _) = listener.accept().await?;
        let html = html.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(), *html
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
    }
}

// ── skill loading ─────────────────────────────────────────────────────────────

struct SkillInfo {
    description: String,
    tools: Vec<String>,
}

fn load_skill_info(name: &str) -> SkillInfo {
    let path = format!("skills/{name}/SKILL.md");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return SkillInfo {
            description: format!("(skill '{name}' not found)"),
            tools: vec![],
        };
    };

    let s = content.trim_start();
    let inner = match s.strip_prefix("---") {
        Some(r) => r,
        None => {
            return SkillInfo {
                description: String::new(),
                tools: vec![],
            }
        }
    };
    let end = match inner.find("\n---") {
        Some(e) => e,
        None => {
            return SkillInfo {
                description: String::new(),
                tools: vec![],
            }
        }
    };
    let yaml = &inner[..end];

    let mut description = String::new();
    let mut tools = vec![];
    for line in yaml.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("description:") {
            description = rest.trim().trim_matches('"').to_string();
        }
        if let Some(rest) = trimmed.strip_prefix("tools:") {
            let list = rest.trim().trim_start_matches('[').trim_end_matches(']');
            tools = list
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    SkillInfo { description, tools }
}

/// Collect skill names referenced by an agent's `call.uses` field.
fn agent_uses_from_node(node: &AgentNode) -> &[String] {
    node.agent
        .call
        .as_ref()
        .map(|c| c.uses.as_slice())
        .unwrap_or(&[])
}

// ── graph data generation ─────────────────────────────────────────────────────

fn generate_graph_data(graph: &AgentGraph) -> serde_json::Value {
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    let mut edges: Vec<serde_json::Value> = Vec::new();

    // Collect all referenced skills (deduped), tracking which agents use each
    let mut skill_agents: HashMap<String, Vec<String>> = HashMap::new();
    for (name, node) in &graph.nodes {
        for skill in agent_uses_from_node(node) {
            skill_agents
                .entry(skill.clone())
                .or_default()
                .push(name.clone());
        }
    }

    // Collect all referenced tools (deduped), tracking which skills use each
    let mut tool_skills: HashMap<String, Vec<String>> = HashMap::new();
    let mut skill_infos: HashMap<String, SkillInfo> = HashMap::new();
    for skill_name in skill_agents.keys() {
        let info = load_skill_info(skill_name);
        for tool in &info.tools {
            tool_skills
                .entry(tool.clone())
                .or_default()
                .push(skill_name.clone());
        }
        skill_infos.insert(skill_name.clone(), info);
    }

    // ── Agent nodes + agent→agent edges ──────────────────────────────────────
    for (name, node) in &graph.nodes {
        let pattern = &node.agent.pattern;
        let is_root = name == &graph.root;
        nodes.push(serde_json::json!({
            "id": name,
            "node_type": "agent",
            "label": format!("{}\n({})", name, pattern_name(pattern)),
            "color": pattern_color(pattern),
            "is_root": is_root,
            "detail_html": build_agent_detail(node, &skill_infos),
        }));

        match pattern {
            AgentPattern::Scatter { worker } => {
                edges.push(edge(name, worker, "worker"));
            }
            AgentPattern::Parallel { workers } => {
                for worker in workers {
                    edges.push(edge(name, worker, ""));
                }
            }
            AgentPattern::Debate { agents, judge, .. } => {
                for agent in agents {
                    edges.push(edge(name, agent, "agent"));
                }
                edges.push(edge(name, judge, "judge"));
            }
            AgentPattern::Fsm { states, .. } => {
                for (state, next) in states {
                    if let Some(next) = next {
                        match next {
                            FsmNext::Unconditional(target) => {
                                edges.push(edge(state, target, "→"));
                            }
                            FsmNext::Conditional(conds) => {
                                for cond in conds {
                                    for (word, target) in cond {
                                        if let Some(target) = target {
                                            edges.push(edge(state, target, word));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // agent → skill edges
        for skill in agent_uses_from_node(node) {
            let skill_id = skill_node_id(skill);
            edges.push(edge(name, &skill_id, "uses"));
        }
    }

    // ── Skill nodes + skill→tool edges ───────────────────────────────────────
    for (skill_name, info) in &skill_infos {
        let skill_id = skill_node_id(skill_name);
        let used_by: Vec<String> = skill_agents.get(skill_name).cloned().unwrap_or_default();
        nodes.push(serde_json::json!({
            "id": skill_id,
            "node_type": "skill",
            "label": format!("{}\n(skill)", skill_name),
            "color": SKILL_COLOR,
            "is_root": false,
            "detail_html": build_skill_detail(skill_name, info, &used_by),
        }));

        for tool in &info.tools {
            let tool_id = tool_node_id(tool);
            edges.push(edge(&skill_id, &tool_id, "tool"));
        }
    }

    // ── Tool nodes ────────────────────────────────────────────────────────────
    let all_tools: HashSet<String> = tool_skills.keys().cloned().collect();
    for tool_name in &all_tools {
        let tool_id = tool_node_id(tool_name);
        let used_by: Vec<String> = tool_skills.get(tool_name).cloned().unwrap_or_default();
        nodes.push(serde_json::json!({
            "id": tool_id,
            "node_type": "tool",
            "label": tool_name,
            "color": TOOL_COLOR,
            "is_root": false,
            "detail_html": build_tool_detail(tool_name, &used_by),
        }));
    }

    serde_json::json!({ "nodes": nodes, "edges": edges })
}

fn skill_node_id(name: &str) -> String {
    format!("skill:{name}")
}
fn tool_node_id(name: &str) -> String {
    format!("tool:{name}")
}

fn edge(source: &str, target: &str, label: &str) -> serde_json::Value {
    serde_json::json!({ "source": source, "target": target, "label": label })
}

// ── detail HTML builders ──────────────────────────────────────────────────────

fn build_agent_detail(node: &AgentNode, skill_infos: &HashMap<String, SkillInfo>) -> String {
    let pattern = &node.agent.pattern;
    let name = html_escape(&node.name);
    let description = html_escape(&node.agent.description);
    let body = node.agent.body.trim();
    let snippet: String = body.chars().take(500).collect();
    let snippet = html_escape(&snippet);
    let ellipsis = if body.len() > 500 { "…" } else { "" };

    let pattern_section = match pattern {
        AgentPattern::Fsm { initial, states } => {
            let mut rows = String::new();
            for (state, next) in states {
                let transition = match next {
                    None => "(terminal)".to_string(),
                    Some(FsmNext::Unconditional(t)) => format!("→ {}", t),
                    Some(FsmNext::Conditional(conds)) => conds
                        .iter()
                        .flat_map(|m| {
                            m.iter().map(|(w, t)| match t {
                                Some(target) => format!("{}: {}", w, target),
                                None => format!("{}: ~", w),
                            })
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                };
                let is_initial = if state == initial { " (initial)" } else { "" };
                rows.push_str(&format!(
                    "<tr><td><code>{}{}</code></td><td>{}</td></tr>",
                    html_escape(state),
                    is_initial,
                    html_escape(&transition)
                ));
            }
            format!(
                "<h3>States &amp; Transitions</h3><table><thead><tr><th>State</th><th>Transition</th></tr></thead><tbody>{rows}</tbody></table>"
            )
        }
        AgentPattern::Scatter { worker } => {
            format!("<p>Worker: <code>{}</code></p>", html_escape(worker))
        }
        AgentPattern::Parallel { workers } => {
            let list = workers
                .iter()
                .map(|s| html_escape(s))
                .collect::<Vec<_>>()
                .join(", ");
            format!("<p>Workers: {list}</p>")
        }
        AgentPattern::Debate {
            agents,
            judge,
            rounds,
        } => {
            let list = agents
                .iter()
                .map(|a| html_escape(a))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "<p>Agents: {list}</p><p>Judge: <code>{}</code></p><p>Rounds: {rounds}</p>",
                html_escape(judge)
            )
        }
        AgentPattern::Reflexion => String::new(),
        AgentPattern::BestOfN { n } => format!("<p>N variants: {n}</p>"),
        _ => String::new(),
    };

    let uses = agent_uses_from_node(node);
    let skills_section = if uses.is_empty() {
        String::new()
    } else {
        let items = uses
            .iter()
            .map(|s| {
                let desc = skill_infos
                    .get(s)
                    .map(|i| i.description.as_str())
                    .unwrap_or("");
                let tools = skill_infos
                    .get(s)
                    .map(|i| i.tools.join(", "))
                    .unwrap_or_default();
                format!(
                    "<li><code>{}</code>{}{}</li>",
                    html_escape(s),
                    if desc.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", html_escape(desc))
                    },
                    if tools.is_empty() {
                        String::new()
                    } else {
                        format!("<br><small>tools: {}</small>", html_escape(&tools))
                    },
                )
            })
            .collect::<String>();
        format!("<h3>Skills</h3><ul>{items}</ul>")
    };

    format!(
        "<h2>{name}</h2>\
        <div class=\"badge\">{pattern}</div>\
        <p>{description}</p>\
        <h3>System Prompt</h3>\
        <pre>{snippet}{ellipsis}</pre>\
        {pattern_section}\
        {skills_section}",
        pattern = pattern_name(pattern),
    )
}

fn build_skill_detail(name: &str, info: &SkillInfo, used_by: &[String]) -> String {
    let name_esc = html_escape(name);
    let desc_esc = html_escape(&info.description);

    let tools_section = if info.tools.is_empty() {
        "<p><em>No tools declared.</em></p>".to_string()
    } else {
        let items = info
            .tools
            .iter()
            .map(|t| format!("<li><code>{}</code></li>", html_escape(t)))
            .collect::<String>();
        format!("<ul>{items}</ul>")
    };

    let used_by_section = if used_by.is_empty() {
        String::new()
    } else {
        let items = used_by
            .iter()
            .map(|a| format!("<li><code>{}</code></li>", html_escape(a)))
            .collect::<String>();
        format!("<h3>Used by</h3><ul>{items}</ul>")
    };

    format!(
        "<h2>{name_esc}</h2>\
        <div class=\"badge\">skill</div>\
        <p>{desc_esc}</p>\
        <h3>Tools</h3>\
        {tools_section}\
        {used_by_section}"
    )
}

fn build_tool_detail(name: &str, used_by_skills: &[String]) -> String {
    let name_esc = html_escape(name);
    let skills_section = if used_by_skills.is_empty() {
        String::new()
    } else {
        let items = used_by_skills
            .iter()
            .map(|s| format!("<li><code>{}</code></li>", html_escape(s)))
            .collect::<String>();
        format!("<h3>Used by skills</h3><ul>{items}</ul>")
    };
    format!(
        "<h2>{name_esc}</h2>\
        <div class=\"badge\">tool</div>\
        {skills_section}"
    )
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn pattern_name(p: &AgentPattern) -> &'static str {
    match p {
        AgentPattern::React => "react",
        AgentPattern::Scatter { .. } => "scatter",
        AgentPattern::Parallel { .. } => "parallel",
        AgentPattern::Fsm { .. } => "fsm",
        AgentPattern::Critic => "critic",
        AgentPattern::Debate { .. } => "debate",
        AgentPattern::Reflexion => "reflexion",
        AgentPattern::Constitutional => "constitutional",
        AgentPattern::ChainOfVerification => "chain-of-verification",
        AgentPattern::PlanExecute => "plan-execute",
        AgentPattern::BestOfN { .. } => "best-of-n",
        AgentPattern::Human => "human",
        AgentPattern::Oneshot => "oneshot",
    }
}

fn pattern_color(p: &AgentPattern) -> &'static str {
    match p {
        AgentPattern::React => "#3b82f6",
        AgentPattern::Scatter { .. } => "#8b5cf6",
        AgentPattern::Parallel { .. } => "#14b8a6",
        AgentPattern::Fsm { .. } => "#f59e0b",
        AgentPattern::Critic => "#22c55e",
        AgentPattern::Debate { .. } => "#ef4444",
        AgentPattern::Reflexion => "#ec4899",
        AgentPattern::Constitutional => "#eab308",
        AgentPattern::ChainOfVerification => "#84cc16",
        AgentPattern::PlanExecute => "#f97316",
        AgentPattern::BestOfN { .. } => "#06b6d4",
        AgentPattern::Human => "#64748b",
        AgentPattern::Oneshot => "#6b7280",
    }
}

fn open_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn()?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(url).spawn()?;

    Ok(())
}

// ── HTML template ─────────────────────────────────────────────────────────────

static HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>tama agent graph</title>
<script src="https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.28.1/cytoscape.min.js"></script>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { display: flex; height: 100vh; font-family: system-ui, sans-serif; background: #0f172a; color: #e2e8f0; }
  #cy { flex: 1; }
  #detail {
    width: 340px; min-width: 340px; overflow-y: auto; padding: 20px;
    background: #1e293b; border-left: 1px solid #334155;
  }
  #detail .placeholder { color: #64748b; font-style: italic; margin-top: 40%; text-align: center; }
  #detail h2 { font-size: 1.2rem; margin-bottom: 8px; color: #f1f5f9; }
  #detail h3 { font-size: 0.8rem; margin: 14px 0 5px; color: #64748b; text-transform: uppercase; letter-spacing: 0.06em; }
  #detail p { font-size: 0.9rem; margin-bottom: 8px; line-height: 1.5; color: #cbd5e1; }
  #detail ul { margin: 0 0 8px 16px; }
  #detail li { font-size: 0.88rem; color: #cbd5e1; line-height: 1.6; }
  #detail small { font-size: 0.78rem; color: #64748b; }
  #detail pre {
    font-size: 0.78rem; background: #0f172a; border: 1px solid #334155;
    border-radius: 6px; padding: 10px; white-space: pre-wrap; word-break: break-word;
    color: #94a3b8; max-height: 220px; overflow-y: auto;
  }
  #detail code { background: #0f172a; padding: 2px 5px; border-radius: 4px; font-size: 0.83em; color: #7dd3fc; }
  #detail .badge {
    display: inline-block; padding: 3px 10px; border-radius: 999px;
    font-size: 0.72rem; font-weight: 700; background: #334155; color: #94a3b8;
    margin-bottom: 12px; text-transform: uppercase; letter-spacing: 0.06em;
  }
  #detail table { width: 100%; border-collapse: collapse; font-size: 0.82rem; margin-top: 6px; }
  #detail th { text-align: left; padding: 6px 8px; background: #0f172a; color: #64748b; font-weight: 600; }
  #detail td { padding: 5px 8px; border-top: 1px solid #334155; color: #cbd5e1; }

  #legend {
    position: absolute; bottom: 16px; left: 16px;
    background: #1e293b; border: 1px solid #334155; border-radius: 8px;
    padding: 10px 14px; display: flex; flex-direction: column; gap: 6px;
    font-size: 0.78rem; color: #94a3b8;
  }
  .legend-item { display: flex; align-items: center; gap: 8px; pointer-events: none; }
  .legend-dot { width: 12px; height: 12px; border-radius: 3px; flex-shrink: 0; }
  #btn-fit {
    margin-top: 4px; padding: 6px 10px; border-radius: 6px;
    background: #0f172a; border: 1px solid #475569;
    font-size: 0.78rem; font-family: system-ui, sans-serif;
    color: #94a3b8; cursor: pointer; transition: background 0.15s, color 0.15s;
  }
  #btn-fit:hover { background: #334155; color: #e2e8f0; }

  #hint {
    position: absolute; top: 14px; left: 50%; transform: translateX(-50%);
    background: #1e293b99; border: 1px solid #334155; border-radius: 20px;
    padding: 6px 16px; font-size: 0.76rem; color: #64748b; pointer-events: none;
    white-space: nowrap;
  }
</style>
</head>
<body>
<div id="cy"></div>
<div id="hint">Click an agent to reveal its skills · click a skill to reveal tools · click canvas to collapse</div>
<div id="legend">
  <div class="legend-item"><div class="legend-dot" style="background:#3b82f6"></div>agent</div>
  <div class="legend-item"><div class="legend-dot" style="background:#10b981;border-radius:50%"></div>skill (click agent)</div>
  <div class="legend-item"><div class="legend-dot" style="background:#f43f5e;border-radius:50%;width:10px;height:10px"></div>tool (click skill)</div>
  <button id="btn-fit">Show all</button>
</div>
<div id="detail"><p class="placeholder">Click a node to see details</p></div>
<script>
const GRAPH_DATA = __GRAPH_DATA__;

const cy = cytoscape({
  container: document.getElementById('cy'),
  elements: {
    nodes: GRAPH_DATA.nodes.map(n => ({ data: n })),
    edges: GRAPH_DATA.edges.map(e => ({ data: { source: e.source, target: e.target, label: e.label } }))
  },
  layout: { name: 'preset' },
  autoungrabify: true,
  maxZoom: 2,
  wheelSensitivity: 0.3,
  style: [
    {
      selector: 'node[node_type = "agent"]',
      style: {
        shape: 'roundrectangle',
        'background-color': 'data(color)',
        label: 'data(label)',
        color: '#ffffff',
        'text-valign': 'center', 'text-halign': 'center',
        'text-wrap': 'wrap', 'text-max-width': '140px',
        'font-size': '13px', 'font-family': 'system-ui, sans-serif',
        width: 160, height: 68,
        'border-width': 2, 'border-color': 'data(color)', 'border-opacity': 0.5,
      }
    },
    {
      selector: 'node[node_type = "agent"][?is_root]',
      style: {
        width: 180, height: 76,
        'font-size': '14px', 'font-weight': 'bold',
        'border-width': 3, 'border-color': '#ffffff', 'border-opacity': 0.9,
      }
    },
    {
      selector: 'node[node_type = "skill"]',
      style: {
        shape: 'hexagon',
        'background-color': '#10b981',
        label: 'data(label)',
        color: '#ffffff',
        'text-valign': 'center', 'text-halign': 'center',
        'text-wrap': 'wrap', 'text-max-width': '80px',
        'font-size': '10px', 'font-family': 'system-ui, sans-serif',
        width: 90, height: 44,
        'border-width': 1.5, 'border-color': '#34d399', 'border-opacity': 0.7,
      }
    },
    {
      selector: 'node[node_type = "tool"]',
      style: {
        shape: 'ellipse',
        'background-color': '#f43f5e',
        label: 'data(label)',
        color: '#ffffff',
        'text-valign': 'center', 'text-halign': 'center',
        'font-size': '9px', 'font-family': 'system-ui, sans-serif',
        width: 72, height: 28,
        'border-width': 1, 'border-color': '#fb7185', 'border-opacity': 0.6,
      }
    },
    {
      selector: 'node:selected',
      style: {
        'border-color': '#ffffff', 'border-width': 3, 'border-opacity': 1,
        'overlay-color': '#ffffff', 'overlay-opacity': 0.08,
      }
    },
    {
      selector: 'edge[label != "uses"][label != "tool"]',
      style: {
        width: 2, 'line-color': '#475569',
        'target-arrow-color': '#475569', 'target-arrow-shape': 'triangle',
        'curve-style': 'bezier',
        label: 'data(label)', 'font-size': '10px', color: '#94a3b8',
        'text-background-color': '#1e293b', 'text-background-opacity': 1,
        'text-background-padding': '3px',
      }
    },
    {
      selector: 'edge[label = "uses"]',
      style: {
        width: 1.5, 'line-color': '#34d399', 'line-style': 'dashed',
        'target-arrow-color': '#34d399', 'target-arrow-shape': 'triangle',
        'curve-style': 'bezier',
        label: 'uses', 'font-size': '9px', color: '#34d399',
        'text-background-color': '#0f172a', 'text-background-opacity': 0.8,
        'text-background-padding': '2px',
      }
    },
    {
      selector: 'edge[label = "tool"]',
      style: {
        width: 1, 'line-color': '#fb7185', 'line-style': 'dashed',
        'target-arrow-color': '#fb7185', 'target-arrow-shape': 'triangle',
        'curve-style': 'bezier',
        label: 'tool', 'font-size': '9px', color: '#fb7185',
        'text-background-color': '#0f172a', 'text-background-opacity': 0.8,
        'text-background-padding': '2px',
      }
    },
  ]
});

// --- initial layout: agents only ---
const agentOnlyElements = cy.nodes('[node_type = "agent"]').union(
  cy.edges().filter(e => e.data('label') !== 'uses' && e.data('label') !== 'tool')
);
const root = cy.nodes('[node_type = "agent"]').filter(n => n.data('is_root'));
agentOnlyElements.layout({
  name: 'breadthfirst', directed: true, padding: 80, spacingFactor: 2.0,
  roots: root.length ? root : undefined,
}).run();

// hide skills/tools after layout so they don't affect positioning
cy.nodes('[node_type = "skill"], [node_type = "tool"]').style('display', 'none');
cy.edges('[label = "uses"], [label = "tool"]').style('display', 'none');

cy.fit(cy.nodes('[node_type = "agent"]'), 80);

// --- helpers ---
function collapseAll() {
  cy.nodes('[node_type = "skill"], [node_type = "tool"]').style('display', 'none');
  cy.edges('[label = "uses"], [label = "tool"]').style('display', 'none');
  cy.elements().style('opacity', 1);
}

function focusNode(node) {
  cy.elements(':visible').style('opacity', 0.15);
  node.style('opacity', 1);
}

function orbitAround(center, neighbors, radius, startAngle) {
  if (!neighbors || neighbors.length === 0) return;
  const pos = center.position();
  const n = neighbors.length;
  neighbors.forEach((nb, i) => {
    const angle = startAngle + (2 * Math.PI * i / n);
    const tx = pos.x + radius * Math.cos(angle);
    const ty = pos.y + radius * Math.sin(angle);
    nb.position(pos);
    nb.style({ display: 'element', opacity: 0 });
    nb.animate(
      { position: { x: tx, y: ty }, style: { opacity: 1 } },
      { duration: 260, easing: 'ease-out-cubic' }
    );
  });
}

// Skills start directly LEFT or RIGHT — whichever side has fewer other agents.
// angle reference: -PI/2=top  0=right  PI/2=bottom  PI=left  (Cytoscape Y grows downward)
function skillStartAngle(centerNode) {
  const pos = centerNode.position();
  const others = cy.nodes('[node_type = "agent"]').not(centerNode).filter(':visible');
  if (others.length === 0) return Math.PI; // default left
  let sumX = 0;
  others.forEach(n => sumX += n.position().x);
  const avgX = sumX / others.length;
  // Other agents mostly to the right → skills go left; mostly to the left → skills go right
  return avgX > pos.x ? Math.PI : 0;
}

// Tools start from top or bottom, whichever side has fewer visible nodes.
function toolStartAngle(skillNode) {
  const pos = skillNode.position();
  const visible = cy.elements(':visible').nodes().not(skillNode);
  const above = visible.filter(n => n.position().y < pos.y).length;
  const below = visible.filter(n => n.position().y > pos.y).length;
  return above > below ? Math.PI / 2 : -Math.PI / 2;
}

// --- event handlers ---
function smoothCenter(node) {
  cy.animate({ center: { eles: node } }, { duration: 380, easing: 'ease-in-out-cubic' });
}

cy.on('tap', 'node[node_type = "agent"]', function(evt) {
  const node = evt.target;
  collapseAll();
  focusNode(node);
  smoothCenter(node);
  const skillEdges = node.connectedEdges().filter(
    e => e.data('label') === 'uses' && e.source().id() === node.id()
  );
  orbitAround(node, skillEdges.targets(), 220, skillStartAngle(node));
  skillEdges.style('display', 'element');
  document.getElementById('detail').innerHTML = node.data('detail_html');
});

cy.on('tap', 'node[node_type = "skill"]', function(evt) {
  const node = evt.target;
  cy.nodes('[node_type = "tool"]').style('display', 'none');
  cy.edges('[label = "tool"]').style('display', 'none');
  focusNode(node);
  smoothCenter(node);
  const toolEdges = node.connectedEdges().filter(
    e => e.data('label') === 'tool' && e.source().id() === node.id()
  );
  orbitAround(node, toolEdges.targets(), 140, toolStartAngle(node));
  toolEdges.style('display', 'element');
  document.getElementById('detail').innerHTML = node.data('detail_html');
});

cy.on('tap', 'node[node_type = "tool"]', function(evt) {
  focusNode(evt.target);
  smoothCenter(evt.target);
  document.getElementById('detail').innerHTML = evt.target.data('detail_html');
});

cy.on('tap', function(evt) {
  if (evt.target === cy) {
    collapseAll();
    document.getElementById('detail').innerHTML = '<p class="placeholder">Click a node to see details</p>';
  }
});

document.getElementById('btn-fit').addEventListener('click', function() {
  collapseAll();
  document.getElementById('detail').innerHTML = '<p class="placeholder">Click a node to see details</p>';
  cy.animate({ fit: { eles: cy.nodes('[node_type = "agent"]'), padding: 80 } }, { duration: 400, easing: 'ease-in-out-cubic' });
});
</script>
</body>
</html>
"#;
