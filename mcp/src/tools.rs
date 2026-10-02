// MCP tool definitions

use async_trait::async_trait;
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::tool_box;
use rust_mcp_sdk::schema::{schema_utils::CallToolError, CallToolRequestParams, CallToolResult, ListToolsResult, PaginatedRequestParams, RpcError, TextContent};
use rust_mcp_sdk::mcp_server::ServerHandler;
use rust_mcp_sdk::McpServer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::client::SameBoyClient;

type ClientArc = Arc<SameBoyClient>;

//****************//
//  ROM Load  //
//****************//
#[mcp_tool(
    name = "rom.load",
    description = "Load a Game Boy ROM file"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RomLoad {
    /// Path to the ROM file
    pub path: String,
}

//****************//
//  Emulator Reset  //
//****************//
#[mcp_tool(
    name = "emulator.reset",
    description = "Reset the emulator. Use reload=true to reload the ROM."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EmulatorReset {
    /// If true, reload the ROM. If false, quick reset.
    #[serde(default)]
    pub reload: bool,
}

//****************//
//  Emulator Pause  //
//****************//
#[mcp_tool(
    name = "emulator.pause",
    description = "Pause emulation"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EmulatorPause {}

//****************//
//  Emulator Resume  //
//****************//
#[mcp_tool(
    name = "emulator.resume",
    description = "Resume emulation"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EmulatorResume {}

//****************//
//  CPU Step  //
//****************//
#[mcp_tool(
    name = "cpu.step",
    description = "Execute one instruction"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CpuStep {}

//****************//
//  CPU Next  //
//****************//
#[mcp_tool(
    name = "cpu.next",
    description = "Step over function calls"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CpuNext {}

//****************//
//  CPU Finish  //
//****************//
#[mcp_tool(
    name = "cpu.finish",
    description = "Run until current function returns"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CpuFinish {}

//****************//
//  CPU Backstep  //
//****************//
#[mcp_tool(
    name = "cpu.backstep",
    description = "Step backward (requires rewind enabled)"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CpuBackstep {}

//****************//
//  CPU Undo  //
//****************//
#[mcp_tool(
    name = "cpu.undo",
    description = "Undo the last debugger command"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CpuUndo {}

//****************//
//  CPU Registers Read  //
//****************//
#[mcp_tool(
    name = "cpu.registers.read",
    description = "Read CPU registers. Optionally specify a single register name (a, f, b, c, d, e, h, l, af, bc, de, hl, sp, pc)."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CpuRegistersRead {
    /// Optional register name. If omitted, returns all registers.
    pub name: Option<String>,
}

//****************//
//  CPU Registers Write  //
//****************//
#[mcp_tool(
    name = "cpu.registers.write",
    description = "Write a CPU register"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CpuRegistersWrite {
    /// Register name (a, f, b, c, d, e, h, l, af, bc, de, hl, sp, pc)
    pub name: String,
    /// 16-bit value to write
    pub value: u16,
}

//****************//
//  Memory Read  //
//****************//
#[mcp_tool(
    name = "memory.read",
    description = "Read memory region"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MemoryRead {
    /// Start address (0-65535)
    pub address: u16,
    /// Number of bytes to read
    pub size: u16,
    /// Memory bank (optional, uses current bank if omitted)
    pub bank: Option<u16>,
}

//****************//
//  Memory Write  //
//****************//
#[mcp_tool(
    name = "memory.write",
    description = "Write to memory"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MemoryWrite {
    /// Start address
    pub address: u16,
    /// Bytes to write
    pub data: Vec<u8>,
    /// Memory bank (optional)
    pub bank: Option<u16>,
}

//****************//
//  Memory Dump  //
//****************//
#[mcp_tool(
    name = "memory.dump",
    description = "Hex dump of memory region"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MemoryDump {
    /// Start address
    pub address: u16,
    /// Number of bytes (default 16)
    #[serde(default = "default_dump_size")]
    pub size: u16,
    /// Memory bank (optional)
    pub bank: Option<u16>,
}

