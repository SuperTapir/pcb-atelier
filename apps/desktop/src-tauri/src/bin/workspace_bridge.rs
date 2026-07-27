use std::sync::{Arc, Mutex};

use atelier_core::{
    CardSide, CombineMode, ContentLayer, FaceProductionLayer, ProductionMapping, ProductionTarget,
    TransformUm,
};
use atelier_desktop::{WorkspaceBridgeRequest, WorkspaceService};
use tiny_http::{Header, Method, Response, Server, StatusCode};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let address =
        std::env::var("PCB_ATELIER_BRIDGE_ADDR").unwrap_or_else(|_| "127.0.0.1:1424".to_owned());
    if !address.starts_with("127.0.0.1:") && !address.starts_with("localhost:") {
        return Err("workspace bridge must bind to localhost".into());
    }
    let server = Server::http(&address)?;
    let service = Arc::new(Mutex::new(WorkspaceService::new(
        local_development_document(),
    )));
    eprintln!("PCB Atelier workspace bridge listening on http://{address}/workspace");

    for request in server.incoming_requests() {
        let service = Arc::clone(&service);
        std::thread::spawn(move || {
            if let Err(error) = handle_request(request, &service) {
                eprintln!("workspace bridge request failed: {error}");
            }
        });
    }
    Ok(())
}

fn handle_request(
    mut request: tiny_http::Request,
    service: &Mutex<WorkspaceService>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if request.method() == &Method::Get && request.url() == "/health" {
        let content_type = Header::from_bytes("Content-Type", "application/json")
            .expect("static header must be valid");
        request.respond(Response::from_string(r#"{"status":"ok"}"#).with_header(content_type))?;
        return Ok(());
    }
    if request.method() == &Method::Post && request.url() == "/reset" {
        *service
            .lock()
            .map_err(|_| "workspace bridge lock is poisoned")? =
            WorkspaceService::new(local_development_document());
        let content_type = Header::from_bytes("Content-Type", "application/json")
            .expect("static header must be valid");
        request
            .respond(Response::from_string(r#"{"status":"reset"}"#).with_header(content_type))?;
        return Ok(());
    }
    if request.method() != &Method::Post || request.url() != "/workspace" {
        request.respond(Response::empty(StatusCode(404)))?;
        return Ok(());
    }
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let response = match serde_json::from_str::<WorkspaceBridgeRequest>(&body) {
        Ok(command) if WorkspaceService::should_use_read_snapshot(&command.command) => {
            let mut snapshot = service
                .lock()
                .map_err(|_| "workspace bridge lock is poisoned")?
                .snapshot_for_read();
            snapshot.invoke(command)
        }
        Ok(command) => service
            .lock()
            .map_err(|_| "workspace bridge lock is poisoned")?
            .invoke(command),
        Err(error) => {
            request.respond(
                Response::from_string(format!("invalid workspace bridge request: {error}"))
                    .with_status_code(StatusCode(400)),
            )?;
            return Ok(());
        }
    };
    let body = serde_json::to_string(&response)?;
    let content_type = Header::from_bytes("Content-Type", "application/json")
        .expect("static header must be valid");
    request.respond(Response::from_string(body).with_header(content_type))?;
    Ok(())
}

fn local_development_document() -> atelier_core::AtelierDocument {
    let mut document = atelier_core::AtelierDocument::new_card("双面非对称黄金卡", 85_600, 53_980);
    let mut group = ContentLayer::new_group("正面组合");
    group.transform = TransformUm::rect(8_000, 8_000, 24_000, 6_000);
    let mut front_title = ContentLayer::new_text(
        "正面标题",
        "·",
        TransformUm::rect(8_000, 8_000, 24_000, 6_000),
    );
    front_title.parent_id = Some(group.id);
    let front_title_id = front_title.id;
    let front_caption = ContentLayer::new_text(
        "正面说明",
        " ",
        TransformUm::rect(12_000, 8_000, 24_000, 6_000),
    );
    let front_caption_id = front_caption.id;
    document.front.layers = vec![front_title, front_caption, group];
    let back_mark = ContentLayer::new_text(
        "背面标记",
        "·",
        TransformUm::rect(8_000, 8_000, 24_000, 6_000),
    );
    let back_mark_id = back_mark.id;
    let back_caption = ContentLayer::new_text(
        "背面说明",
        " ",
        TransformUm::rect(12_000, 8_000, 24_000, 6_000),
    );
    let back_caption_id = back_caption.id;
    document.back.layers = vec![back_mark, back_caption];
    document.mappings = [
        (front_title_id, CardSide::Front),
        (front_caption_id, CardSide::Front),
        (back_mark_id, CardSide::Back),
        (back_caption_id, CardSide::Back),
    ]
    .into_iter()
    .map(|(layer_id, side)| {
        ProductionMapping::new(
            layer_id,
            ProductionTarget::new(side, FaceProductionLayer::Silkscreen),
            CombineMode::Add,
        )
    })
    .collect();
    document
}
