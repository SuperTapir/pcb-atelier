#[path = "../../../../crates/atelier-core/tests/support/mod.rs"]
#[allow(dead_code)]
mod support;

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use atelier_core::{
    CardSide, ContentKind, FaceProductionLayer, ImageTreatment, ProductionTarget,
    ProjectBundleRasterizer, build_production_trace, compile_fabrication_plan,
    resolve_fabrication_plan,
};
use atelier_desktop::{
    WORKSPACE_CONTRACT_VERSION, WorkspaceBridgeRequest, WorkspaceBridgeResponse, WorkspaceService,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[test]
fn one_asymmetric_project_matches_core_cli_web_and_tauri_golden_contract() {
    let mut fixture = support::asymmetric_golden_card();
    attach_default_image_treatments(&mut fixture.bundle.document);
    let directory = tempfile::tempdir().unwrap();
    let project_path = directory.path().join("cross-runtime-golden.pcba");
    fixture.bundle.save(&project_path).unwrap();

    let plan = compile_fabrication_plan(&fixture.bundle.document).unwrap();
    let mut rasterizer = ProjectBundleRasterizer::new(&fixture.bundle).unwrap();
    let resolved = resolve_fabrication_plan(&plan, 200, &mut rasterizer).unwrap();
    let core = build_production_trace(1, &fixture.bundle.document, &resolved);
    let core_json = serde_json::to_value(&core).unwrap();
    assert_golden_baseline(&core_json);

    let cli: Value = serde_json::from_str(
        &atelier_cli::execute(&[
            "production-inspect".to_owned(),
            project_path.display().to_string(),
            "--pitch-um".to_owned(),
            "200".to_owned(),
        ])
        .unwrap(),
    )
    .unwrap();
    assert_eq!(cli["coordinateSpace"], core_json["coordinateSpace"]);
    assert_eq!(cli["layers"], core_json["layers"]);
    assert_eq!(cli["operations"], core_json["operations"]);
    assert_eq!(cli["manufacturerProfile"], core_json["manufacturerProfile"]);
    assert_eq!(
        cli["manufacturerProfileFingerprint"],
        core_json["manufacturerProfileFingerprint"]
    );
    assert_eq!(
        cli["build"]["inputSha256"],
        core_json["fabricationInputSha256"]
    );
    assert_eq!(
        cli["build"]["outputSha256"],
        core_json["fabricationOutputSha256"]
    );

    let mut tauri = WorkspaceService::new(atelier_core::AtelierDocument::new_card(
        "empty", 1_000, 1_000,
    ));
    open_project(&mut tauri, &project_path);
    let tauri_trace = invoke(&mut tauri, "get_production_trace", json!({}));
    assert_eq!(tauri_trace.payload, core_json);

    let port = reserve_port();
    let mut web = WebBridgeProcess::spawn(port);
    web.wait_until_ready();
    let opened = web.invoke(
        "open_project",
        json!({ "request": { "path": project_path } }),
    );
    assert_eq!(opened.error, None);
    let web_trace = web.invoke("get_production_trace", json!({}));
    assert_eq!(web_trace.payload, core_json);
}

fn attach_default_image_treatments(document: &mut atelier_core::AtelierDocument) {
    for side in [CardSide::Front, CardSide::Back] {
        let layers = match side {
            CardSide::Front => &document.front.layers,
            CardSide::Back => &document.back.layers,
        };
        let image = layers
            .iter()
            .find_map(|layer| match layer.kind {
                ContentKind::Image(ref image) => Some((layer.id, image.asset_id)),
                _ => None,
            })
            .unwrap();
        let treatment = ImageTreatment::new(image.1, Default::default());
        let treatment_id = treatment.id;
        document.image_treatments.push(treatment);
        for mapping in document
            .mappings
            .iter_mut()
            .filter(|mapping| mapping.source_layer_id == image.0)
        {
            mapping.treatment_id = Some(treatment_id);
        }
    }
}

fn assert_golden_baseline(trace: &Value) {
    assert_eq!(trace["coordinateSpace"], "boardPhysicalUpright");
    let layers = trace["layers"].as_array().unwrap();
    assert_eq!(
        layers,
        json!([
            {
                "target": { "side": "front", "layer": "copper" },
                "polarity": "positive",
                "compositeSha256": "ace0a46b8272fa496d03059265d0351421799d429a49d3693867cc31dbc89d93",
                "boundsUm": { "minXUm": 13200, "minYUm": 31000, "maxXUm": 21000, "maxYUm": 42000 },
                "topology": { "islandCount": 1, "holeCount": 0 }
            },
            {
                "target": { "side": "front", "layer": "solderMaskOpen" },
                "polarity": "opening",
                "compositeSha256": "ace0a46b8272fa496d03059265d0351421799d429a49d3693867cc31dbc89d93",
                "boundsUm": { "minXUm": 13200, "minYUm": 31000, "maxXUm": 21000, "maxYUm": 42000 },
                "topology": { "islandCount": 1, "holeCount": 0 }
            },
            {
                "target": { "side": "front", "layer": "silkscreen" },
                "polarity": "positive",
                "compositeSha256": "bc3c46c9e19d4f24d268016afca28a240e80680ff86142bc9651c7c6a7b5519e",
                "boundsUm": { "minXUm": 43400, "minYUm": 10000, "maxXUm": 45000, "maxYUm": 13000 },
                "topology": { "islandCount": 1, "holeCount": 0 }
            },
            {
                "target": { "side": "back", "layer": "copper" },
                "polarity": "positive",
                "compositeSha256": "bde45a4ea7ce9d9b45c9327c2db141ef924347e613f1063c5127e239b7f16f0f",
                "boundsUm": { "minXUm": 33200, "minYUm": 38000, "maxXUm": 55000, "maxYUm": 85000 },
                "topology": { "islandCount": 2, "holeCount": 0 }
            },
            {
                "target": { "side": "back", "layer": "solderMaskOpen" },
                "polarity": "opening",
                "compositeSha256": "bde45a4ea7ce9d9b45c9327c2db141ef924347e613f1063c5127e239b7f16f0f",
                "boundsUm": { "minXUm": 33200, "minYUm": 38000, "maxXUm": 55000, "maxYUm": 85000 },
                "topology": { "islandCount": 2, "holeCount": 0 }
            },
            {
                "target": { "side": "back", "layer": "silkscreen" },
                "polarity": "positive",
                "compositeSha256": "d4859663e151f15400e8b37bff403e3567ab1e6370c0627c8b386520e15ed279",
                "boundsUm": { "minXUm": 7400, "minYUm": 76000, "maxXUm": 9400, "maxYUm": 79000 },
                "topology": { "islandCount": 1, "holeCount": 2 }
            }
        ])
        .as_array()
        .unwrap()
    );
    assert_eq!(
        trace["manufacturerProfileFingerprint"],
        "610ced5a12cb82eb217bdb5786ae2c6182fe583390309be904ab0b0d51eeb6d9"
    );
    let front_copper = layer(trace, CardSide::Front, FaceProductionLayer::Copper);
    let front_opening = layer(trace, CardSide::Front, FaceProductionLayer::SolderMaskOpen);
    let back_copper = layer(trace, CardSide::Back, FaceProductionLayer::Copper);
    let back_opening = layer(trace, CardSide::Back, FaceProductionLayer::SolderMaskOpen);
    assert_eq!(front_copper["polarity"], "positive");
    assert_eq!(front_opening["polarity"], "opening");
    assert_eq!(back_copper["polarity"], "positive");
    assert_eq!(back_opening["polarity"], "opening");
    assert_eq!(
        front_copper["topology"],
        json!({ "islandCount": 1, "holeCount": 0 })
    );
    assert_eq!(
        back_copper["topology"],
        json!({ "islandCount": 2, "holeCount": 0 })
    );
    assert_eq!(
        front_copper["compositeSha256"],
        front_opening["compositeSha256"]
    );
    assert_eq!(
        back_copper["compositeSha256"],
        back_opening["compositeSha256"]
    );
    assert_ne!(
        front_copper["compositeSha256"],
        back_copper["compositeSha256"]
    );
    assert!(
        back_copper["boundsUm"]["minXUm"].as_i64().unwrap()
            > front_copper["boundsUm"]["maxXUm"].as_i64().unwrap(),
        "背面非对称标记必须保持板物理坐标，不得被适配器预镜像"
    );
    for operation in trace["operations"].as_array().unwrap() {
        if operation["assetId"].is_string() {
            assert_eq!(
                operation["algorithmVersion"],
                atelier_core::TREATMENT_ALGORITHM_VERSION
            );
            assert_eq!(
                operation["recipeFingerprint"],
                "5abe3f2d53ee2523d2b34586760ca4e97cdccc2f1ec267534ecea05972c318cf"
            );
        }
    }
}

fn layer(trace: &Value, side: CardSide, production_layer: FaceProductionLayer) -> &Value {
    let target = ProductionTarget::new(side, production_layer);
    trace["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| {
            serde_json::from_value::<ProductionTarget>(layer["target"].clone()).unwrap() == target
        })
        .unwrap()
}

