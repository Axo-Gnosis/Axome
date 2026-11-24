use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axo::SessionTools;
use axo_agent::{ExecutionMode, Replay, Tool, ToolContext, ToolError, ToolOutput};
use axo_ai::{ToolCall, UserBlock};
use base64::Engine as _;
use playwright_rs::protocol::Page;
use playwright_rs::{GotoOptions, ScreenshotOptions};
use serde::Deserialize;
use serde_json::{json, Value};
use url::Url;

use crate::policy::validate_navigation_url;
use crate::service::{BrowserConfig, BrowserService, ManagedTabInfo};
use crate::tab::{
    cap_or_spill_text, capture_aria_snapshot, parse_wait_until, resolve_locator, save_spill_target,
    settle_after_action, ConsoleEntry, DialogEntry, NetworkEntry, TelemetryDelta,
};

const MAX_BATCH_STEPS: usize = 12;
const DEFAULT_TAB: &str = "main";

const GUIDELINES: &[Cow<'static, str>] = &[
    Cow::Borrowed("Use `browser` for interactive web pages, localhost UI verification, console/network errors, JS evaluation, and screenshots. For static docs/URLs, prefer `web_fetch`."),
    Cow::Borrowed("Batch sequential operations in `steps` (e.g. `open` + `snapshot`, or `click` + `snapshot`) to save turns."),
    Cow::Borrowed("Prefer `snapshot` (AI ARIA tree with `@e1` refs) before acting; use `screenshot` when visual layout confirmation is needed."),
    Cow::Borrowed("New console errors, failed network requests, and auto-handled JS dialogs are appended automatically after each call."),
];

pub struct BrowserTool {
    service: Arc<BrowserService>,
    session_tools: Arc<SessionTools>,
    session_key: String,
    fallback_spill_dir: PathBuf,
    config: BrowserConfig,
    parameters: Value,
}

impl BrowserTool {
    pub fn new(
        service: Arc<BrowserService>,
        session_tools: Arc<SessionTools>,
        session_key: String,
        config: BrowserConfig,
    ) -> Self {
        let fallback_spill_dir = config.spill_dir.join(&session_key);
        Self {
            service,
            session_tools,
            session_key,
            fallback_spill_dir,
            config,
            parameters: build_parameters_schema(),
        }
    }
}

fn step_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": [
                    "open", "tabs", "close", "goto", "back", "forward", "reload",
                    "snapshot", "text", "screenshot", "logs", "network",
                    "click", "dblclick", "hover", "fill", "type", "press",
                    "select", "check", "uncheck", "scroll", "click_at", "upload_file",
                    "wait_for", "eval"
                ]
            },
            "tab": { "type": "string", "description": "Named tab (defaults to 'main')." },
            "url": { "type": "string", "description": "http:// or https:// URL for open/goto/wait_for." },
            "headed": { "type": "boolean", "description": "Open a visible browser window instead of headless." },
            "persist": { "type": "boolean", "description": "Keep tab open after session ends." },
            "cdp_url": { "type": "string", "description": "Optional CDP WebSocket/HTTP endpoint to attach to." },
            "wait_until": { "type": "string", "enum": ["domcontentloaded", "load", "networkidle", "commit"] },
            "all": { "type": "boolean", "description": "For close: close all non-persistent tabs in this session." },
            "target": { "type": "string", "description": "ARIA ref from snapshot (e.g. '@e5') or Playwright selector (CSS, text=..., role=...)." },
            "selector": { "type": "string", "description": "Optional selector to scope snapshot/text/screenshot/wait_for." },
            "depth": { "type": "integer", "minimum": 1, "maximum": 32, "description": "Optional max depth for snapshot." },
            "boxes": { "type": "boolean", "description": "Include [box=x,y,w,h] bounding boxes in snapshot." },
            "full_page": { "type": "boolean", "description": "Capture full scrollable page for screenshot." },
            "text": { "type": "string", "description": "Text for fill/type/wait_for." },
            "key": { "type": "string", "description": "Key name for press (e.g. 'Enter', 'Escape', 'Control+a')." },
            "values": { "type": "array", "items": { "type": "string" }, "description": "Option values for select." },
            "dx": { "type": "number", "description": "Horizontal scroll delta in pixels." },
            "dy": { "type": "number", "description": "Vertical scroll delta in pixels." },
            "x": { "type": "number", "description": "Viewport X coordinate for click_at." },
            "y": { "type": "number", "description": "Viewport Y coordinate for click_at." },
            "paths": { "type": "array", "items": { "type": "string" }, "description": "Local file paths for upload_file." },
            "timeout_ms": { "type": "integer", "minimum": 100, "maximum": 60000 },
            "expression": { "type": "string", "description": "JavaScript expression to evaluate in the page." },
            "clear": { "type": "boolean", "description": "Clear logs/network buffer after reading." },
            "failed_only": { "type": "boolean", "description": "Only return 4xx/5xx and failed network requests." }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