fn default_dump_size() -> u16 { 16 }

//****************//
//  Breakpoint Add  //
//****************//
#[mcp_tool(
    name = "breakpoint.add",
    description = "Add a breakpoint"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct BreakpointAdd {
    /// Breakpoint address
    pub address: u16,
    /// Memory bank (-1 for any)
    pub bank: Option<u16>,
    /// Range end address (for range breakpoints)
    pub range_end: Option<u16>,
    /// Whether range_end is inclusive
    #[serde(default)]
    pub inclusive: bool,
    /// Condition expression (e.g., "a > 5")
    pub condition: Option<String>,
}

//****************//
//  Breakpoint Remove  //
//****************//
#[mcp_tool(
    name = "breakpoint.remove",
    description = "Remove a breakpoint. If id is omitted, removes all."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct BreakpointRemove {
    /// Breakpoint ID to remove
    pub id: Option<u32>,
}

//****************//
//  Breakpoint List  //
//****************//
#[mcp_tool(
    name = "breakpoint.list",
    description = "List all breakpoints"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct BreakpointList {}

//****************//
//  Watchpoint Add  //
//****************//
#[mcp_tool(
    name = "watchpoint.add",
    description = "Add a memory watchpoint. Type: 'r' (read), 'w' (write), 'rw' (both)."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct WatchpointAdd {
    /// Watch address
    pub address: u16,
    /// Watch type: "r", "w", or "rw"
    #[serde(default = "default_watch_type")]
    pub r#type: String,
    /// Memory bank (optional)
    pub bank: Option<u16>,
    /// Condition expression (can use 'old' and 'new')
    pub condition: Option<String>,
}

fn default_watch_type() -> String { "w".into() }

//****************//
//  Watchpoint Remove  //
//****************//
#[mcp_tool(
    name = "watchpoint.remove",
    description = "Remove a watchpoint. If id is omitted, removes all."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct WatchpointRemove {
    pub id: Option<u32>,
}

//****************//
//  Watchpoint List  //
//****************//
#[mcp_tool(
    name = "watchpoint.list",
    description = "List all watchpoints"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct WatchpointList {}

//****************//
//  Disassemble  //
//****************//
#[mcp_tool(
    name = "disassemble",
    description = "Disassemble code at PC or given address"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Disassemble {
    /// Address to disassemble (default: current PC)
    pub address: Option<u16>,
    /// Number of instructions (default: 5)
    #[serde(default = "default_disasm_count")]
    pub count: u16,
}

fn default_disasm_count() -> u16 { 5 }

//****************//
//  Eval  //
//****************//
#[mcp_tool(
    name = "eval",
    description = "Evaluate a debugger expression (registers, memory, arithmetic)"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Eval {
    /// Expression to evaluate (e.g., "a + [pc]", "$8000:$100")
    pub expression: String,
}

//****************//
//  Backtrace  //
//****************//
#[mcp_tool(
    name = "backtrace",
    description = "Get the call stack"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Backtrace {}

//****************//
//  State Save  //
//****************//
#[mcp_tool(
    name = "state.save",
    description = "Save emulator state to slot (0-9)"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StateSave {
    /// Save slot (0-9)
    pub slot: u8,
}

//****************//
//  State Load  //
//****************//
#[mcp_tool(
    name = "state.load",
    description = "Load emulator state from slot (0-9)"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StateLoad {
    /// Save slot (0-9)
    pub slot: u8,
}

//****************//
//  Symbol Load  //
//****************//
#[mcp_tool(
    name = "symbol.load",
    description = "Load a symbol file (.sym)"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SymbolLoad {
    /// Path to .sym file
    pub path: String,
}

