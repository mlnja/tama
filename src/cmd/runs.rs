use anyhow::{Context, Result};
use duckdb::Connection;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist/"]
struct WebDist;

const DB_PATH: &str = ".tama/runs.duckdb";

pub fn list() -> Result<()> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT trace_id, timestamp, entrypoint, task, status, duration_ms
         FROM runs ORDER BY timestamp DESC LIMIT 20",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?, // trace_id
            row.get::<_, i64>(1)?,    // timestamp
            row.get::<_, String>(2)?, // entrypoint
            row.get::<_, String>(3)?, // task
            row.get::<_, String>(4)?, // status
            row.get::<_, i64>(5)?,    // duration_ms
        ))
    })?;

    println!(
        "{:<38} {:<20} {:<12} {:<8} task",
        "trace_id", "entrypoint", "status", "ms"
    );
    println!("{}", "-".repeat(100));
    for row in rows {
        let (trace_id, _ts, entrypoint, task, status, duration_ms) = row?;
        let task_short: String = task.chars().take(40).collect();
        println!(
            "{:<38} {:<20} {:<12} {:<8} {}",
            trace_id, entrypoint, status, duration_ms, task_short
        );
    }
    Ok(())
}

pub fn show(trace_id: &str, show_llm: bool) -> Result<()> {
    let conn = open_db()?;

    let mut stmt = conn.prepare(
        "SELECT entrypoint, task, status, output, duration_ms FROM runs WHERE trace_id=?",
    )?;
    let run = stmt
        .query_row([trace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .with_context(|| format!("run '{trace_id}' not found"))?;

    let (entrypoint, task, status, output, duration_ms) = run;
    println!("trace_id:   {trace_id}");
    println!("entrypoint: {entrypoint}");
    println!("status:     {status}");
    println!("duration:   {duration_ms}ms");
    println!("task:       {task}");
    println!();

    let mut stmt = conn.prepare(
        "SELECT span_id, parent_span_id, name, kind, start_ms, end_ms
         FROM spans WHERE trace_id=? ORDER BY start_ms",
    )?;
    let spans: Vec<_> = stmt
        .query_map([trace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    println!("spans:");
    for (span_id, _parent, name, kind, start_ms, end_ms) in &spans {
        let dur = end_ms - start_ms;
        println!("  [{kind:>5}] {name:<40} {dur}ms  ({span_id})");

        if show_llm && kind == "llm" {
            if let Ok(llm) = get_llm_call(&conn, span_id) {
                println!("           model: {}", llm.0);
                if let Some(temp) = llm.5 {
                    println!("           temperature: {temp}");
                }
                println!("           in={} out={} tokens", llm.3, llm.4);
                println!("           system: {}", truncate(&llm.1, 200));
                println!("           response: {}", truncate(&llm.2, 300));
            }
        }
    }

    println!();
    println!("output:");
    println!("{output}");
    Ok(())
}

pub async fn retry(trace_id: &str) -> Result<()> {
    let conn = open_db()?;
    let (entrypoint, task) = conn
        .query_row(
            "SELECT entrypoint, task FROM runs WHERE trace_id=?",
            [trace_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .with_context(|| format!("run '{trace_id}' not found"))?;

    eprintln!("retrying run {trace_id}: entrypoint={entrypoint} task={task:?}");
    drop(conn);

    super::run::run(&task, Some(&entrypoint), false, vec![]).await
}

// ── Web dashboard ─────────────────────────────────────────────────────────────

pub async fn serve() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let url = format!("http://localhost:{port}");

    open_in_browser(&url);
    eprintln!("  → tama runs dashboard at {url}  (Ctrl+C to stop)");

    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let path = parse_request_path(&buf[..n]);
            let response = tokio::task::block_in_place(|| build_response(&path));
            let _ = stream.write_all(&response).await;
        });
    }
}

fn parse_request_path(buf: &[u8]) -> String {
    let s = String::from_utf8_lossy(buf);
    let first_line = s.lines().next().unwrap_or("");
    let mut parts = first_line.splitn(3, ' ');
    let _ = parts.next();
    let path = parts.next().unwrap_or("/");
    path.split('?').next().unwrap_or("/").to_string()
}

fn build_response(path: &str) -> Vec<u8> {
    // JSON API routes
    let json_result: Option<Result<String>> = match path {
        "/api/runs" => Some(api_list_runs()),
        p if p.starts_with("/api/runs/") && p.ends_with("/spans") => {
            let trace_id = &p["/api/runs/".len()..p.len() - "/spans".len()];
            Some(api_list_spans(trace_id))
        }
        p if p.starts_with("/api/spans/") => Some(api_span_detail(&p["/api/spans/".len()..])),
        p if p.starts_with("/api/runs/") && p.ends_with("/tree") => {
            let trace_id = &p["/api/runs/".len()..p.len() - "/tree".len()];
            Some(api_run_tree(trace_id))
        }
        p if p.starts_with("/api/agents/") && p.ends_with("/timeline") => {
            let span_id = &p["/api/agents/".len()..p.len() - "/timeline".len()];
            Some(api_agent_timeline(span_id))
        }
        _ => None,
    };

    if let Some(result) = json_result {
        let body = result.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"));
        return http_response("application/json", body.as_bytes());
    }

    // Static file serving from embedded web/dist/
    let file_path = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    match WebDist::get(file_path) {
        Some(file) => {
            let mime = mime_type(file_path);
            http_response(mime, &file.data)
        }
        None => {
            // SPA fallback: serve index.html for unknown paths
            match WebDist::get("index.html") {
                Some(file) => http_response("text/html; charset=utf-8", &file.data),
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_vec(),
            }
        }
    }
}

fn http_response(content_type: &str, body: &[u8]) -> Vec<u8> {
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut resp = header.into_bytes();
    resp.extend_from_slice(body);
    resp
}

fn mime_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

fn api_list_runs() -> Result<String> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT trace_id, timestamp, entrypoint, task, status, duration_ms
         FROM runs ORDER BY timestamp DESC LIMIT 50",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

    let json: Vec<_> = rows.iter().map(|(tid, ts, ep, task, st, dur)| {
        serde_json::json!({ "trace_id": tid, "timestamp": ts, "entrypoint": ep, "task": task, "status": st, "duration_ms": dur })
    }).collect();
    Ok(serde_json::to_string(&json)?)
}

fn api_list_spans(trace_id: &str) -> Result<String> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT span_id, parent_span_id, name, kind, start_ms, end_ms
         FROM spans WHERE trace_id=? ORDER BY start_ms",
    )?;
    let rows = stmt
        .query_map([trace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

    let json: Vec<_> = rows.iter().map(|(sid, pid, name, kind, start, end)| {
        serde_json::json!({ "span_id": sid, "parent_span_id": pid, "name": name, "kind": kind, "start_ms": start, "end_ms": end })
    }).collect();
    Ok(serde_json::to_string(&json)?)
}

fn api_span_detail(span_id: &str) -> Result<String> {
    let conn = open_db()?;
    let (kind, name, start_ms, end_ms): (String, String, i64, i64) = conn.query_row(
        "SELECT kind, name, start_ms, end_ms FROM spans WHERE span_id=?",
        [span_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let duration_ms = end_ms - start_ms;

    let v = if kind == "llm" {
        let (model, system_prompt, response, in_tok, out_tok, temperature, role): (String, String, String, i32, i32, Option<f64>, String) = conn.query_row(
            "SELECT model, system_prompt, response, input_tokens, output_tokens, temperature, role FROM llm_calls WHERE span_id=?",
            [span_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
        )?;
        serde_json::json!({ "kind": "llm", "name": name, "model": model, "role": role, "temperature": temperature, "system_prompt": system_prompt, "response": response, "input_tokens": in_tok, "output_tokens": out_tok, "duration_ms": duration_ms })
    } else if kind == "tool" {
        let (tool_name, args_json, result, dur): (String, String, String, i64) = conn.query_row(
            "SELECT tool_name, args_json, result, duration_ms FROM tool_calls WHERE span_id=?",
            [span_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        serde_json::json!({ "kind": "tool", "name": name, "tool_name": tool_name, "args_json": args_json, "result": result, "duration_ms": dur })
    } else {
        serde_json::json!({ "kind": kind, "name": name, "duration_ms": duration_ms })
    };
    Ok(serde_json::to_string(&v)?)
}

fn open_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn open_db() -> Result<Connection> {
    Connection::open(DB_PATH)
        .context("failed to open .tama/runs.duckdb — have you run `tama run` yet?")
}

fn get_llm_call(
    conn: &Connection,
    span_id: &str,
) -> Result<(String, String, String, i32, i32, Option<f64>)> {
    conn.query_row(
        "SELECT model, system_prompt, response, input_tokens, output_tokens, temperature FROM llm_calls WHERE span_id=?",
        [span_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).context("llm_call not found")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

// ── Tree API ──────────────────────────────────────────────────────────────────

fn api_run_tree(trace_id: &str) -> Result<String> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT span_id, parent_span_id, prev_span_id, node_id, name, start_ms, end_ms
         FROM spans WHERE trace_id=? AND kind='agent' ORDER BY start_ms",
    )?;
    // (span_id, parent_span_id, prev_span_id, node_id, name, start_ms, end_ms)
    type Row = (
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        i64,
        i64,
    );
    let spans: Vec<Row> = stmt
        .query_map([trace_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let all_ids: std::collections::HashSet<String> = spans.iter().map(|s| s.0.clone()).collect();
    let groups = build_agent_groups(None, &spans, &all_ids);
    Ok(serde_json::to_string(&groups)?)
}

fn build_agent_groups(
    parent_id: Option<&str>,
    spans: &[(
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        i64,
        i64,
    )],
    all_ids: &std::collections::HashSet<String>,
) -> Vec<serde_json::Value> {
    // (span_id, parent_span_id, prev_span_id, node_id, name, start_ms, end_ms)
    let children: Vec<_> = spans
        .iter()
        .filter(|(_, pid, _, _, _, _, _)| match parent_id {
            None => pid.as_ref().map(|p| !all_ids.contains(p)).unwrap_or(true),
            Some(p) => pid.as_deref() == Some(p),
        })
        .collect();

    // Parallel detection: multiple children sharing the same prev_span_id.
    let mut prev_counts: std::collections::HashMap<Option<&str>, usize> =
        std::collections::HashMap::new();
    for (_, _, prev, _, _, _, _) in &children {
        *prev_counts.entry(prev.as_deref()).or_insert(0) += 1;
    }
    let parallel_prevs: std::collections::HashSet<Option<&str>> = prev_counts
        .into_iter()
        .filter(|(_, c)| *c > 1)
        .map(|(p, _)| p)
        .collect();

    // Group by node_id — each unique node_id is one logical invocation; retries share it.
    let mut seen: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
    for span in &children {
        let nid = &span.3;
        if !groups.contains_key(nid) {
            seen.push(nid.clone());
        }
        groups.entry(nid.clone()).or_default().push(*span);
    }

    seen.into_iter()
        .map(|nid| {
            let attempts = &groups[&nid];
            // Use the first attempt's name and prev for display/parallel detection
            let (_, _, first_prev, _, name, _, _) = attempts[0];
            let is_parallel = parallel_prevs.contains(&first_prev.as_deref());

            let parts: Vec<&str> = name.splitn(3, ':').collect();
            let agent_name = if parts.len() >= 2 {
                parts[1]
            } else {
                name.as_str()
            };
            let pattern = if parts.len() >= 3 { parts[2] } else { "" };

            let attempts_json: Vec<_> = attempts
                .iter()
                .map(|(sid, _, _, _, _, start, end)| {
                    serde_json::json!({
                        "span_id": sid,
                        "duration_ms": end - start,
                        "children": build_agent_groups(Some(sid), spans, all_ids),
                    })
                })
                .collect();

            serde_json::json!({
                "id": nid,
                "name": agent_name,
                "pattern": pattern,
                "is_parallel": is_parallel,
                "attempts": attempts_json,
            })
        })
        .collect()
}

// ── Timeline API ──────────────────────────────────────────────────────────────

fn api_agent_timeline(span_id: &str) -> Result<String> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT span_id, kind, name, start_ms, end_ms
         FROM spans WHERE parent_span_id=? AND kind IN ('llm','tool','synthetic') ORDER BY seq",
    )?;
    let child_spans: Vec<(String, String, String, i64, i64)> = stmt
        .query_map([span_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut events: Vec<serde_json::Value> = Vec::new();
    let mut system_prompt_sent = false;
    for (sid, kind, name, start_ms, end_ms) in child_spans {
        let duration_ms = end_ms - start_ms;
        let step = name.split(':').nth(1).unwrap_or(&name).to_string();
        if kind == "llm" {
            if let Ok((model, system_prompt, response, in_tok, out_tok, temperature, role)) = conn.query_row(
                "SELECT model, COALESCE(system_prompt,''), response, input_tokens, output_tokens, temperature, role FROM llm_calls WHERE span_id=?",
                [&sid],
                |row| Ok((row.get::<_,String>(0)?, row.get::<_,String>(1)?, row.get::<_,String>(2)?, row.get::<_,i32>(3)?, row.get::<_,i32>(4)?, row.get::<_,Option<f64>>(5)?, row.get::<_,String>(6)?)),
            ) {
                // Send system_prompt only on first LLM event; empty string for the rest
                let sp = if !system_prompt_sent && !system_prompt.is_empty() {
                    system_prompt_sent = true;
                    system_prompt
                } else {
                    String::new()
                };
                events.push(serde_json::json!({
                    "kind": "llm", "step": step, "duration_ms": duration_ms,
                    "model": model, "role": role, "temperature": temperature, "system_prompt": sp, "response": response,
                    "input_tokens": in_tok, "output_tokens": out_tok,
                }));
            }
        } else if kind == "tool" || kind == "synthetic" {
            if let Ok((tool_name, args_json, result)) = conn.query_row(
                "SELECT tool_name, args_json, result FROM tool_calls WHERE span_id=?",
                [&sid],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            ) {
                events.push(serde_json::json!({
                    "kind": kind, "tool_name": tool_name, "duration_ms": duration_ms,
                    "args_json": args_json, "result": result,
                }));
            }
        }
    }
    Ok(serde_json::to_string(&events)?)
}
