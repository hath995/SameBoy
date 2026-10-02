// SameBoy MCP Server — entry point

use rust_mcp_sdk::{
    error::SdkResult,
    mcp_server::{server_runtime, McpServerOptions, ServerHandler},
    schema::*,
    StdioTransport, TransportOptions, McpServer, ToMcpServerHandler,
};
use std::sync::Arc;

mod client;
pub mod protocol;
mod resources;
mod tools;

use client::SameBoyClient;
use tools::ToolHandler;

#[derive(Default)]
struct SameBoyMcpHandler {
    tool_handler: ToolHandler,
}

#[async_trait::async_trait]
impl ServerHandler for SameBoyMcpHandler {
    async fn handle_initialize_request(
        &self,
        _request: InitializeRequestParams,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<InitializeResult, RpcError> {
        Ok(InitializeResult {
            server_info: Implementation {
                name: "sameboy-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("SameBoy MCP Server".into()),
                description: Some("AI-driven Game Boy emulation via SameBoy".into()),
                icons: vec![],
                website_url: Some("https://github.com/LIJI32/SameBoy".into()),
            },
            capabilities: ServerCapabilities {
                tools: Some(ServerCapabilitiesTools { list_changed: None }),
                resources: Some(ServerCapabilitiesResources {
                    subscribe: None,
                    list_changed: None,
                }),
                ..Default::default()
            },
            protocol_version: ProtocolVersion::V2025_11_25.into(),
            instructions: Some(
                "Load a ROM with rom.load, then control execution with cpu.step, breakpoints, and memory inspection."
                    .into(),
            ),
            meta: None,
        })
    }

    async fn handle_list_tools_request(
        &self,
        params: Option<PaginatedRequestParams>,
        runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        self.tool_handler
            .handle_list_tools_request(params, runtime)
            .await
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        self.tool_handler
            .handle_call_tool_request(params, runtime)
            .await
    }

    async fn handle_list_resources_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListResourcesResult, RpcError> {
        Ok(ListResourcesResult {
            resources: resources::list_resources(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_list_resource_templates_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListResourceTemplatesResult, RpcError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: resources::list_resource_templates(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_read_resource_request(
        &self,
        params: ReadResourceRequestParams,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ReadResourceResult, RpcError> {
        let Some(client) = &self.tool_handler.client else {
            return Err(RpcError::invalid_request().with_message("No ROM loaded"));
        };

        let Some((method, cmd_params)) = resources::parse_resource_uri(&params.uri) else {
            return Err(RpcError::invalid_request()
                .with_message(format!("Unknown resource: {}", params.uri)));
        };

        let resp = client
            .call(&method, cmd_params)
            .await
            .map_err(|e| RpcError::internal_error().with_message(e.to_string()))?;

        if let Some(err) = &resp.error {
            return Err(RpcError::internal_error().with_message(err.clone()));
        }

        let content = resp
            .result
            .map(|v| serde_json::to_string(&v).unwrap_or_default())
            .unwrap_or_default();

        Ok(ReadResourceResult {
            contents: vec![TextResourceContents::new(content, params.uri)
                .with_mime_type("application/json")
                .into()],
            meta: None,
        })
    }
}

#[tokio::main]
async fn main() -> SdkResult<()> {
    let bin_path = std::env::var("SAMEBOY_JSON_BIN")
        .ok()
        .unwrap_or_else(|| "build/bin/json-server/sameboy-json".into());

    let rom_path = std::env::args().nth(1)
        .or_else(|| std::env::var("SAMEBOY_ROM").ok());

    let mut handler = SameBoyMcpHandler::default();
    if let Some(rom) = &rom_path {
        match SameBoyClient::spawn(&bin_path, Some(rom)).await {
            Ok(client) => {
                handler.tool_handler.client = Some(Arc::new(client));
            }
            Err(e) => {
                eprintln!("Warning: Failed to spawn sameboy-json: {}", e);
            }
        }
    }

    let server_details = InitializeResult {
        server_info: Implementation {
            name: "sameboy-mcp".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            title: Some("SameBoy MCP Server".into()),
            description: Some("AI-driven Game Boy emulation".into()),
            icons: vec![],
            website_url: Some("https://github.com/LIJI32/SameBoy".into()),
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            resources: Some(ServerCapabilitiesResources {
                subscribe: None,
                list_changed: None,
            }),
            ..Default::default()
        },
        protocol_version: ProtocolVersion::V2025_11_25.into(),
        instructions: None,
        meta: None,
    };

    let transport = StdioTransport::new(TransportOptions::default())?;
    let handler_arc = handler.to_mcp_server_handler();

    let server = server_runtime::create_server(McpServerOptions {
        transport,
        handler: handler_arc,
        server_details,
        task_store: None,
        client_task_store: None,
        message_observer: None,
    });

    server.start().await
}