//****************//
//  Input Press  //
//****************//
#[mcp_tool(
    name = "input.press",
    description = "Simulate button press or release. Keys: 0=Right, 1=Left, 2=Up, 3=Down, 4=A, 5=B, 6=Select, 7=Start"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct InputPress {
    /// Key index (0-7)
    pub key: u32,
    /// true=press, false=release
    #[serde(default)]
    pub state: bool,
}

//****************//
//  APU State  //
//****************//
#[mcp_tool(
    name = "apu.state",
    description = "Get APU (audio processor) state"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ApuState {}

//****************//
//  APU Wave  //
//****************//
#[mcp_tool(
    name = "apu.wave",
    description = "Get wave RAM (16 bytes, channel 3)"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ApuWave {}

//****************//
//  LCD State  //
//****************//
#[mcp_tool(
    name = "lcd.state",
    description = "Get LCD controller state"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LcdState {}

//****************//
//  Cartridge Info  //
//****************//
#[mcp_tool(
    name = "cartridge.info",
    description = "Get cartridge/MBC info"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CartridgeInfo {}

//****************//
//  VRAM Read  //
//****************//
#[mcp_tool(
    name = "vram.read",
    description = "Read VRAM (tile data, $8000-$9FFF)"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct VramRead {
    /// Offset from $8000
    pub offset: u16,
    /// Number of bytes to read
    #[serde(default = "default_vram_size")]
    pub size: u16,
}

fn default_vram_size() -> u16 { 256 }

//****************//
//  VRAM Tile  //
//****************//
#[mcp_tool(
    name = "vram.tile",
    description = "Get a specific tile from VRAM as structured 8x8 data"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct VramTile {
    /// Tile ID (0-255 for DMG, 0-511 for CGB)
    pub tile_id: u8,
}

//****************//
//  VRAM Tiles  //
//****************//
#[mcp_tool(
    name = "vram.tiles",
    description = "List all tiles with metadata"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct VramTiles {}

//****************//
//  OAM Read  //
//****************//
#[mcp_tool(
    name = "oam.read",
    description = "Read sprite attributes. If sprite_id is omitted, returns all."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OamRead {
    /// Sprite ID (0-39). Omit for all sprites.
    pub sprite_id: Option<u8>,
}

//****************//
//  OAM List  //
//****************//
#[mcp_tool(
    name = "oam.list",
    description = "List all active sprites"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct OamList {}

//****************//
//  PPU State  //
//****************//
#[mcp_tool(
    name = "ppu.state",
    description = "Get LCD/PPU state (control registers, scanline, mode)"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PpuState {}

//****************//
//  PPU Palette  //
//****************//
#[mcp_tool(
    name = "ppu.palette",
    description = "Get current palette data"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PpuPalette {}

//****************//
//  Context Snapshot  //
//****************//
#[mcp_tool(
    name = "context.snapshot",
    description = "Full register dump + disassembly around PC or given address"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ContextSnapshot {
    /// Address (default: current PC)
    pub address: Option<u16>,
    /// Context range (instructions before/after)
    #[serde(default = "default_context_range")]
    pub range: u16,
}

fn default_context_range() -> u16 { 8 }

//****************//
//  Context History  //
//****************//
#[mcp_tool(
    name = "context.history",
    description = "Get recent register snapshots from breakpoint hits"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ContextHistory {}

//****************//
//  Screenshot  //
//****************//
#[mcp_tool(
    name = "screenshot",
    description = "Get current screen as PNG (base64 encoded)"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Screenshot {}

//****************//
//  Tool Box  //
//****************//
tool_box!(SameBoyTools, [
    RomLoad, EmulatorReset, EmulatorPause, EmulatorResume,
    CpuStep, CpuNext, CpuFinish, CpuBackstep, CpuUndo,
    CpuRegistersRead, CpuRegistersWrite,
    MemoryRead, MemoryWrite, MemoryDump,
    BreakpointAdd, BreakpointRemove, BreakpointList,
    WatchpointAdd, WatchpointRemove, WatchpointList,
    Disassemble, Eval, Backtrace,
    StateSave, StateLoad, SymbolLoad,
    InputPress,
    ApuState, ApuWave, LcdState, CartridgeInfo,
    VramRead, VramTile, VramTiles,
    OamRead, OamList,
    PpuState, PpuPalette,
    ContextSnapshot, ContextHistory,
    Screenshot,
]);

