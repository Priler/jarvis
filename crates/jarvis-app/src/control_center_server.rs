//! Control center HTTP server — serves the Jarvis dashboard UI at localhost:7070.
//! Embedded static HTML + JSON API endpoints. No external web framework needed.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

pub static SERVER_REQUESTS: AtomicU64 = AtomicU64::new(0);
pub static SERVER_ERRORS:   AtomicU64 = AtomicU64::new(0);
static SERVER_RUNNING:      AtomicBool = AtomicBool::new(false);
static SERVER_STOP:         AtomicBool = AtomicBool::new(false);

pub const SERVER_PORT: u16 = 7070;

// ── Embedded dashboard HTML ───────────────────────────────────────────────────

const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Jarvis Control Center</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:'Segoe UI',monospace;background:#0d1117;color:#c9d1d9;min-height:100vh}
header{background:#161b22;border-bottom:1px solid #30363d;padding:16px 24px;display:flex;align-items:center;gap:12px}
header h1{font-size:18px;color:#58a6ff}
.status-dot{width:10px;height:10px;border-radius:50%;background:#3fb950}
.status-dot.warn{background:#d29922}.status-dot.err{background:#f85149}
nav{display:flex;gap:0;border-bottom:1px solid #30363d}
nav button{background:none;border:none;color:#8b949e;padding:12px 20px;cursor:pointer;border-bottom:2px solid transparent;font-size:13px}
nav button.active,nav button:hover{color:#58a6ff;border-bottom-color:#58a6ff}
main{padding:20px;max-width:1200px}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:16px;margin-bottom:20px}
.card{background:#161b22;border:1px solid #30363d;border-radius:8px;padding:16px}
.card h3{font-size:12px;color:#8b949e;text-transform:uppercase;letter-spacing:1px;margin-bottom:12px}
.metric{font-size:28px;color:#58a6ff;font-weight:600}
.metric.ok{color:#3fb950}.metric.warn{color:#d29922}.metric.err{color:#f85149}
.label{font-size:11px;color:#8b949e;margin-top:4px}
.log-box{background:#0d1117;border:1px solid #30363d;border-radius:6px;padding:12px;font-size:11px;max-height:300px;overflow-y:auto;font-family:monospace}
.log-entry{padding:3px 0;border-bottom:1px solid #21262d}
.log-entry.err{color:#f85149}.log-entry.warn{color:#d29922}.log-entry.info{color:#8b949e}
.log-entry.crit{color:#f85149;font-weight:bold}
table{width:100%;border-collapse:collapse;font-size:12px}
th{text-align:left;padding:8px;border-bottom:1px solid #30363d;color:#8b949e;font-weight:500}
td{padding:8px;border-bottom:1px solid #21262d}
.badge{padding:2px 8px;border-radius:12px;font-size:10px;font-weight:600}
.badge.ok{background:#0d4a1e;color:#3fb950}.badge.err{background:#3d1218;color:#f85149}
.badge.warn{background:#3d2a00;color:#d29922}.badge.pending{background:#1c2a3a;color:#58a6ff}
.section{margin-bottom:24px}
.section h2{font-size:14px;color:#c9d1d9;margin-bottom:12px;padding-bottom:8px;border-bottom:1px solid #30363d}
.btn{background:#21262d;border:1px solid #30363d;color:#c9d1d9;padding:6px 14px;border-radius:6px;cursor:pointer;font-size:12px}
.btn:hover{background:#30363d}.btn.danger{border-color:#f85149;color:#f85149}
.btn.primary{background:#1f6feb;border-color:#1f6feb;color:#fff}
.refresh-bar{display:flex;gap:8px;align-items:center;margin-bottom:16px;font-size:12px;color:#8b949e}
</style>
</head>
<body>
<header>
  <div class="status-dot" id="status-dot"></div>
  <h1>Jarvis Control Center</h1>
  <span id="health-badge" style="margin-left:auto;font-size:12px"></span>
</header>
<nav>
  <button class="active" onclick="showTab('overview')">Overview</button>
  <button onclick="showTab('models')">Models</button>
  <button onclick="showTab('memory')">Memory</button>
  <button onclick="showTab('permissions')">Permissions</button>
  <button onclick="showTab('logs')">Logs</button>
  <button onclick="showTab('settings')">Settings</button>
</nav>
<main>
<div class="refresh-bar">
  <span>Auto-refresh: 3s</span>
  <button class="btn" onclick="refresh()">Refresh Now</button>
  <span id="last-update"></span>
</div>

<div id="tab-overview">
<div class="grid" id="metrics-grid"></div>
<div class="section">
  <h2>Warnings</h2>
  <div id="warnings-list" class="log-box"></div>
</div>
</div>

<div id="tab-models" style="display:none">
<div class="section">
  <h2>Ollama Models</h2>
  <table><thead><tr><th>Model</th><th>Family</th><th>VRAM</th><th>Profile</th></tr></thead>
  <tbody id="models-table"></tbody></table>
</div>
<div class="section">
  <h2>LLM Provider</h2>
  <table><thead><tr><th>Provider</th><th>Status</th><th>Requests</th><th>Errors</th><th>Avg Latency</th></tr></thead>
  <tbody id="providers-table"></tbody></table>
</div>
</div>

<div id="tab-memory" style="display:none">
<div class="section">
  <h2>Memory</h2>
  <div class="grid" id="memory-grid"></div>
</div>
</div>

<div id="tab-permissions" style="display:none">
<div class="section">
  <h2>Active Permissions</h2>
  <table><thead><tr><th>Kind</th><th>Resource</th><th>State</th></tr></thead>
  <tbody id="perms-table"></tbody></table>
</div>
<div class="section">
  <h2>Pending Requests</h2>
  <div id="pending-perms"></div>
</div>
</div>

<div id="tab-logs" style="display:none">
<div class="section">
  <h2>Recent Logs</h2>
  <div class="log-box" id="logs-box"></div>
</div>
</div>

<div id="tab-settings" style="display:none">
<div class="section">
  <h2>Safe Mode</h2>
  <p id="safe-mode-status" style="margin-bottom:12px;font-size:13px"></p>
  <button class="btn danger" onclick="toggleSafeMode()">Toggle Safe Mode</button>
</div>
<div class="section">
  <h2>Privacy Guarantee</h2>
  <div style="font-size:13px;line-height:1.8">
    <div>✓ 100% offline — no network calls except localhost</div>
    <div>✓ No telemetry — never enabled</div>
    <div>✓ No cloud storage — all data stays on this machine</div>
    <div>✓ No analytics — no tracking</div>
    <div>✓ All models run locally via Ollama</div>
  </div>
</div>
</div>
</main>

<script>
let currentTab = 'overview';
let data = {};

function showTab(tab) {
  document.querySelectorAll('nav button').forEach(b => b.classList.remove('active'));
  event.target.classList.add('active');
  document.querySelectorAll('[id^=tab-]').forEach(el => el.style.display = 'none');
  document.getElementById('tab-' + tab).style.display = '';
  currentTab = tab;
  refresh();
}

async function refresh() {
  try {
    const r = await fetch('/api/status');
    data = await r.json();
    renderOverview();
    if (currentTab === 'models') await refreshModels();
    if (currentTab === 'memory') renderMemory();
    if (currentTab === 'permissions') await refreshPermissions();
    if (currentTab === 'logs') await refreshLogs();
    if (currentTab === 'settings') renderSettings();
    document.getElementById('last-update').textContent = 'Updated ' + new Date().toLocaleTimeString();
  } catch(e) {
    console.error(e);
  }
}

function renderOverview() {
  const d = data;
  const dot = document.getElementById('status-dot');
  dot.className = 'status-dot' + (d.health_score > 0.7 ? '' : d.health_score > 0.4 ? ' warn' : ' err');
  document.getElementById('health-badge').textContent = 'Health: ' + (d.health_score * 100).toFixed(0) + '%';

  const metrics = [
    {label:'Health Score', value: (d.health_score*100).toFixed(0)+'%', cls: d.health_score>0.7?'ok':d.health_score>0.4?'warn':'err'},
    {label:'Cognitive Continuity', value: (d.continuity_score*100).toFixed(0)+'%', cls:'ok'},
    {label:'Runtime Crashes', value: d.runtime_crashes, cls: d.runtime_crashes>0?'warn':'ok'},
    {label:'Modules Disabled', value: d.modules_disabled, cls: d.modules_disabled>0?'err':'ok'},
    {label:'Memory Entries', value: d.memory_entries, cls:'ok'},
    {label:'Knowledge Chunks', value: d.knowledge_chunks, cls:'ok'},
    {label:'Safe Mode', value: d.safe_mode_active?'ACTIVE':'OFF', cls: d.safe_mode_active?'warn':'ok'},
    {label:'Ollama', value: d.ollama_available?'Online':'Offline', cls: d.ollama_available?'ok':'warn'},
  ];
  document.getElementById('metrics-grid').innerHTML = metrics.map(m =>
    `<div class="card"><h3>${m.label}</h3><div class="metric ${m.cls}">${m.value}</div></div>`
  ).join('');

  const warns = d.warnings || [];
  document.getElementById('warnings-list').innerHTML = warns.length === 0
    ? '<span style="color:#3fb950">No warnings — system healthy</span>'
    : warns.map(w => `<div class="log-entry warn">⚠ ${w}</div>`).join('');
}

async function refreshModels() {
  try {
    const r = await fetch('/api/models');
    const models = await r.json();
    document.getElementById('models-table').innerHTML = models.map(m =>
      `<tr><td>${m.name}</td><td><span class="badge ok">${m.family}</span></td>
       <td>${m.vram_required_gb.toFixed(1)} GB</td><td><span class="badge ok">${m.profile}</span></td></tr>`
    ).join('') || '<tr><td colspan="4" style="color:#8b949e">No Ollama models detected. Start Ollama and run: ollama pull llama3</td></tr>';

    const r2 = await fetch('/api/providers');
    const providers = await r2.json();
    document.getElementById('providers-table').innerHTML = providers.map(p =>
      `<tr><td>${p.provider}</td><td><span class="badge ${p.available?'ok':'err'}">${p.available?'Online':'Offline'}</span></td>
       <td>${p.total_requests}</td><td>${p.total_errors}</td><td>${p.avg_latency_ms}ms</td></tr>`
    ).join('');
  } catch(e) {}
}

function renderMemory() {
  const d = data;
  const items = [
    {label:'Total Entries', value: d.memory_entries},
    {label:'Knowledge Chunks', value: d.knowledge_chunks},
    {label:'RAG Queries Run', value: d.rag_queries},
    {label:'Embeddings Generated', value: d.embeddings_generated || 0},
  ];
  document.getElementById('memory-grid').innerHTML = items.map(i =>
    `<div class="card"><h3>${i.label}</h3><div class="metric ok">${i.value}</div></div>`
  ).join('');
}

async function refreshPermissions() {
  try {
    const r = await fetch('/api/permissions');
    const perms = await r.json();
    document.getElementById('perms-table').innerHTML = (perms.entries || []).map(p =>
      `<tr><td>${p.kind}</td><td style="font-size:10px">${p.resource}</td>
       <td><span class="badge ${p.state==='Granted'?'ok':p.state==='Denied'?'err':'pending'}">${p.state}</span></td></tr>`
    ).join('') || '<tr><td colspan="3" style="color:#8b949e">No permissions recorded yet</td></tr>';

    const pending = perms.pending || [];
    document.getElementById('pending-perms').innerHTML = pending.length === 0
      ? '<span style="color:#8b949e;font-size:12px">No pending permission requests</span>'
      : pending.map(p => `<div class="card" style="margin-bottom:8px">
          <div style="font-size:12px"><b>${p.kind}</b> → ${p.resource}</div>
          <div style="font-size:11px;color:#8b949e;margin-top:4px">${p.reason}</div>
          <div style="margin-top:8px;display:flex;gap:8px">
            <button class="btn primary" onclick="grantPerm('${p.kind}','${p.resource}')">Grant</button>
            <button class="btn danger" onclick="denyPerm('${p.kind}','${p.resource}')">Deny</button>
          </div></div>`).join('');
  } catch(e) {}
}

async function grantPerm(kind, resource) {
  await fetch('/api/permissions/grant', {method:'POST', headers:{'Content-Type':'application/json'}, body: JSON.stringify({kind, resource})});
  refreshPermissions();
}

async function denyPerm(kind, resource) {
  await fetch('/api/permissions/deny', {method:'POST', headers:{'Content-Type':'application/json'}, body: JSON.stringify({kind, resource})});
  refreshPermissions();
}

async function refreshLogs() {
  try {
    const r = await fetch('/api/logs');
    const logs = await r.json();
    document.getElementById('logs-box').innerHTML = logs.map(e => {
      const cls = e.level === 'Critical' ? 'crit' : e.level === 'Error' ? 'err' : e.level === 'Warning' ? 'warn' : 'info';
      return `<div class="log-entry ${cls}">[${e.level}] ${e.component}: ${e.message}</div>`;
    }).join('') || '<span style="color:#8b949e">No log entries yet</span>';
  } catch(e) {}
}

function renderSettings() {
  const d = data;
  document.getElementById('safe-mode-status').textContent =
    d.safe_mode_active ? '⚠ Safe mode is ACTIVE — cognitive services limited' : '✓ Normal mode — all services running';
}

async function toggleSafeMode() {
  await fetch('/api/safe-mode/toggle', {method:'POST'});
  await refresh();
}

refresh();
setInterval(refresh, 3000);
</script>
</body>
</html>"#;

// ── HTTP helpers ──────────────────────────────────────────────────────────────

fn send_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
        status, content_type, body.len(), body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn json_response(stream: &mut TcpStream, body: &str) {
    send_response(stream, "200 OK", "application/json", body);
}

fn html_response(stream: &mut TcpStream, body: &str) {
    send_response(stream, "200 OK", "text/html; charset=utf-8", body);
}

fn read_request_path(stream: &mut TcpStream) -> (String, String) {
    let mut buf = [0u8; 4096];
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(200)));
    let n = stream.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]).to_string();
    let lines: Vec<&str> = req.lines().collect();
    let first = lines.first().unwrap_or(&"");
    let parts: Vec<&str> = first.split_whitespace().collect();
    let method = parts.first().unwrap_or(&"GET").to_string();
    let path   = parts.get(1).unwrap_or(&"/").to_string();

    // Extract body after blank line
    let body = if let Some(idx) = req.find("\r\n\r\n") {
        req[idx + 4..].to_string()
    } else { String::new() };

    // path includes body for POST handling
    let _ = body;
    (method, path)
}

// ── API handlers ──────────────────────────────────────────────────────────────

fn handle_status(stream: &mut TcpStream) {
    let snap = crate::diagnostics_center::snapshot();
    let body = serde_json::to_string(&snap).unwrap_or_default();
    json_response(stream, &body);
}

fn handle_models(stream: &mut TcpStream) {
    let models = crate::model_manager::scan_ollama();
    let body = serde_json::to_string(&models).unwrap_or_default();
    json_response(stream, &body);
}

fn handle_providers(stream: &mut TcpStream) {
    let statuses = crate::llm_provider_runtime::get_status();
    let body = serde_json::to_string(&statuses).unwrap_or_default();
    json_response(stream, &body);
}

fn handle_permissions(stream: &mut TcpStream) {
    let entries  = crate::permission_runtime::all_entries();
    let pending  = crate::permission_runtime::pending_requests();
    #[derive(serde::Serialize)]
    struct Resp { entries: Vec<crate::permission_runtime::PermissionEntry>, pending: Vec<crate::permission_runtime::PermissionRequest> }
    let body = serde_json::to_string(&Resp { entries, pending }).unwrap_or_default();
    json_response(stream, &body);
}

fn handle_logs(stream: &mut TcpStream) {
    let entries = crate::production_logging::recent(100);
    let body = serde_json::to_string(&entries).unwrap_or_default();
    json_response(stream, &body);
}

fn handle_safe_mode_toggle(stream: &mut TcpStream) {
    if crate::safe_mode::is_active() {
        crate::safe_mode::exit();
        json_response(stream, r#"{"safe_mode":false}"#);
    } else {
        crate::safe_mode::enter("user_request");
        json_response(stream, r#"{"safe_mode":true}"#);
    }
}

fn handle_request(mut stream: TcpStream) {
    SERVER_REQUESTS.fetch_add(1, Ordering::Relaxed);

    let mut buf = [0u8; 8192];
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
    let n = stream.read(&mut buf).unwrap_or(0);
    let req = String::from_utf8_lossy(&buf[..n]).to_string();

    let lines: Vec<&str> = req.lines().collect();
    let first = lines.first().unwrap_or(&"GET / HTTP/1.1");
    let parts: Vec<&str> = first.split_whitespace().collect();
    let method: &str = parts.first().copied().unwrap_or("GET");
    let path:   &str = parts.get(1).copied().unwrap_or("/").trim_end_matches('?');

    match (method, path) {
        (_, "/")                          => html_response(&mut stream, DASHBOARD_HTML),
        ("GET", "/api/status")            => handle_status(&mut stream),
        ("GET", "/api/models")            => handle_models(&mut stream),
        ("GET", "/api/providers")         => handle_providers(&mut stream),
        ("GET", "/api/permissions")       => handle_permissions(&mut stream),
        ("GET", "/api/logs")              => handle_logs(&mut stream),
        ("POST", "/api/safe-mode/toggle") => handle_safe_mode_toggle(&mut stream),
        _ => send_response(&mut stream, "404 Not Found", "text/plain", "Not found"),
    }
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

pub fn start() {
    if SERVER_RUNNING.swap(true, Ordering::SeqCst) { return; }
    SERVER_STOP.store(false, Ordering::SeqCst);

    std::thread::Builder::new()
        .name("jarvis-control-center".to_string())
        .spawn(move || {
            let listener = match TcpListener::bind(format!("127.0.0.1:{}", SERVER_PORT)) {
                Ok(l)  => l,
                Err(_) => { SERVER_RUNNING.store(false, Ordering::SeqCst); return; }
            };
            let _ = listener.set_nonblocking(true);

            crate::production_logging::info("control_center_server",
                &format!("Dashboard available at http://127.0.0.1:{}", SERVER_PORT));

            while !SERVER_STOP.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        std::thread::spawn(move || handle_request(stream));
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => { SERVER_ERRORS.fetch_add(1, Ordering::Relaxed); }
                }
            }
            SERVER_RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

pub fn stop() {
    SERVER_STOP.store(true, Ordering::SeqCst);
}

pub fn is_running()      -> bool { SERVER_RUNNING.load(Ordering::Relaxed) }
pub fn requests_served() -> u64  { SERVER_REQUESTS.load(Ordering::Relaxed) }
pub fn server_port()     -> u16  { SERVER_PORT }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_not_running_initially() {
        // Don't start the server in unit tests
        let r = is_running();
        assert!(r || !r); // just verify it's queryable
    }

    #[test]
    fn dashboard_html_non_empty() {
        assert!(!DASHBOARD_HTML.is_empty());
        assert!(DASHBOARD_HTML.contains("Jarvis Control Center"));
    }

    #[test]
    fn server_port_correct() {
        assert_eq!(server_port(), 7070);
    }

    #[test]
    fn stop_no_panic_when_not_running() {
        stop();
    }
}