fn build_parameters_schema() -> Value {
    let mut base = step_schema();
    if let Some(props) = base.get_mut("properties").and_then(Value::as_object_mut) {
        props.insert(
            "steps".to_owned(),
            json!({
                "type": "array",
                "items": step_schema(),
                "minItems": 1,
                "maxItems": MAX_BATCH_STEPS,
                "description": "Optional ordered batch of browser actions to execute in one turn."
            }),
        );
    }
    if let Some(obj) = base.as_object_mut() {
        obj.remove("required");
    }
    base
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserStep {
    action: String,
    tab: Option<String>,
    url: Option<String>,
    headed: Option<bool>,
    persist: Option<bool>,
    cdp_url: Option<String>,
    wait_until: Option<String>,
    all: Option<bool>,
    target: Option<String>,
    selector: Option<String>,
    depth: Option<i32>,
    boxes: Option<bool>,
    full_page: Option<bool>,
    text: Option<String>,
    key: Option<String>,
    values: Option<Vec<String>>,
    dx: Option<f64>,
    dy: Option<f64>,
    x: Option<f64>,
    y: Option<f64>,
    paths: Option<Vec<String>>,
    timeout_ms: Option<u64>,
    expression: Option<String>,
    clear: Option<bool>,
    failed_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserBatchArgs {
    steps: Vec<BrowserStep>,
}

fn parse_steps(args: Value) -> Result<Vec<BrowserStep>, String> {
    let obj = args
        .as_object()
        .ok_or_else(|| "Browser arguments must be a JSON object".to_owned())?;
    if obj.contains_key("steps") {
        if obj.contains_key("action") {
            return Err("Provide either `action` or `steps`, not both".to_owned());
        }
        let batch: BrowserBatchArgs = serde_json::from_value(args)
            .map_err(|err| format!("Invalid browser arguments: {err}"))?;
        if batch.steps.is_empty() {
            return Err("`steps` must contain at least one action".to_owned());
        }
        if batch.steps.len() > MAX_BATCH_STEPS {
            return Err(format!("`steps` exceeds maximum of {MAX_BATCH_STEPS}"));
        }
        return Ok(batch.steps);
    }
    if obj.contains_key("action") {
        let step: BrowserStep = serde_json::from_value(args)
            .map_err(|err| format!("Invalid browser arguments: {err}"))?;
        return Ok(vec![step]);
    }
    Err("Provide either `action` or `steps`".to_owned())
}

impl Drop for BrowserTool {
    fn drop(&mut self) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let service = Arc::clone(&self.service);
            let session_key = self.session_key.clone();
            let fallback_dir = self.fallback_spill_dir.clone();
            handle.spawn(async move {
                service
                    .release_tool_handle(&session_key, &fallback_dir)
                    .await;
            });
        }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Drive a Chromium-based browser via Playwright: open pages, inspect AI ARIA snapshots (@e1 refs), click/fill/type/eval, capture screenshots, and read console/network errors."
    }

    fn parameters(&self) -> &Value {
        &self.parameters
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Sequential
    }

    fn replay(&self, _call: &ToolCall) -> Replay {
        Replay::Never
    }

    fn prompt_guidelines(&self) -> &[Cow<'static, str>] {
        GUIDELINES
    }

    async fn execute(&self, cx: ToolContext) -> Result<ToolOutput, ToolError> {
        self.service.reap_idle(self.config.idle_close_secs).await;

        let steps = parse_steps(Value::Object(cx.call.arguments.clone())).map_err(ToolError)?;

        let run = self.run_steps(steps);
        tokio::select! {
            biased;
            _ = cx.cancel.cancelled() => Err(ToolError("Browser operation cancelled".to_owned())),
            res = run => res.map_err(ToolError),
        }
    }
}