//****************//
//  Handler  //
//****************//
#[derive(Default)]
pub struct ToolHandler {
    pub client: Option<ClientArc>,
}

#[async_trait]
impl ServerHandler for ToolHandler {
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: SameBoyTools::tools(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _runtime: Arc<dyn McpServer>,
    ) -> std::result::Result<CallToolResult, CallToolError> {
        let tool = SameBoyTools::try_from(params).map_err(CallToolError::new)?;
        
        match tool {
            SameBoyTools::RomLoad(t) => handle_rom_load(&self.client, &t).await,
            SameBoyTools::EmulatorReset(t) => handle_emulator_reset(&self.client, &t).await,
            SameBoyTools::EmulatorPause(_) => call_simple(&self.client, "emulator.pause").await,
            SameBoyTools::EmulatorResume(_) => call_simple(&self.client, "emulator.resume").await,
            SameBoyTools::CpuStep(_) => call_simple(&self.client, "cpu.step").await,
            SameBoyTools::CpuNext(_) => call_simple(&self.client, "cpu.next").await,
            SameBoyTools::CpuFinish(_) => call_simple(&self.client, "cpu.finish").await,
            SameBoyTools::CpuBackstep(_) => call_simple(&self.client, "cpu.backstep").await,
            SameBoyTools::CpuUndo(_) => call_simple(&self.client, "cpu.undo").await,
            SameBoyTools::CpuRegistersRead(t) => handle_registers_read(&self.client, &t).await,
            SameBoyTools::CpuRegistersWrite(t) => handle_registers_write(&self.client, &t).await,
            SameBoyTools::MemoryRead(t) => handle_memory_read(&self.client, &t).await,
            SameBoyTools::MemoryWrite(t) => handle_memory_write(&self.client, &t).await,
            SameBoyTools::MemoryDump(t) => handle_memory_dump(&self.client, &t).await,
            SameBoyTools::BreakpointAdd(t) => handle_breakpoint_add(&self.client, &t).await,
            SameBoyTools::BreakpointRemove(t) => handle_breakpoint_remove(&self.client, &t).await,
            SameBoyTools::BreakpointList(_) => call_simple(&self.client, "breakpoint.list").await,
            SameBoyTools::WatchpointAdd(t) => handle_watchpoint_add(&self.client, &t).await,
            SameBoyTools::WatchpointRemove(t) => handle_watchpoint_remove(&self.client, &t).await,
            SameBoyTools::WatchpointList(_) => call_simple(&self.client, "watchpoint.list").await,
            SameBoyTools::Disassemble(t) => handle_disassemble(&self.client, &t).await,
            SameBoyTools::Eval(t) => handle_eval(&self.client, &t).await,
            SameBoyTools::Backtrace(_) => call_simple(&self.client, "backtrace").await,
            SameBoyTools::StateSave(t) => handle_state_save(&self.client, &t).await,
            SameBoyTools::StateLoad(t) => handle_state_load(&self.client, &t).await,
            SameBoyTools::SymbolLoad(t) => handle_symbol_load(&self.client, &t).await,
            SameBoyTools::InputPress(t) => handle_input_press(&self.client, &t).await,
            SameBoyTools::ApuState(_) => call_simple(&self.client, "apu.state").await,
            SameBoyTools::ApuWave(_) => call_simple(&self.client, "apu.wave").await,
            SameBoyTools::LcdState(_) => call_simple(&self.client, "lcd.state").await,
            SameBoyTools::CartridgeInfo(_) => call_simple(&self.client, "cartridge.info").await,
            SameBoyTools::VramRead(t) => handle_vram_read(&self.client, &t).await,
            SameBoyTools::VramTile(t) => handle_vram_tile(&self.client, &t).await,
            SameBoyTools::VramTiles(_) => call_simple(&self.client, "vram.tiles").await,
            SameBoyTools::OamRead(t) => handle_oam_read(&self.client, &t).await,
            SameBoyTools::OamList(_) => call_simple(&self.client, "oam.list").await,
            SameBoyTools::PpuState(_) => call_simple(&self.client, "ppu.state").await,
            SameBoyTools::PpuPalette(_) => call_simple(&self.client, "ppu.palette").await,
            SameBoyTools::ContextSnapshot(t) => handle_context_snapshot(&self.client, &t).await,
            SameBoyTools::ContextHistory(_) => call_simple(&self.client, "context.history").await,
            SameBoyTools::Screenshot(_) => call_simple(&self.client, "screenshot").await,
        }
    }
}