fn open_project(service: &mut WorkspaceService, path: &std::path::Path) {
    let response = invoke(
        service,
        "open_project",
        json!({ "request": { "path": path } }),
    );
    assert_eq!(response.error, None);
}

fn invoke(service: &mut WorkspaceService, command: &str, args: Value) -> WorkspaceBridgeResponse {
    service.invoke(WorkspaceBridgeRequest {
        contract_version: WORKSPACE_CONTRACT_VERSION.to_owned(),
        command: command.to_owned(),
        args,
    })
}

fn reserve_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct WebBridgeProcess {
    child: Child,
    port: u16,
}

impl WebBridgeProcess {
    fn spawn(port: u16) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_workspace-bridge"))
            .env("PCB_ATELIER_BRIDGE_ADDR", format!("127.0.0.1:{port}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self { child, port }
    }

    fn wait_until_ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(status) = http_request(self.port, "GET", "/health", "") {
                if status.contains(r#"{"status":"ok"}"#) {
                    return;
                }
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "Web bridge exited"
            );
            thread::sleep(Duration::from_millis(25));
        }
        panic!("Web bridge did not become ready");
    }

    fn invoke(&self, command: &str, args: Value) -> WorkspaceBridgeResponse {
        let request = WorkspaceBridgeRequest {
            contract_version: WORKSPACE_CONTRACT_VERSION.to_owned(),
            command: command.to_owned(),
            args,
        };
        let body = serde_json::to_string(&request).unwrap();
        let response = http_request(self.port, "POST", "/workspace", &body).unwrap();
        let response: HttpBridgeResponse =
            serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
        WorkspaceBridgeResponse {
            contract_version: WORKSPACE_CONTRACT_VERSION,
            revision: response.revision,
            payload: response.payload,
            error: response.error,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpBridgeResponse {
    revision: u64,
    payload: Value,
    error: Option<String>,
}

impl Drop for WebBridgeProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn http_request(port: u16, method: &str, path: &str, body: &str) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}
