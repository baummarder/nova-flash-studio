//! Opt-in live tests against a real local Ollama instance.
//!
//! These tests are ignored by default so `cargo test --workspace` stays
//! deterministic and offline. Run them explicitly:
//!
//! ```text
//! cargo test -p nova-ai --test live_ollama -- --ignored --nocapture --test-threads=1
//! ```
//!
//! `NOVA_LIVE_OLLAMA_MODEL` selects an installed model; the default is
//! `qwen2.5-coder:7b`. No model is downloaded by these tests.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use nova_ai::{
    AgentSession, ChatMessage, ContextSelection, ProviderConfig, ProviderKind, ProviderManager,
};

fn temp_config_dir(name: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("nova-live-{name}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temporary config directory");
    dir
}

fn local_provider(model: &str) -> ProviderConfig {
    let mut provider = ProviderConfig::new(ProviderKind::OllamaLocal);
    provider.name = "Ollama Live Test".into();
    provider.base_url = "http://localhost:11434".into();
    provider.model = model.into();
    provider.temperature = Some(0.0);
    provider
}

fn live_model() -> String {
    std::env::var("NOVA_LIVE_OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5-coder:7b".into())
}

#[test]
#[ignore = "requires a running local Ollama instance and an installed model"]
fn live_ollama_discovery_and_chat() {
    let dir = temp_config_dir("ollama-chat");
    let mut manager = ProviderManager::load(&dir).expect("load provider manager");
    let model = live_model();
    let provider = local_provider(&model);
    let id = manager.add(provider).expect("add local provider");
    manager.set_default(&id).expect("select local provider");
    manager.save().expect("save temporary provider settings");

    let connection = manager.test_connection(&id).expect("live model discovery");
    println!(
        "LIVE ollama connection: {} model(s), selected={model}",
        connection.models.len()
    );
    assert!(
        connection.models.iter().any(|name| name == &model),
        "selected model {model:?} is not installed; available: {:?}",
        connection.models
    );

    let response = manager
        .chat(
            &id,
            &[
                ChatMessage::system("Antworte knapp und auf Deutsch."),
                ChatMessage::user("Nenne in einem Satz, was ActionScript 3 ist."),
            ],
        )
        .expect("live Ollama chat");
    println!(
        "LIVE ollama reply ({}): {}",
        response.model, response.content
    );
    assert!(!response.content.trim().is_empty());
    assert_eq!(response.model, model);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "requires a running local Ollama instance and an installed model"]
fn live_ollama_agent_request_returns_parsed_reply() {
    let dir = temp_config_dir("ollama-agent");
    let mut manager = ProviderManager::load(&dir).expect("load provider manager");
    let model = live_model();
    let provider = local_provider(&model);
    let id = manager.add(provider).expect("add local provider");
    manager.set_default(&id).expect("select local provider");

    let project = nova_core::Project::demo();
    let mut session = AgentSession::default();
    session.context = ContextSelection {
        include_project_structure: true,
        include_timeline: true,
        include_script_metadata: true,
        include_script_sources: true,
        include_assets: true,
        include_diagnostics: true,
        selected_scripts: project.scripts.iter().map(|script| script.id).collect(),
        selected_objects: Vec::new(),
        selected_code: String::new(),
        notes: "Nur erklären, keine Datei ändern.".into(),
    };
    let outcome = session
        .request(
            &manager,
            &project,
            0,
            "Code",
            &[],
            "Erkläre in zwei Sätzen die Aufgabe des ausgewählten ActionScript-Codes. \
             Ändere keine Dateien.",
        )
        .expect("live agent request");

    println!(
        "LIVE agent provider={} model={} cloud={} context_chars={}",
        outcome.provider_name, outcome.model, outcome.cloud, outcome.context_chars
    );
    println!("LIVE agent message: {}", outcome.reply.message);
    assert!(!outcome.reply.message.trim().is_empty());
    assert!(
        !outcome.cloud,
        "localhost Ollama must be classified as local"
    );
    assert!(outcome.context_chars > 0);
    assert!(
        outcome.reply.changes.is_empty(),
        "the model was told not to edit"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "checks a deliberately unavailable local endpoint"]
fn live_ollama_unavailable_endpoint_is_a_network_error() {
    let dir = temp_config_dir("ollama-unavailable");
    let mut manager = ProviderManager::load(&dir).expect("load provider manager");
    let mut provider = local_provider("does-not-matter");
    provider.base_url = "http://127.0.0.1:1".into();
    let id = manager.add(provider).expect("add unreachable provider");

    let error = manager
        .test_connection(&id)
        .expect_err("unreachable endpoint must fail");
    println!("LIVE unavailable endpoint error: {error}");
    assert!(matches!(error, nova_ai::AiError::Network(_)));

    let _ = fs::remove_dir_all(&dir);
}