//****************//
//  Helpers  //
//****************//
async fn call_simple(
    client: &Option<ClientArc>,
    method: &str,
) -> std::result::Result<CallToolResult, CallToolError> {
    let client = client.as_ref().ok_or_else(|| {
        CallToolError::from_message("No ROM loaded. Use rom.load first.")
    })?;
    
    let resp = client.call(method, None).await
        .map_err(|e| CallToolError::from_message(format!("SameBoy error: {}", e)))?;
    
    if let Some(err) = &resp.error {
        return Err(CallToolError::from_message(err.clone()));
    }
    
    Ok(CallToolResult::text_content(vec![
        TextContent::from(
            serde_json::to_string(&resp.result).unwrap_or_default()
        )
    ]))
}

async fn call_with_params(
    client: &Option<ClientArc>,
    method: &str,
    params: serde_json::Value,
) -> std::result::Result<CallToolResult, CallToolError> {
    let client = client.as_ref().ok_or_else(|| {
        CallToolError::from_message("No ROM loaded. Use rom.load first.")
    })?;
    
    let resp = client.call(method, Some(params)).await
        .map_err(|e| CallToolError::from_message(format!("SameBoy error: {}", e)))?;
    
    if let Some(err) = &resp.error {
        return Err(CallToolError::from_message(err.clone()));
    }
    
    Ok(CallToolResult::text_content(vec![
        TextContent::from(
            serde_json::to_string(&resp.result).unwrap_or_default()
        )
    ]))
}

// Tool handlers
async fn handle_rom_load(
    _client: &Option<ClientArc>,
    _tool: &RomLoad,
) -> std::result::Result<CallToolResult, CallToolError> {
    Err(CallToolError::from_message("ROM loading requires process restart. Use SAMEBOY_ROM env var or command-line argument."))
}

async fn handle_emulator_reset(
    client: &Option<ClientArc>,
    tool: &EmulatorReset,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "emulator.reset", serde_json::json!({
        "reload": tool.reload,
    })).await
}

async fn handle_registers_read(
    client: &Option<ClientArc>,
    tool: &CpuRegistersRead,
) -> std::result::Result<CallToolResult, CallToolError> {
    if let Some(ref name) = tool.name {
        call_with_params(client, "cpu.registers.read", serde_json::json!({"name": name})).await
    } else {
        call_simple(client, "cpu.registers.read").await
    }
}

async fn handle_registers_write(
    client: &Option<ClientArc>,
    tool: &CpuRegistersWrite,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "cpu.registers.write", serde_json::json!({
        "name": tool.name,
        "value": tool.value,
    })).await
}

async fn handle_memory_read(
    client: &Option<ClientArc>,
    tool: &MemoryRead,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "memory.read", serde_json::json!({
        "address": tool.address,
        "size": tool.size,
        "bank": tool.bank,
    })).await
}