struct StepOutcome {
    action: String,
    tab: String,
    text: String,
    image: Option<(String, Vec<u8>)>,
    url: Option<String>,
    title: Option<String>,
    spilled_path: Option<String>,
    delta: TelemetryDelta,
}

impl BrowserTool {
    async fn run_steps(&self, steps: Vec<BrowserStep>) -> Result<ToolOutput, String> {
        let mut text_parts = Vec::new();
        let mut content: Vec<UserBlock> = Vec::new();
        let mut step_summaries = Vec::new();
        let mut last_tab = DEFAULT_TAB.to_owned();
        let mut last_url: Option<String> = None;
        let mut last_title: Option<String> = None;
        let mut spilled_path: Option<String> = None;
        let mut all_console_errors = Vec::new();
        let mut all_failed_requests = Vec::new();
        let mut all_dialogs = Vec::new();
        let mut has_screenshot = false;

        let total = steps.len();
        for (idx, step) in steps.into_iter().enumerate() {
            let action = step.action.clone();
            let need_title = idx + 1 == total;
            let outcome = match self.run_single(step, need_title).await {
                Ok(outcome) => outcome,
                Err(err) => {
                    let mut msg = if total > 1 {
                        format!("Step {}/{total} ({action}) failed: {err}", idx + 1)
                    } else {
                        err
                    };
                    append_telemetry_sections(
                        &mut msg,
                        &all_dialogs,
                        &all_console_errors,
                        &all_failed_requests,
                    );
                    return Err(msg);
                }
            };

            last_tab = outcome.tab.clone();
            if outcome.url.is_some() {
                last_url = outcome.url.clone();
            }
            if outcome.title.is_some() {
                last_title = outcome.title.clone();
            }
            if let Some(p) = outcome.spilled_path {
                spilled_path = Some(p);
            }
            all_console_errors.extend(outcome.delta.console_errors);
            all_failed_requests.extend(outcome.delta.failed_requests);
            all_dialogs.extend(outcome.delta.dialogs);

            step_summaries.push(outcome.action);
            if !outcome.text.is_empty() {
                text_parts.push(outcome.text);
            }
            if let Some((mime_type, bytes)) = outcome.image {
                has_screenshot = true;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                content.push(UserBlock::Image {
                    data: Arc::from(encoded),
                    mime_type,
                });
            }
        }

        let mut body = text_parts.join("\n\n");
        append_telemetry_sections(
            &mut body,
            &all_dialogs,
            &all_console_errors,
            &all_failed_requests,
        );

        content.insert(0, UserBlock::Text { text: body });

        let details = json!({
            "action": step_summaries.first().cloned().unwrap_or_default(),
            "steps": step_summaries,
            "tab": last_tab,
            "url": last_url,
            "title": last_title,
            "consoleErrors": all_console_errors,
            "failedRequests": all_failed_requests,
            "dialogs": all_dialogs,
            "spilledPath": spilled_path,
            "hasScreenshot": has_screenshot,
            "guidelines": GUIDELINES,
        });

        Ok(ToolOutput {
            content,
            details: Some(details),
            is_error: false,
            ..ToolOutput::default()
        })
    }

