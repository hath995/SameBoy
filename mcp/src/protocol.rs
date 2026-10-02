// JSON protocol for communicating with SameBoy JSON mode

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A JSON-RPC request sent to SameBoy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// A JSON-RPC response from SameBoy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A notification from SameBoy (no id, no result expected)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Commands that can be sent to SameBoy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Command {
    RomLoad { path: String },
    EmulatorReset { reload: bool },
    EmulatorPause,
    EmulatorResume,
    CpuStep,
    CpuNext,
    CpuFinish,
    CpuBackstep,
    CpuUndo,
    CpuRegistersRead { name: Option<String> },
    CpuRegistersWrite { name: String, value: u16 },
    MemoryRead { address: u16, size: u16, bank: Option<u16> },
    MemoryWrite { address: u16, data: Vec<u8>, bank: Option<u16> },
    MemoryDump { address: u16, size: u16, bank: Option<u16> },
    BreakpointAdd {
        address: u16,
        bank: Option<u16>,
        range_end: Option<u16>,
        inclusive: bool,
        condition: Option<String>,
    },
    BreakpointRemove { id: Option<u32> },
    BreakpointList,
    WatchpointAdd {
        address: u16,
        bank: Option<u16>,
        r#type: String, // "r", "w", or "rw"
        condition: Option<String>,
    },
    WatchpointRemove { id: Option<u32> },
    WatchpointList,
    Disassemble { address: Option<u16>, count: u16 },
    Eval { expression: String },
    Backtrace,
    StateSave { slot: u8 },
    StateLoad { slot: u8 },
    SymbolLoad { path: String },
    InputPress { key: u32, state: bool },
    ApuState,
    ApuWave,
    LcdState,
    CartridgeInfo,
    VramRead { offset: u16, size: u16 },
    VramTile { tile_id: u8 },
    VramTiles,
    OamRead { sprite_id: Option<u8> },
    OamList,
    PpuState,
    PpuPalette,
    ContextSnapshot { address: Option<u16>, range: u16 },
    ContextHistory,
    Screenshot,
    Quit,
}

impl Command {
    pub fn to_request(&self, id: u64) -> JsonRpcRequest {
        let method = self.method_name();
        let params = self.to_params();
        JsonRpcRequest {
            id,
            method,
            params,
        }
    }

    fn method_name(&self) -> String {
        match self {
            Command::RomLoad { .. } => "rom.load".into(),
            Command::EmulatorReset { .. } => "emulator.reset".into(),
            Command::EmulatorPause => "emulator.pause".into(),
            Command::EmulatorResume => "emulator.resume".into(),
            Command::CpuStep => "cpu.step".into(),
            Command::CpuNext => "cpu.next".into(),
            Command::CpuFinish => "cpu.finish".into(),
            Command::CpuBackstep => "cpu.backstep".into(),
            Command::CpuUndo => "cpu.undo".into(),
            Command::CpuRegistersRead { .. } => "cpu.registers.read".into(),
            Command::CpuRegistersWrite { .. } => "cpu.registers.write".into(),
            Command::MemoryRead { .. } => "memory.read".into(),
            Command::MemoryWrite { .. } => "memory.write".into(),
            Command::MemoryDump { .. } => "memory.dump".into(),
            Command::BreakpointAdd { .. } => "breakpoint.add".into(),
            Command::BreakpointRemove { .. } => "breakpoint.remove".into(),
            Command::BreakpointList => "breakpoint.list".into(),
            Command::WatchpointAdd { .. } => "watchpoint.add".into(),
            Command::WatchpointRemove { .. } => "watchpoint.remove".into(),
            Command::WatchpointList => "watchpoint.list".into(),
            Command::Disassemble { .. } => "disassemble".into(),
            Command::Eval { .. } => "eval".into(),
            Command::Backtrace => "backtrace".into(),
            Command::StateSave { .. } => "state.save".into(),
            Command::StateLoad { .. } => "state.load".into(),
            Command::SymbolLoad { .. } => "symbol.load".into(),
            Command::InputPress { .. } => "input.press".into(),
            Command::ApuState => "apu.state".into(),
            Command::ApuWave => "apu.wave".into(),
            Command::LcdState => "lcd.state".into(),
            Command::CartridgeInfo => "cartridge.info".into(),
            Command::VramRead { .. } => "vram.read".into(),
            Command::VramTile { .. } => "vram.tile".into(),
            Command::VramTiles => "vram.tiles".into(),
            Command::OamRead { .. } => "oam.read".into(),
            Command::OamList => "oam.list".into(),
            Command::PpuState => "ppu.state".into(),
            Command::PpuPalette => "ppu.palette".into(),
            Command::ContextSnapshot { .. } => "context.snapshot".into(),
            Command::ContextHistory => "context.history".into(),
            Command::Screenshot => "screenshot".into(),
            Command::Quit => "quit".into(),
        }
    }

