// MCP resource handlers

use rust_mcp_sdk::schema::*;

pub fn list_resources() -> Vec<Resource> {
    vec![
        Resource {
            uri: "registers://".into(),
            name: "CPU Registers".into(),
            title: Some("CPU Registers".into()),
            description: Some("Current CPU register state".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            size: None,
            annotations: None,
        },
        Resource {
            uri: "cpu://".into(),
            name: "CPU State".into(),
            title: Some("CPU State".into()),
            description: Some("CPU flags, IME, ticks".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            size: None,
            annotations: None,
        },
        Resource {
            uri: "context://".into(),
            name: "Execution Context".into(),
            title: Some("Execution Context".into()),
            description: Some("Registers + disassembly around current PC".into()),
            mime_type: Some("text/plain".into()),
            icons: vec![],
            meta: None,
            size: None,
            annotations: None,
        },
        Resource {
            uri: "vram://".into(),
            name: "VRAM".into(),
            title: Some("VRAM".into()),
            description: Some("Full VRAM dump (tile data)".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            size: None,
            annotations: None,
        },
        Resource {
            uri: "oam://".into(),
            name: "OAM (Sprites)".into(),
            title: Some("OAM (Sprites)".into()),
            description: Some("All sprite attributes".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            size: None,
            annotations: None,
        },
        Resource {
            uri: "ppu://".into(),
            name: "PPU State".into(),
            title: Some("PPU State".into()),
            description: Some("LCD controller state and palette".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            size: None,
            annotations: None,
        },
        Resource {
            uri: "apu://".into(),
            name: "APU State".into(),
            title: Some("APU State".into()),
            description: Some("Audio processor state".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            size: None,
            annotations: None,
        },
        Resource {
            uri: "cartridge://".into(),
            name: "Cartridge Info".into(),
            title: Some("Cartridge Info".into()),
            description: Some("MBC type, ROM/RAM size, header info".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            size: None,
            annotations: None,
        },
    ]
}

pub fn list_resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate {
            uri_template: "memory://{bank}:{address}/{size}".into(),
            name: "Memory Region".into(),
            title: Some("Memory Region".into()),
            description: Some("Memory snapshot at given bank:address for size bytes".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            annotations: None,
        },
        ResourceTemplate {
            uri_template: "vram://tile/{id}".into(),
            name: "VRAM Tile".into(),
            title: Some("VRAM Tile".into()),
            description: Some("Single tile data from VRAM".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            annotations: None,
        },
        ResourceTemplate {
            uri_template: "oam://sprite/{id}".into(),
            name: "Sprite".into(),
            title: Some("Sprite".into()),
            description: Some("Single sprite attributes".into()),
            mime_type: Some("application/json".into()),
            icons: vec![],
            meta: None,
            annotations: None,
        },
    ]
}

/// Parse a resource URI and return the command method and params to fetch it
pub fn parse_resource_uri(uri: &str) -> Option<(String, Option<serde_json::Value>)> {
    if uri == "registers://" {
        return Some(("cpu.registers.read".into(), None));
    }
    if uri == "cpu://" {
        return Some(("cpu.registers.read".into(), None));
    }
    if uri == "context://" {
        return Some(("context.snapshot".into(), Some(serde_json::json!({"range": 8}))));
    }
    if uri == "vram://" {
        return Some(("vram.read".into(), Some(serde_json::json!({"offset": 0, "size": 8192}))));
    }
    if uri == "oam://" {
        return Some(("oam.list".into(), None));
    }
    if uri == "ppu://" {
        return Some(("ppu.state".into(), None));
    }
    if uri == "apu://" {
        return Some(("apu.state".into(), None));
    }
    if uri == "cartridge://" {
        return Some(("cartridge.info".into(), None));
    }
    
    // memory://bank:addr/size
    if uri.starts_with("memory://") {
        let rest = &uri[9..];
        if let Some(colon) = rest.find(':') {
            if let Some(slash) = rest.find('/') {
                let bank: u16 = rest[..colon].parse().ok()?;
                let addr: u16 = rest[colon+1..slash].parse().ok()?;
                let size: u16 = rest[slash+1..].parse().ok()?;
                return Some(("memory.read".into(), Some(serde_json::json!({
                    "bank": bank, "address": addr, "size": size
                }))));
            }
        }
    }
    
    // vram://tile/id
    if uri.starts_with("vram://tile/") {
        let id: u8 = uri[12..].parse().ok()?;
        return Some(("vram.tile".into(), Some(serde_json::json!({"tile_id": id}))));
    }
    
    // oam://sprite/id
    if uri.starts_with("oam://sprite/") {
        let id: u8 = uri[13..].parse().ok()?;
        return Some(("oam.read".into(), Some(serde_json::json!({"sprite_id": id}))));
    }
    
    None
}