    async fn run_single(&self, step: BrowserStep, need_title: bool) -> Result<StepOutcome, String> {
        let tab_name = step.tab.clone().unwrap_or_else(|| DEFAULT_TAB.to_owned());
        let action = step.action.clone();

        match action.as_str() {
            "open" => {
                let parsed = step
                    .url
                    .as_deref()
                    .map(validate_navigation_url)
                    .transpose()?;
                let handle = self
                    .service
                    .open_or_get_tab(
                        &self.session_key,
                        &tab_name,
                        &self.config,
                        step.headed,
                        step.cdp_url.as_deref(),
                        step.persist,
                    )
                    .await?;
                let mut session = handle.lock().await;
                let tab = session
                    .tabs
                    .get_mut(&tab_name)
                    .ok_or_else(|| format!("Tab '{tab_name}' not found"))?;
                tab.touch();
                if let Some(ref url) = parsed {
                    navigate_page(&tab.page, url, step.wait_until.as_deref())
                        .await
                        .map_err(|err| format!("Failed to open '{url}': {err}"))?;
                }
                let landed = tab.page.url();
                let title = if need_title {
                    tab.page.title().await.unwrap_or_default()
                } else {
                    String::new()
                };
                let delta = tab.drain_delta();
                Ok(StepOutcome {
                    action,
                    tab: tab_name.clone(),
                    text: if title.is_empty() {
                        format!("Opened tab '{tab_name}': {landed}")
                    } else {
                        format!("Opened tab '{tab_name}': {landed} ({title})")
                    },
                    image: None,
                    url: Some(landed),
                    title: need_title.then_some(title),
                    spilled_path: None,
                    delta,
                })
            }

            "tabs" => {
                let handle = self.service.session_handle(&self.session_key).await;
                let session = handle.lock().await;
                let mut infos = Vec::new();
                for (name, tab) in &session.tabs {
                    infos.push(ManagedTabInfo {
                        name: name.clone(),
                        url: tab.page.url(),
                        title: tab.page.title().await.unwrap_or_default(),
                        persist: tab.persist,
                    });
                }
                let text = if infos.is_empty() {
                    "No open browser tabs.".to_owned()
                } else {
                    infos
                        .iter()
                        .map(|t| {
                            format!(
                                "- {}: {} ({}){}",
                                t.name,
                                t.url,
                                t.title,
                                if t.persist { " [persist]" } else { "" }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                Ok(StepOutcome {
                    action,
                    tab: tab_name,
                    text,
                    image: None,
                    url: None,
                    title: None,
                    spilled_path: None,
                    delta: TelemetryDelta::default(),
                })
            }

            "close" => {
                if step.all.unwrap_or(false) {
                    self.service.close_session(&self.session_key, false).await;
                    return Ok(StepOutcome {
                        action,
                        tab: tab_name,
                        text: "Closed all non-persistent browser tabs for this session.".to_owned(),
                        image: None,
                        url: None,
                        title: None,
                        spilled_path: None,
                        delta: TelemetryDelta::default(),
                    });
                }
                let handle = self.service.session_handle(&self.session_key).await;
                let mut session = handle.lock().await;
                if let Some(tab) = session.tabs.remove(&tab_name) {
                    let _ = tab.page.close().await;
                    Ok(StepOutcome {
                        action,
                        tab: tab_name.clone(),
                        text: format!("Closed tab '{tab_name}'."),
                        image: None,
                        url: None,
                        title: None,
                        spilled_path: None,
                        delta: TelemetryDelta::default(),
                    })
                } else {
                    Err(format!("No open tab named '{tab_name}'"))
                }
            }

            _ => self.run_tab_action(step, tab_name, need_title).await,
        }
    }

    async fn run_tab_action(
        &self,
        step: BrowserStep,
        tab_name: String,
        need_title: bool,
    ) -> Result<StepOutcome, String> {
        let handle = self.service.session_handle(&self.session_key).await;
        let mut session = handle.lock().await;
        let tab = session
            .tabs
            .get_mut(&tab_name)
            .ok_or_else(|| format!("No open tab named '{tab_name}'. Call action 'open' first."))?;
        tab.touch();

        let action = step.action.clone();
        let mut image: Option<(String, Vec<u8>)> = None;
        let mut spilled_path: Option<String> = None;

        let res: Result<String, String> = async {
            let text = match action.as_str() {
                "goto" => {
                    let raw = step
                        .url
                        .as_deref()
                        .ok_or_else(|| "goto requires `url`".to_owned())?;
                    let url = validate_navigation_url(raw)
                        .map_err(|err| format!("goto failed: {err}"))?;
                    navigate_page(&tab.page, &url, step.wait_until.as_deref())
                        .await
                        .map_err(|err| format!("goto failed: {err}"))?;
                    format!("Navigated to {}", tab.page.url())
                }

                "back" => {
                    tab.page
                        .go_back(None)
                        .await
                        .map_err(|err| format!("back failed: {err}"))?;
                    format!("Navigated back to {}", tab.page.url())
                }

                "forward" => {
                    tab.page
                        .go_forward(None)
                        .await
                        .map_err(|err| format!("forward failed: {err}"))?;
                    format!("Navigated forward to {}", tab.page.url())
                }

                "reload" => {
                    tab.page
                        .reload(None)
                        .await
                        .map_err(|err| format!("reload failed: {err}"))?;
                    format!("Reloaded {}", tab.page.url())
                }

                "snapshot" => {
                    let depth = step.depth.map(|d| d.clamp(1, 32));
                    let raw = capture_aria_snapshot(
                        &tab.page,
                        step.selector.as_deref(),
                        depth,
                        step.boxes.unwrap_or(false),
                    )
                    .await?;
                    let (bounded, spilled) = cap_or_spill_text(
                        &raw,
                        Some(self.session_tools.as_ref()),
                        &self.fallback_spill_dir,
                        "snapshot",
                    )
                    .await;
                    spilled_path = spilled;
                    bounded
                }

                "text" => {
                    let sel = clean_opt(step.selector.as_deref()).unwrap_or("body");
                    let loc = resolve_locator(&tab.page, sel);
                    let raw = loc
                        .inner_text()
                        .await
                        .map_err(|err| format!("text('{sel}') failed: {err}"))?;
                    let (bounded, spilled) = cap_or_spill_text(
                        &raw,
                        Some(self.session_tools.as_ref()),
                        &self.fallback_spill_dir,
                        "text",
                    )
                    .await;
                    spilled_path = spilled;
                    bounded
                }

                "screenshot" => {
                    let opts = ScreenshotOptions::builder()
                        .full_page(step.full_page.unwrap_or(false))
                        .build();
                    let target_sel = clean_opt(step.selector.as_deref())
                        .or_else(|| clean_opt(step.target.as_deref()));
                    let bytes = match target_sel {
                        Some(sel) => {
                            let loc = resolve_locator(&tab.page, sel);
                            loc.screenshot(Some(opts))
                                .await
                                .map_err(|err| format!("screenshot('{sel}') failed: {err}"))?
                        }
                        None => tab
                            .page
                            .screenshot(Some(opts))
                            .await
                            .map_err(|err| format!("screenshot failed: {err}"))?,
                    };
                    let saved = save_spill_target(
                        Some(self.session_tools.as_ref()),
                        &self.fallback_spill_dir,
                        "screenshot",
                        "png",
                        &bytes,
                    )
                    .await;
                    let note = match &saved {
                        Some(p) => {
                            format!("Captured screenshot ({} bytes, saved to {p})", bytes.len())
                        }
                        None => format!("Captured screenshot ({} bytes)", bytes.len()),
                    };
                    spilled_path = saved;
                    image = Some(("image/png".to_owned(), bytes));
                    note
                }

                "click" => {
                    let (target, loc) = require_target_locator(&tab.page, &step, "click")?;
                    loc.click(None)
                        .await
                        .map_err(|err| format!("click('{target}') failed: {err}"))?;
                    settle_after_action(&tab.page).await;
                    format!("Clicked {target}")
                }

                "dblclick" => {
                    let (target, loc) = require_target_locator(&tab.page, &step, "dblclick")?;
                    loc.dblclick(None)
                        .await
                        .map_err(|err| format!("dblclick('{target}') failed: {err}"))?;
                    settle_after_action(&tab.page).await;
                    format!("Double-clicked {target}")
                }

                "hover" => {
                    let (target, loc) = require_target_locator(&tab.page, &step, "hover")?;
                    loc.hover(None)
                        .await
                        .map_err(|err| format!("hover('{target}') failed: {err}"))?;
                    format!("Hovered {target}")
                }

                "fill" => {
                    let (target, loc) = require_target_locator(&tab.page, &step, "fill")?;
                    let text = step
                        .text
                        .as_deref()
                        .ok_or_else(|| "fill requires `text`".to_owned())?;
                    loc.fill(text, None)
                        .await
                        .map_err(|err| format!("fill('{target}') failed: {err}"))?;
                    format!("Filled {target}")
                }

                "type" => {
                    let (target, loc) = require_target_locator(&tab.page, &step, "type")?;
                    let text = step
                        .text
                        .as_deref()
                        .ok_or_else(|| "type requires `text`".to_owned())?;
                    loc.press_sequentially(text, None)
                        .await
                        .map_err(|err| format!("type('{target}') failed: {err}"))?;
                    format!("Typed into {target}")
                }

                "press" => {
                    let key = step
                        .key
                        .as_deref()
                        .ok_or_else(|| "press requires `key`".to_owned())?;
                    if let Some(target) = clean_opt(step.target.as_deref()) {
                        let loc = resolve_locator(&tab.page, target);
                        loc.press(key, None)
                            .await
                            .map_err(|err| format!("press('{key}' on '{target}') failed: {err}"))?;
                    } else {
                        tab.page
                            .keyboard()
                            .press(key, None)
                            .await
                            .map_err(|err| format!("press('{key}') failed: {err}"))?;
                    }
                    settle_after_action(&tab.page).await;
                    format!("Pressed {key}")
                }

                "select" => {
                    let (target, loc) = require_target_locator(&tab.page, &step, "select")?;
                    let values = step
                        .values
                        .as_deref()
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| "select requires non-empty `values`".to_owned())?;
                    if values.len() == 1 {
                        loc.select_option(values[0].as_str(), None)
                            .await
                            .map_err(|err| format!("select('{target}') failed: {err}"))?;
                    } else {
                        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
                        loc.select_option_multiple(&refs, None)
                            .await
                            .map_err(|err| format!("select('{target}') failed: {err}"))?;
                    }
                    format!("Selected {:?} on {target}", values)
                }

                "check" => {
                    let (target, loc) = require_target_locator(&tab.page, &step, "check")?;
                    loc.check(None)
                        .await
                        .map_err(|err| format!("check('{target}') failed: {err}"))?;
                    format!("Checked {target}")
                }

                "uncheck" => {
                    let (target, loc) = require_target_locator(&tab.page, &step, "uncheck")?;
                    loc.uncheck(None)
                        .await
                        .map_err(|err| format!("uncheck('{target}') failed: {err}"))?;
                    format!("Unchecked {target}")
                }

                "scroll" => {
                    let dx = step.dx.unwrap_or(0.0);
                    let dy = step.dy.unwrap_or(500.0);
                    tab.page
                        .mouse()
                        .wheel(dx, dy)
                        .await
                        .map_err(|err| format!("scroll failed: {err}"))?;
                    format!("Scrolled ({dx}, {dy})")
                }

                "click_at" => {
                    let x = step.x.ok_or_else(|| "click_at requires `x`".to_owned())?;
                    let y = step.y.ok_or_else(|| "click_at requires `y`".to_owned())?;
                    tab.page
                        .mouse()
                        .click(x, y, None)
                        .await
                        .map_err(|err| format!("click_at({x}, {y}) failed: {err}"))?;
                    settle_after_action(&tab.page).await;
                    format!("Clicked at ({x}, {y})")
                }

                "upload_file" => {
                    let (target, loc) = require_target_locator(&tab.page, &step, "upload_file")?;
                    let paths = step
                        .paths
                        .as_deref()
                        .filter(|p| !p.is_empty())
                        .ok_or_else(|| "upload_file requires non-empty `paths`".to_owned())?;
                    if paths.len() == 1 {
                        let path = PathBuf::from(&paths[0]);
                        loc.set_input_files(&path, None)
                            .await
                            .map_err(|err| format!("upload_file failed: {err}"))?;
                    } else {
                        let bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
                        let refs: Vec<&PathBuf> = bufs.iter().collect();
                        loc.set_input_files_multiple(&refs, None)
                            .await
                            .map_err(|err| format!("upload_file failed: {err}"))?;
                    }
                    format!("Uploaded {} file(s) to {target}", paths.len())
                }

                "wait_for" => {
                    let timeout = Duration::from_millis(
                        step.timeout_ms
                            .map(|t| t.clamp(100, 60_000))
                            .unwrap_or(10_000),
                    );
                    if let Some(sel) = clean_opt(step.selector.as_deref())
                        .or_else(|| clean_opt(step.target.as_deref()))
                    {
                        let loc = resolve_locator(&tab.page, sel);
                        tokio::time::timeout(timeout, loc.wait_for(None))
                            .await
                            .map_err(|_| format!("Timed out waiting for selector '{sel}'"))?
                            .map_err(|err| format!("wait_for('{sel}') failed: {err}"))?;
                        format!("Selector '{sel}' is ready")
                    } else if let Some(expected_text) = clean_opt(step.text.as_deref()) {
                        let loc = tab.page.locator(format!("text={expected_text}")).first();
                        tokio::time::timeout(timeout, loc.wait_for(None))
                            .await
                            .map_err(|_| format!("Timed out waiting for text '{expected_text}'"))?
                            .map_err(|err| format!("wait_for text failed: {err}"))?;
                        format!("Text '{expected_text}' appeared")
                    } else if let Some(url) = clean_opt(step.url.as_deref()) {
                        tokio::time::timeout(timeout, tab.page.wait_for_url(url, None))
                            .await
                            .map_err(|_| format!("Timed out waiting for URL '{url}'"))?
                            .map_err(|err| format!("wait_for url failed: {err}"))?;
                        format!("URL matched '{url}'")
                    } else {
                        return Err("wait_for requires `selector`, `text`, or `url`".to_owned());
                    }
                }

                "eval" => {
                    let expr = step
                        .expression
                        .as_deref()
                        .ok_or_else(|| "eval requires `expression`".to_owned())?;
                    let val: Value = tab
                        .page
                        .evaluate::<(), Value>(expr, None)
                        .await
                        .map_err(|err| format!("eval failed: {err}"))?;
                    let rendered =
                        serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string());
                    let (bounded, spilled) = cap_or_spill_text(
                        &rendered,
                        Some(self.session_tools.as_ref()),
                        &self.fallback_spill_dir,
                        "eval",
                    )
                    .await;
                    spilled_path = spilled;
                    bounded
                }

                "logs" => {
                    let entries = tab
                        .telemetry
                        .lock()
                        .all_console(step.clear.unwrap_or(false));
                    if entries.is_empty() {
                        "No console logs recorded.".to_owned()
                    } else {
                        entries
                            .iter()
                            .map(|e| format!("[{}] {}", e.level, e.text))
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                }

                "network" => {
                    let entries = tab.telemetry.lock().all_network(
                        step.failed_only.unwrap_or(false),
                        step.clear.unwrap_or(false),
                    );
                    if entries.is_empty() {
                        "No matching network requests recorded.".to_owned()
                    } else {
                        entries
                            .iter()
                            .map(|r| format_network_line(r, "-"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                }

                other => return Err(format!("Unknown browser action '{other}'")),
            };
            Ok(text)
        }
        .await;

        let delta = tab.drain_delta();
        let text = match res {
            Ok(text) => text,
            Err(mut err) => {
                append_telemetry_sections(
                    &mut err,
                    &delta.dialogs,
                    &delta.console_errors,
                    &delta.failed_requests,
                );
                return Err(err);
            }
        };

        let url = tab.page.url();
        let title = if need_title {
            Some(tab.page.title().await.unwrap_or_default())
        } else {
            None
        };

        Ok(StepOutcome {
            action,
            tab: tab_name,
            text,
            image,
            url: Some(url),
            title,
            spilled_path,
            delta,
        })
    }
}

fn clean_opt(opt: Option<&str>) -> Option<&str> {
    opt.map(str::trim).filter(|s| !s.is_empty())
}

fn require_target_locator<'a>(
    page: &Page,
    step: &'a BrowserStep,
    action: &str,
) -> Result<(&'a str, playwright_rs::protocol::Locator), String> {
    let target = step
        .target
        .as_deref()
        .ok_or_else(|| format!("{action} requires `target`"))?;
    Ok((target, resolve_locator(page, target)))
}

async fn navigate_page(page: &Page, url: &Url, wait_until: Option<&str>) -> Result<(), String> {
    let opts = GotoOptions::new().wait_until(parse_wait_until(wait_until));
    page.goto(url.as_str(), Some(opts))
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn format_network_line(entry: &NetworkEntry, fallback: &str) -> String {
    let code = entry
        .status
        .map(|s| s.to_string())
        .or_else(|| entry.failure.clone())
        .unwrap_or_else(|| fallback.to_owned());
    if entry.method.is_empty() {
        format!("[{code}] {}", entry.url)
    } else {
        format!("[{code} {}] {}", entry.method, entry.url)
    }
}

fn append_telemetry_sections(
    out: &mut String,
    dialogs: &[DialogEntry],
    console_errors: &[ConsoleEntry],
    failed_requests: &[NetworkEntry],
) {
    if !dialogs.is_empty() {
        out.push_str("\n\nDialogs:");
        for d in dialogs {
            out.push_str(&format!("\n- [{} {}] {}", d.kind, d.action, d.message));
        }
    }
    if !console_errors.is_empty() {
        out.push_str("\n\nConsole errors:");
        for e in console_errors {
            out.push_str(&format!("\n- [{}] {}", e.level, e.text));
        }
    }
    if !failed_requests.is_empty() {
        out.push_str("\n\nFailed requests:");
        for r in failed_requests {
            out.push_str(&format!("\n- {}", format_network_line(r, "failed")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_action_and_batched_steps() {
        let steps = parse_steps(json!({"action": "open", "url": "http://localhost:5173"})).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].action, "open");

        let steps = parse_steps(json!({
            "steps": [
                {"action": "open", "url": "http://localhost:5173"},
                {"action": "snapshot", "boxes": true}
            ]
        }))
        .unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[1].action, "snapshot");
        assert_eq!(steps[1].boxes, Some(true));
    }

    #[test]
    fn rejects_empty_oversized_mixed_and_unknown_fields() {
        assert!(parse_steps(json!({"steps": []})).is_err());

        let too_many = vec![json!({"action": "tabs"}); MAX_BATCH_STEPS + 1];
        assert!(parse_steps(json!({"steps": too_many})).is_err());

        let mixed = parse_steps(json!({
            "action": "open",
            "steps": [{"action": "tabs"}]
        }))
        .unwrap_err();
        assert!(mixed.contains("not both"));

        let unknown = parse_steps(json!({"action": "open", "bogus": 1})).unwrap_err();
        assert!(unknown.contains("unknown field"));
    }
}