    fn to_params(&self) -> Option<serde_json::Value> {
        serde_json::to_value(self).ok()
            .and_then(|v| v.get("params").cloned())
            .or(match self {
                Command::RomLoad { path } => Some(serde_json::json!({"path": path})),
                Command::EmulatorReset { reload } => Some(serde_json::json!({"reload": reload})),
                Command::CpuRegistersRead { name } => {
                    name.as_ref().map(|n| serde_json::json!({"name": n}))
                }
                Command::CpuRegistersWrite { name, value } => {
                    Some(serde_json::json!({"name": name, "value": value}))
                }
                Command::MemoryRead { address, size, bank } => {
                    Some(serde_json::json!({"address": address, "size": size, "bank": bank}))
                }
                Command::MemoryWrite { address, data, bank } => {
                    Some(serde_json::json!({"address": address, "data": data, "bank": bank}))
                }
                Command::MemoryDump { address, size, bank } => {
                    Some(serde_json::json!({"address": address, "size": size, "bank": bank}))
                }
                Command::BreakpointAdd { address, bank, range_end, inclusive, condition } => {
                    Some(serde_json::json!({
                        "address": address,
                        "bank": bank,
                        "range_end": range_end,
                        "inclusive": inclusive,
                        "condition": condition,
                    }))
                }
                Command::BreakpointRemove { id } => {
                    id.map(|i| serde_json::json!({"id": i}))
                }
                Command::WatchpointAdd { address, bank, r#type: wp_type, condition } => {
                    Some(serde_json::json!({
                        "address": address,
                        "bank": bank,
                        "type": wp_type,
                        "condition": condition,
                    }))
                }
                Command::WatchpointRemove { id } => {
                    id.map(|i| serde_json::json!({"id": i}))
                }
                Command::Disassemble { address, count } => {
                    Some(serde_json::json!({"address": address, "count": count}))
                }
                Command::Eval { expression } => {
                    Some(serde_json::json!({"expression": expression}))
                }
                Command::StateSave { slot } | Command::StateLoad { slot } => {
                    Some(serde_json::json!({"slot": slot}))
                }
                Command::SymbolLoad { path } => {
                    Some(serde_json::json!({"path": path}))
                }
                Command::InputPress { key, state } => {
                    Some(serde_json::json!({"key": key, "state": state}))
                }
                Command::VramRead { offset, size } => {
                    Some(serde_json::json!({"offset": offset, "size": size}))
                }
                Command::VramTile { tile_id } => {
                    Some(serde_json::json!({"tile_id": tile_id}))
                }
                Command::OamRead { sprite_id } => {
                    sprite_id.map(|s| serde_json::json!({"sprite_id": s}))
                }
                Command::ContextSnapshot { address, range } => {
                    Some(serde_json::json!({"address": address, "range": range}))
                }
                _ => None,
            })
    }
}

/// Notification types from SameBoy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
pub enum Notification {
    #[serde(rename = "breakpoint.hit")]
    BreakpointHit {
        id: u32,
        address: u16,
        pc: u16,
        registers: serde_json::Value,
    },
    #[serde(rename = "watchpoint.hit")]
    WatchpointHit {
        id: u32,
        address: u16,
        r#type: String,
        old_value: Option<u8>,
        new_value: Option<u8>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_serialization() {
        let req = JsonRpcRequest {
            id: 1,
            method: "cpu.step".to_string(),
            params: None,
        };
        let json_str = serde_json::to_string(&req).unwrap();
        assert!(json_str.contains("\"id\":1"));
        assert!(json_str.contains("\"method\":\"cpu.step\""));
    }

    #[test]
    fn test_json_rpc_request_with_params() {
        let req = JsonRpcRequest {
            id: 2,
            method: "memory.read".to_string(),
            params: Some(serde_json::json!({"address": 0, "size": 16})),
        };
        let json_str = serde_json::to_string(&req).unwrap();
        assert!(json_str.contains("\"address\":0"));
        assert!(json_str.contains("\"size\":16"));
    }

    #[test]
    fn test_json_rpc_response_serialization() {
        let resp = JsonRpcResponse {
            id: 1,
            result: Some(serde_json::json!({"af": 0, "pc": 0})),
            error: None,
        };
        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(json_str.contains("\"id\":1"));
        assert!(json_str.contains("\"result\""));
    }

    #[test]
    fn test_json_rpc_error_response() {
        let resp = JsonRpcResponse {
            id: 1,
            result: None,
            error: Some("unknown method".to_string()),
        };
        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(json_str.contains("\"error\""));
        assert!(json_str.contains("\"unknown method\""));
    }

    #[test]
    fn test_json_rpc_notification_serialization() {
        let notif = JsonRpcNotification {
            method: "breakpoint.hit".to_string(),
            params: Some(serde_json::json!({"address": 0x100})),
        };
        let json_str = serde_json::to_string(&notif).unwrap();
        assert!(json_str.contains("\"method\":\"breakpoint.hit\""));
        assert!(json_str.contains("\"params\""));
    }

    #[test]
    fn test_request_deserialization() {
        let json_str = r#"{"id":1,"method":"cpu.registers.read","params":{"name":"a"}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.id, 1);
        assert_eq!(req.method, "cpu.registers.read");
        assert!(req.params.is_some());
    }

    #[test]
    fn test_response_deserialization() {
        let json_str = r#"{"id":1,"result":{"af":0,"pc":0}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_error_response_deserialization() {
        let json_str = r#"{"id":1,"error":"address out of range"}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json_str).unwrap();
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_none());
        assert_eq!(resp.error, Some("address out of range".to_string()));
    }

    #[test]
    fn test_notification_deserialization() {
        let json_str = r#"{"method":"breakpoint.hit","params":{"address":256}}"#;
        let notif: JsonRpcNotification = serde_json::from_str(json_str).unwrap();
        assert_eq!(notif.method, "breakpoint.hit");
        assert!(notif.params.is_some());
    }

    #[test]
    fn test_request_without_params() {
        let json_str = r#"{"id":1,"method":"quit"}"#;
        let req: JsonRpcRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(req.id, 1);
        assert_eq!(req.method, "quit");
        assert!(req.params.is_none());
    }
}