async fn handle_memory_write(
    client: &Option<ClientArc>,
    tool: &MemoryWrite,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "memory.write", serde_json::json!({
        "address": tool.address,
        "data": tool.data,
        "bank": tool.bank,
    })).await
}

async fn handle_memory_dump(
    client: &Option<ClientArc>,
    tool: &MemoryDump,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "memory.dump", serde_json::json!({
        "address": tool.address,
        "size": tool.size,
        "bank": tool.bank,
    })).await
}

async fn handle_breakpoint_add(
    client: &Option<ClientArc>,
    tool: &BreakpointAdd,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "breakpoint.add", serde_json::json!({
        "address": tool.address,
        "bank": tool.bank,
        "range_end": tool.range_end,
        "inclusive": tool.inclusive,
        "condition": tool.condition,
    })).await
}

async fn handle_breakpoint_remove(
    client: &Option<ClientArc>,
    tool: &BreakpointRemove,
) -> std::result::Result<CallToolResult, CallToolError> {
    if let Some(id) = tool.id {
        call_with_params(client, "breakpoint.remove", serde_json::json!({"id": id})).await
    } else {
        call_simple(client, "breakpoint.remove").await
    }
}

async fn handle_watchpoint_add(
    client: &Option<ClientArc>,
    tool: &WatchpointAdd,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "watchpoint.add", serde_json::json!({
        "address": tool.address,
        "type": tool.r#type,
        "bank": tool.bank,
        "condition": tool.condition,
    })).await
}

async fn handle_watchpoint_remove(
    client: &Option<ClientArc>,
    tool: &WatchpointRemove,
) -> std::result::Result<CallToolResult, CallToolError> {
    if let Some(id) = tool.id {
        call_with_params(client, "watchpoint.remove", serde_json::json!({"id": id})).await
    } else {
        call_simple(client, "watchpoint.remove").await
    }
}

async fn handle_disassemble(
    client: &Option<ClientArc>,
    tool: &Disassemble,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "disassemble", serde_json::json!({
        "address": tool.address,
        "count": tool.count,
    })).await
}

async fn handle_eval(
    client: &Option<ClientArc>,
    tool: &Eval,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "eval", serde_json::json!({
        "expression": tool.expression,
    })).await
}

async fn handle_state_save(
    client: &Option<ClientArc>,
    tool: &StateSave,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "state.save", serde_json::json!({"slot": tool.slot})).await
}

async fn handle_state_load(
    client: &Option<ClientArc>,
    tool: &StateLoad,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "state.load", serde_json::json!({"slot": tool.slot})).await
}

async fn handle_symbol_load(
    client: &Option<ClientArc>,
    tool: &SymbolLoad,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "symbol.load", serde_json::json!({"path": tool.path})).await
}

async fn handle_input_press(
    client: &Option<ClientArc>,
    tool: &InputPress,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "input.press", serde_json::json!({
        "key": tool.key,
        "state": tool.state,
    })).await
}

async fn handle_vram_read(
    client: &Option<ClientArc>,
    tool: &VramRead,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "vram.read", serde_json::json!({
        "offset": tool.offset,
        "size": tool.size,
    })).await
}

async fn handle_vram_tile(
    client: &Option<ClientArc>,
    tool: &VramTile,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "vram.tile", serde_json::json!({
        "tile_id": tool.tile_id,
    })).await
}

async fn handle_oam_read(
    client: &Option<ClientArc>,
    tool: &OamRead,
) -> std::result::Result<CallToolResult, CallToolError> {
    if let Some(id) = tool.sprite_id {
        call_with_params(client, "oam.read", serde_json::json!({"sprite_id": id})).await
    } else {
        call_simple(client, "oam.read").await
    }
}

async fn handle_context_snapshot(
    client: &Option<ClientArc>,
    tool: &ContextSnapshot,
) -> std::result::Result<CallToolResult, CallToolError> {
    call_with_params(client, "context.snapshot", serde_json::json!({
        "address": tool.address,
        "range": tool.range,
    })).await
}
