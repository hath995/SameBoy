// SameBoy JSON Mode — JSON-RPC frontend for headless emulation
// Reads JSON commands from stdin, returns JSON responses to stdout
// Optional SDL display for visual feedback

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <stdint.h>
#include <unistd.h>
#include <signal.h>
#include <errno.h>
#include <fcntl.h>
#include <SDL.h>
#include <png.h>

#include "Core/gb.h"
#include "Core/memory.h"
#include "Core/display.h"
#include "configuration.h"
#include "audio/audio.h"
#include "utils.h"
#include "json_mode.h"

// Core/defs.h defines `noinline` as an attribute macro which recursively
// breaks yyjson's own `__attribute__((noinline))` expansion. This translation
// unit does not use Core's `noinline`, so drop it for yyjson.
#undef noinline
#include "lib/yyjson/yyjson.h"

// Configuration struct required by audio subsystem
configuration_t configuration = { 0 };

// Global gameboy instance
GB_gameboy_t gb;

static volatile sig_atomic_t json_shutdown_requested = 0;
static bool json_running = false; // When true, main loop runs frames
static bool json_breakpoint_hit = false; // Set when debugger stops at a breakpoint
static char *current_rom = NULL;

static void json_signal_handler(int sig)
{
    (void) sig;
    json_shutdown_requested = 1;
    // Waking the loop via a self-pipe would be ideal, but the loop
    // returns from GB_run at least every frame, so the flag is enough.
}

static uint32_t screen_buffer[160 * 144];

// PNG memory write callback
typedef struct {
    uint8_t *buffer;
    size_t size;
    size_t pos;
} png_mem_write_t;

static void png_mem_write(png_structp png, png_bytep data, png_size_t length)
{
    png_mem_write_t *mem = png_get_io_ptr(png);
    if (mem->pos + length > mem->size) {
        mem->size *= 2;
        mem->buffer = realloc(mem->buffer, mem->size);
    }
    memcpy(mem->buffer + mem->pos, data, length);
    mem->pos += length;
}

// SDL display state
static SDL_Window *json_window = NULL;
static SDL_Renderer *json_renderer = NULL;
static SDL_Texture *json_texture = NULL;
static SDL_PixelFormat *json_pixel_format = NULL;
static bool json_sdl_enabled = false;

static bool json_audio_enabled = false;

// Audio callback — queues samples to SDL audio hardware
static void json_audio_callback(GB_gameboy_t *gb, GB_sample_t *sample)
{
    if (GB_audio_get_queue_length() > GB_audio_get_frequency() / 8) {
        return;
    }
    GB_audio_queue_sample(sample);
}

// Default key mapping (same as regular SameBoy frontend)
static const SDL_Scancode json_key_map[GB_KEY_MAX] = {
    [GB_KEY_RIGHT] = SDL_SCANCODE_RIGHT,
    [GB_KEY_LEFT] = SDL_SCANCODE_LEFT,
    [GB_KEY_UP] = SDL_SCANCODE_UP,
    [GB_KEY_DOWN] = SDL_SCANCODE_DOWN,
    [GB_KEY_A] = SDL_SCANCODE_X,
    [GB_KEY_B] = SDL_SCANCODE_Z,
    [GB_KEY_SELECT] = SDL_SCANCODE_BACKSPACE,
    [GB_KEY_START] = SDL_SCANCODE_RETURN,
};

static void json_sdl_init(void)
{
    if (SDL_Init(SDL_INIT_VIDEO) < 0) {
        fprintf(stderr, "SDL init failed: %s\n", SDL_GetError());
        return;
    }

    json_window = SDL_CreateWindow("SameBoy JSON", SDL_WINDOWPOS_UNDEFINED, SDL_WINDOWPOS_UNDEFINED,
                                   160 * 3, 144 * 3, SDL_WINDOW_HIDDEN);
    if (!json_window) {
        fprintf(stderr, "SDL window creation failed: %s\n", SDL_GetError());
        SDL_Quit();
        return;
    }

    json_renderer = SDL_CreateRenderer(json_window, -1, SDL_RENDERER_ACCELERATED | SDL_RENDERER_PRESENTVSYNC);
    if (!json_renderer) {
        fprintf(stderr, "SDL renderer creation failed: %s\n", SDL_GetError());
        SDL_DestroyWindow(json_window);
        SDL_Quit();
        return;
    }

    json_texture = SDL_CreateTexture(json_renderer, SDL_GetWindowPixelFormat(json_window),
                                     SDL_TEXTUREACCESS_STREAMING, 160, 144);
    if (!json_texture) {
        fprintf(stderr, "SDL texture creation failed: %s\n", SDL_GetError());
        SDL_DestroyRenderer(json_renderer);
        SDL_DestroyWindow(json_window);
        SDL_Quit();
        return;
    }

    json_pixel_format = SDL_AllocFormat(SDL_GetWindowPixelFormat(json_window));
    json_sdl_enabled = true;
}

static void json_audio_init(void)
{
    if (GB_audio_init()) {
        GB_set_sample_rate(&gb, GB_audio_get_frequency());
        GB_apu_set_sample_callback(&gb, json_audio_callback);
        GB_audio_set_paused(false);
        json_audio_enabled = true;
        fprintf(stderr, "Audio initialized: %s (%u Hz)\n", GB_audio_driver_name(), GB_audio_get_frequency());
    }
    else {
        fprintf(stderr, "Audio initialization failed, continuing without audio\n");
    }
}

// Render current frame to SDL window and process events
static void json_sdl_render(void)
{
    if (!json_sdl_enabled || !json_texture) return;

    SDL_UpdateTexture(json_texture, NULL, screen_buffer, 160 * sizeof(uint32_t));
    SDL_RenderClear(json_renderer);
    SDL_RenderCopy(json_renderer, json_texture, NULL, NULL);
    SDL_RenderPresent(json_renderer);

    SDL_Event event;
    while (SDL_PollEvent(&event)) {
        if (event.type == SDL_KEYDOWN || event.type == SDL_KEYUP) {
            bool pressed = (event.type == SDL_KEYDOWN);
            for (unsigned i = 0; i < GB_KEY_MAX; i++) {
                if (event.key.keysym.scancode == json_key_map[i]) {
                    GB_set_key_state(&gb, i, pressed);
                    break;
                }
            }
        }
        if (event.type == SDL_QUIT) {
            json_shutdown_requested = 1;
        }
    }
}

static void json_sdl_cleanup(void)
{
    if (json_texture) SDL_DestroyTexture(json_texture);
    if (json_renderer) SDL_DestroyRenderer(json_renderer);
    if (json_window) SDL_DestroyWindow(json_window);
    if (json_pixel_format) SDL_FreeFormat(json_pixel_format);
    if (json_sdl_enabled) SDL_Quit();
    json_sdl_enabled = false;
    json_pixel_format = NULL;
}

// RGB encoder — uses SDL_MapRGB for correct pixel format
static uint32_t json_rgb_encode(GB_gameboy_t *gb, uint8_t r, uint8_t g, uint8_t b)
{
    (void) gb;
    if (json_pixel_format) return SDL_MapRGB(json_pixel_format, r, g, b);
    return ((uint32_t) r << 16) | ((uint32_t) g << 8) | (uint32_t) b | 0xFF000000;
}

// ---------------------------------------------------------------------------
// JSON (yyjson) — request param accessors and response builders
// ---------------------------------------------------------------------------

static char *json_exec_debugger_cmd(const char *cmd);

// Read a number param (0 if missing or not a number)
static double json_pnum(yyjson_val *params, const char *key, double defval)
{
    yyjson_val *v = params ? yyjson_obj_get(params, key) : NULL;
    return (v && yyjson_is_num(v)) ? yyjson_get_num(v) : defval;
}

// Read a string param (borrowed pointer, valid while the request is alive)
static const char *json_pstr(yyjson_val *params, const char *key, const char *defval)
{
    yyjson_val *v = params ? yyjson_obj_get(params, key) : NULL;
    return (v && yyjson_is_str(v)) ? yyjson_get_str(v) : defval;
}

// Read a bool param
static bool json_pbool(yyjson_val *params, const char *key, bool defval)
{
    yyjson_val *v = params ? yyjson_obj_get(params, key) : NULL;
    return (v && yyjson_is_bool(v)) ? yyjson_get_bool(v) : defval;
}

// Key present and not null
static bool json_phas(yyjson_val *params, const char *key)
{
    yyjson_val *v = params ? yyjson_obj_get(params, key) : NULL;
    return (v != NULL) && !yyjson_is_null(v);
}

// New doc containing a response frame {"id":id}; caller adds "result"/"error"
static yyjson_mut_doc *json_frame_doc(unsigned id)
{
    yyjson_mut_doc *doc = yyjson_mut_doc_new(NULL);
    if (doc) {
        yyjson_mut_val *root = yyjson_mut_obj(doc);
        yyjson_mut_doc_set_root(doc, root);
        yyjson_mut_obj_add_uint(doc, root, "id", id);
    }
    return doc;
}

// New doc containing a notification frame {"method":method}
static yyjson_mut_doc *json_notification_doc(const char *method)
{
    yyjson_mut_doc *doc = yyjson_mut_doc_new(NULL);
    if (doc) {
        yyjson_mut_val *root = yyjson_mut_obj(doc);
        yyjson_mut_doc_set_root(doc, root);
        yyjson_mut_obj_add_str(doc, root, "method", method);
    }
    return doc;
}

// Serialize a frame doc to stdout, newline-terminated, and free it
static void json_send_doc(yyjson_mut_doc *doc)
{
    if (!doc) return;
    char *s = yyjson_mut_write(doc, 0, NULL);
    if (s) {
        fputs(s, stdout);
        fputc('\n', stdout);
        fflush(stdout);
        free(s);
    }
    yyjson_mut_doc_free(doc);
}

// Send {"id":id,"result":<result>} (null result if result is NULL)
static void json_send_result(yyjson_mut_doc *doc, yyjson_mut_val *result)
{
    if (!doc) return;
    yyjson_mut_val *root = yyjson_mut_doc_get_root(doc);
    if (result) yyjson_mut_obj_add_val(doc, root, "result", result);
    else yyjson_mut_obj_add_null(doc, root, "result");
    json_send_doc(doc);
}

// Send {"id":id,"result":"str"} (string is copied into the doc)
static void json_send_string(unsigned id, const char *str)
{
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return;
    yyjson_mut_val *root = yyjson_mut_doc_get_root(doc);
    if (str) yyjson_mut_obj_add_strcpy(doc, root, "result", str);
    else yyjson_mut_obj_add_null(doc, root, "result");
    json_send_doc(doc);
}

// Send {"id":id,"error":"msg"} (message is copied and escaped properly)
static void json_send_error(unsigned id, const char *msg)
{
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return;
    yyjson_mut_obj_add_strcpy(doc, yyjson_mut_doc_get_root(doc), "error",
                              msg ? msg : "error");
    json_send_doc(doc);
}

// Run a debugger command and send its output as {"id":id,"result":{key: out}}
static void json_send_debugger_output(unsigned id, const char *cmd, const char *key)
{
    char *result = json_exec_debugger_cmd(cmd);
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (doc) {
        yyjson_mut_val *r = yyjson_mut_obj(doc);
        yyjson_mut_obj_add_strcpy(doc, r, key, result ? result : "");
        json_send_result(doc, r);
    }
    free(result);
}

// Capture log output
static char *captured_log = NULL;

static void json_log_callback(GB_gameboy_t *gb, const char *string, GB_log_attributes_t attributes)
{
    (void) gb;
    (void) attributes;
    // Send debugger log to stderr (not stdout) to avoid mixing with JSON
    fputs(string, stderr);
    // Also capture for context commands
    size_t current_len = captured_log ? strlen(captured_log) : 0;
    size_t len_to_add = strlen(string);
    captured_log = realloc(captured_log, current_len + len_to_add + 1);
    memcpy(captured_log + current_len, string, len_to_add);
    captured_log[current_len + len_to_add] = '\0';
}

// Execute a debugger command and capture its output; caller must free the result
static char *json_exec_debugger_cmd(const char *cmd)
{
    free(captured_log);
    captured_log = malloc(1);
    captured_log[0] = '\0';

    char *cmd_copy = strdup(cmd);
    GB_debugger_execute_command(&gb, cmd_copy);

    char *result = captured_log;
    captured_log = NULL;
    return result;
}

static yyjson_mut_val *json_registers_val(yyjson_mut_doc *doc, GB_gameboy_t *gb)
{
    GB_registers_t *regs = GB_get_registers(gb);
    yyjson_mut_val *v = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_uint(doc, v, "af", regs->af);
    yyjson_mut_obj_add_uint(doc, v, "bc", regs->bc);
    yyjson_mut_obj_add_uint(doc, v, "de", regs->de);
    yyjson_mut_obj_add_uint(doc, v, "hl", regs->hl);
    yyjson_mut_obj_add_uint(doc, v, "sp", regs->sp);
    yyjson_mut_obj_add_uint(doc, v, "pc", regs->pc);
    yyjson_mut_obj_add_uint(doc, v, "ime", gb->ime);
    return v;
}

static const char *const json_register_names[] = {
    "a", "f", "b", "c", "d", "e", "h", "l", "af", "bc", "de", "hl", "sp", "pc",
};

static const char *json_canonical_register_name(const char *name)
{
    for (unsigned i = 0; i < sizeof(json_register_names) / sizeof(json_register_names[0]); i++) {
        if (strcmp(name, json_register_names[i]) == 0) return json_register_names[i];
    }
    return NULL;
}

static uint16_t json_get_register(GB_gameboy_t *gb, const char *name)
{
    GB_registers_t *regs = GB_get_registers(gb);
    switch (name[0]) {
        case 'a': return name[1] == 'f' ? regs->af : regs->a;
        case 'f': return regs->f;
        case 'b': return name[1] == 'c' ? regs->bc : regs->b;
        case 'c': return regs->c;
        case 'd': return name[1] == 'e' ? regs->de : regs->d;
        case 'e': return regs->e;
        case 'h': return name[1] == 'l' ? regs->hl : regs->h;
        case 'l': return regs->l;
        case 's': return regs->sp;
        case 'p': return regs->pc;
    }
    return 0;
}

// VRAM tile as structured data: {"tile_id":N,"pixels":[[8],[8]...]}
static yyjson_mut_val *json_vram_tile_val(yyjson_mut_doc *doc, GB_gameboy_t *gb, uint8_t tile_id)
{
    uint8_t *vram = GB_get_direct_access(gb, GB_DIRECT_ACCESS_VRAM, NULL, NULL);
    // CGB has 2 banks, 256 tiles each
    uint16_t offset = tile_id * 16;
    if (!vram || offset > 0x3FF0) return yyjson_mut_null(doc);

    yyjson_mut_val *v = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_uint(doc, v, "tile_id", tile_id);
    yyjson_mut_val *pixels = yyjson_mut_arr(doc);
    for (unsigned y = 0; y < 8; y++) {
        yyjson_mut_val *row = yyjson_mut_arr(doc);
        for (unsigned x = 0; x < 8; x++) {
            // Each row is two bitplanes, bit 7 - x of each
            uint8_t lo = vram[offset + y * 2];
            uint8_t hi = vram[offset + y * 2 + 1];
            uint8_t pixel = ((hi >> (7 - x)) & 1) | (((lo >> (7 - x)) & 1) << 1);
            yyjson_mut_arr_add_uint(doc, row, pixel);
        }
        yyjson_mut_arr_add_val(pixels, row);
    }
    yyjson_mut_obj_add_val(doc, v, "pixels", pixels);
    return v;
}

static yyjson_mut_val *json_oam_sprite_val(yyjson_mut_doc *doc, GB_gameboy_t *gb, uint8_t sprite_id)
{
    uint8_t *oam = GB_get_direct_access(gb, GB_DIRECT_ACCESS_OAM, NULL, NULL);
    if (!oam || sprite_id >= 40) return yyjson_mut_null(doc);

    uint8_t *s = &oam[sprite_id * 4];
    yyjson_mut_val *v = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_uint(doc, v, "id", sprite_id);
    yyjson_mut_obj_add_uint(doc, v, "y", (uint32_t) (s[0] - 16));
    yyjson_mut_obj_add_uint(doc, v, "x", (uint32_t) (s[1] - 8));
    yyjson_mut_obj_add_uint(doc, v, "tile", s[2]);
    yyjson_mut_val *attrs = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_uint(doc, attrs, "flip_y", (s[3] >> 6) & 1);
    yyjson_mut_obj_add_uint(doc, attrs, "flip_x", (s[3] >> 5) & 1);
    yyjson_mut_obj_add_uint(doc, attrs, "priority", (s[3] >> 4) & 1);
    yyjson_mut_obj_add_uint(doc, attrs, "palette", s[3] & 0xF);
    yyjson_mut_obj_add_val(doc, v, "attributes", attrs);
    return v;
}

// Context snapshot: registers + pc + disassembly
static yyjson_mut_val *json_context_snapshot_val(yyjson_mut_doc *doc, GB_gameboy_t *gb,
                                                 uint16_t addr, uint16_t range)
{
    yyjson_mut_val *v = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_val(doc, v, "registers", json_registers_val(doc, gb));
    yyjson_mut_obj_add_uint(doc, v, "pc", addr);
    char cmd[64];
    snprintf(cmd, sizeof(cmd), "disassemble $%04x/%u", addr, range * 2 + 1);
    char *disasm = json_exec_debugger_cmd(cmd);
    yyjson_mut_obj_add_strcpy(doc, v, "disassembly", disasm ? disasm : "");
    free(disasm);
    return v;
}

static void json_send_stop_notification(GB_gameboy_t *gb)
{
    yyjson_mut_doc *doc = json_notification_doc("debugger.stopped");
    if (!doc) return;
    GB_registers_t *regs = GB_get_registers(gb);
    yyjson_mut_val *params = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_uint(doc, params, "pc", regs->pc);
    yyjson_mut_obj_add_val(doc, params, "registers", json_registers_val(doc, gb));
    yyjson_mut_obj_add_val(doc, yyjson_mut_doc_get_root(doc), "params", params);
    json_send_doc(doc);
}

// Input callback (blocking — called when the debugger stops)
static char *json_input_callback(GB_gameboy_t *gb)
{
    (void) gb;
    // Return NULL to let the debugger exit the run loop
    return NULL;
}

static char *json_async_input_callback(GB_gameboy_t *gb)
{
    (void) gb;
    return NULL; // No async input in JSON mode
}

// Command handlers. Returning true requests shutdown after responding.

static bool handle_quit(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_string(id, "ok");
    return true;
}

static bool handle_rom_load(unsigned id, yyjson_val *params)
{
    const char *path = json_pstr(params, "path", "");
    if (!path[0]) {
        json_send_error(id, "missing path");
        return false;
    }
    if (GB_load_rom(&gb, path) != 0) {
        json_send_error(id, "failed to load ROM");
        return false;
    }
    free(current_rom);
    current_rom = strdup(path);
    json_send_string(id, "loaded");
    return false;
}

static bool handle_emulator_reset(unsigned id, yyjson_val *params)
{
    if (json_pbool(params, "reload", false)) {
        GB_reset(&gb);
    }
    else {
        GB_quick_reset(&gb);
    }
    json_send_string(id, "ok");
    return false;
}

static bool handle_emulator_pause(unsigned id, yyjson_val *params)
{
    (void) params;
    json_running = false;
    GB_debugger_break(&gb);
    json_send_string(id, "paused");
    return false;
}

static bool handle_emulator_resume(unsigned id, yyjson_val *params)
{
    (void) params;
    gb.debug_stopped = false;
    json_breakpoint_hit = false;
    json_running = true;
    json_send_string(id, "running");
    return false;
}

static bool handle_cpu_step(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "step", "output");
    return false;
}

static bool handle_cpu_next(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "next", "output");
    return false;
}

static bool handle_cpu_finish(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "finish", "output");
    return false;
}

static bool handle_cpu_backstep(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "backstep", "output");
    return false;
}

static bool handle_cpu_undo(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "undo", "output");
    return false;
}

static bool handle_registers_read(unsigned id, yyjson_val *params)
{
    const char *name = json_pstr(params, "name", NULL);
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return false;
    if (!name) {
        json_send_result(doc, json_registers_val(doc, &gb));
        return false;
    }
    const char *canonical = json_canonical_register_name(name);
    if (!canonical) {
        yyjson_mut_doc_free(doc);
        json_send_error(id, "unknown register");
        return false;
    }
    uint16_t value = json_get_register(&gb, canonical);
    char hex[16];
    snprintf(hex, sizeof(hex), "$%04x", value);
    yyjson_mut_val *result = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_uint(doc, result, canonical, value);
    yyjson_mut_obj_add_strcpy(doc, result, "hex", hex);
    json_send_result(doc, result);
    return false;
}

static bool handle_registers_write(unsigned id, yyjson_val *params)
{
    const char *name = json_pstr(params, "name", "");
    uint16_t value = (uint16_t) json_pnum(params, "value", 0);
    char cmd[64];
    snprintf(cmd, sizeof(cmd), "%s = $%04x", name, value);
    free(json_exec_debugger_cmd(cmd));
    json_send_string(id, "ok");
    return false;
}

static bool handle_memory_read(unsigned id, yyjson_val *params)
{
    uint16_t address = (uint16_t) json_pnum(params, "address", 0);
    uint16_t size = (uint16_t) json_pnum(params, "size", 1);
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return false;
    yyjson_mut_val *result = yyjson_mut_obj(doc);
    yyjson_mut_val *data = yyjson_mut_arr(doc);
    for (uint16_t i = 0; i < size && i < 65535; i++) {
        yyjson_mut_arr_add_uint(doc, data, GB_read_memory(&gb, address + i));
    }
    yyjson_mut_obj_add_val(doc, result, "data", data);
    json_send_result(doc, result);
    return false;
}

static bool handle_memory_write(unsigned id, yyjson_val *params)
{
    uint16_t address = (uint16_t) json_pnum(params, "address", 0);
    yyjson_val *data = params ? yyjson_obj_get(params, "data") : NULL;
    if (yyjson_is_arr(data)) {
        yyjson_val *item;
        size_t idx, max;
        yyjson_arr_foreach(data, idx, max, item) {
            GB_write_memory(&gb, (uint16_t) (address + idx), (uint8_t) yyjson_get_int(item));
        }
    }
    json_send_string(id, "ok");
    return false;
}

static bool handle_memory_dump(unsigned id, yyjson_val *params)
{
    uint16_t address = (uint16_t) json_pnum(params, "address", 0);
    uint16_t size = (uint16_t) json_pnum(params, "size", 16);
    char cmd[64];
    snprintf(cmd, sizeof(cmd), "examine $%04x/%u", address, size);
    json_send_debugger_output(id, cmd, "dump");
    return false;
}

static bool handle_breakpoint_add(unsigned id, yyjson_val *params)
{
    uint16_t address = (uint16_t) json_pnum(params, "address", 0);
    const char *condition = json_pstr(params, "condition", NULL);
    uint16_t range_end = (uint16_t) json_pnum(params, "range_end", 0);
    bool inclusive = json_pbool(params, "inclusive", false);

    char cmd[256];
    snprintf(cmd, sizeof(cmd), "breakpoint $%04x", address);
    if (range_end) {
        snprintf(cmd + strlen(cmd), sizeof(cmd) - strlen(cmd), " to $%04x%s",
                 range_end, inclusive ? " inclusive" : "");
    }
    if (condition) {
        snprintf(cmd + strlen(cmd), sizeof(cmd) - strlen(cmd), " if %s", condition);
    }

    free(json_exec_debugger_cmd(cmd));
    json_send_string(id, "ok");
    return false;
}

static bool handle_breakpoint_remove(unsigned id, yyjson_val *params)
{
    char cmd[64] = "delete";
    if (json_phas(params, "id")) {
        snprintf(cmd + strlen(cmd), sizeof(cmd) - strlen(cmd), " %u",
                 (unsigned) json_pnum(params, "id", 0));
    }
    free(json_exec_debugger_cmd(cmd));
    json_send_string(id, "ok");
    return false;
}

static bool handle_breakpoint_list(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "list", "breakpoints");
    return false;
}

static bool handle_watchpoint_add(unsigned id, yyjson_val *params)
{
    uint16_t address = (uint16_t) json_pnum(params, "address", 0);
    const char *type = json_pstr(params, "type", "w");
    const char *condition = json_pstr(params, "condition", NULL);

    char cmd[256];
    snprintf(cmd, sizeof(cmd), "watch /%s $%04x", type, address);
    if (condition) {
        snprintf(cmd + strlen(cmd), sizeof(cmd) - strlen(cmd), " if %s", condition);
    }

    free(json_exec_debugger_cmd(cmd));
    json_send_string(id, "ok");
    return false;
}

static bool handle_watchpoint_remove(unsigned id, yyjson_val *params)
{
    char cmd[64] = "unwatch";
    if (json_phas(params, "id")) {
        snprintf(cmd + strlen(cmd), sizeof(cmd) - strlen(cmd), " %u",
                 (unsigned) json_pnum(params, "id", 0));
    }
    free(json_exec_debugger_cmd(cmd));
    json_send_string(id, "ok");
    return false;
}

static bool handle_watchpoint_list(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "list", "watchpoints");
    return false;
}

static bool handle_disassemble(unsigned id, yyjson_val *params)
{
    bool has_address = json_phas(params, "address");
    uint16_t address = has_address ? (uint16_t) json_pnum(params, "address", 0) : GB_get_registers(&gb)->pc;
    uint16_t count = (uint16_t) json_pnum(params, "count", 5);

    char cmd[64];
    snprintf(cmd, sizeof(cmd), "disassemble $%04x/%u", address, count);
    json_send_debugger_output(id, cmd, "disassembly");
    return false;
}

static bool handle_eval(unsigned id, yyjson_val *params)
{
    const char *expression = json_pstr(params, "expression", "");
    uint16_t result, bank;
    if (GB_debugger_evaluate(&gb, expression, &result, &bank)) {
        yyjson_mut_doc *doc = json_frame_doc(id);
        if (doc) {
            yyjson_mut_val *r = yyjson_mut_obj(doc);
            yyjson_mut_obj_add_uint(doc, r, "result", result);
            char hex[16];
            snprintf(hex, sizeof(hex), "$%04x", result);
            yyjson_mut_obj_add_strcpy(doc, r, "hex", hex);
            yyjson_mut_obj_add_uint(doc, r, "bank", bank);
            json_send_result(doc, r);
        }
    }
    else {
        json_send_error(id, "evaluation failed");
    }
    return false;
}

static bool handle_backtrace(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "backtrace", "backtrace");
    return false;
}

static bool handle_state_save(unsigned id, yyjson_val *params)
{
    unsigned slot = (unsigned) json_pnum(params, "slot", 0);
    if (slot > 9) slot = 0;
    char cmd[64];
    snprintf(cmd, sizeof(cmd), "savestate %u", slot);
    free(json_exec_debugger_cmd(cmd));
    json_send_string(id, "ok");
    return false;
}

static bool handle_state_load(unsigned id, yyjson_val *params)
{
    unsigned slot = (unsigned) json_pnum(params, "slot", 0);
    if (slot > 9) slot = 0;
    char cmd[64];
    snprintf(cmd, sizeof(cmd), "loadstate %u", slot);
    free(json_exec_debugger_cmd(cmd));
    json_send_string(id, "ok");
    return false;
}

static bool handle_symbol_load(unsigned id, yyjson_val *params)
{
    const char *path = json_pstr(params, "path", "");
    GB_debugger_load_symbol_file(&gb, path);
    json_send_string(id, "ok");
    return false;
}

static bool handle_input_press(unsigned id, yyjson_val *params)
{
    unsigned key = (unsigned) json_pnum(params, "key", 0);
    bool state = json_pbool(params, "state", true);
    if (key < GB_KEY_MAX) {
        GB_set_key_state(&gb, key, state);
    }
    json_send_string(id, "ok");
    return false;
}

static bool handle_apu_state(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "apu", "apu");
    return false;
}

static bool handle_apu_wave(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "wave", "wave");
    return false;
}

static bool handle_lcd_state(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "lcd", "lcd");
    return false;
}

static bool handle_cartridge_info(unsigned id, yyjson_val *params)
{
    (void) params;
    json_send_debugger_output(id, "cartridge", "cartridge");
    return false;
}

static bool handle_vram_read(unsigned id, yyjson_val *params)
{
    uint16_t offset = (uint16_t) json_pnum(params, "offset", 0);
    uint16_t size = (uint16_t) json_pnum(params, "size", 256);
    uint8_t *vram = GB_get_direct_access(&gb, GB_DIRECT_ACCESS_VRAM, NULL, NULL);
    if (!vram) {
        json_send_error(id, "VRAM access failed");
        return false;
    }
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return false;
    yyjson_mut_val *result = yyjson_mut_obj(doc);
    yyjson_mut_val *data = yyjson_mut_arr(doc);
    for (uint16_t i = 0; i < size && offset + i < 0x2000; i++) {
        yyjson_mut_arr_add_uint(doc, data, vram[offset + i]);
    }
    yyjson_mut_obj_add_val(doc, result, "data", data);
    json_send_result(doc, result);
    return false;
}

static bool handle_vram_tile(unsigned id, yyjson_val *params)
{
    uint8_t tile_id = (uint8_t) json_pnum(params, "tile_id", 0);
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (doc) json_send_result(doc, json_vram_tile_val(doc, &gb, tile_id));
    return false;
}

static bool handle_vram_tiles(unsigned id, yyjson_val *params)
{
    (void) params;
    uint8_t *vram = GB_get_direct_access(&gb, GB_DIRECT_ACCESS_VRAM, NULL, NULL);
    if (!vram) {
        json_send_error(id, "VRAM access failed");
        return false;
    }
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return false;
    yyjson_mut_val *result = yyjson_mut_obj(doc);
    bool cgb = GB_is_cgb_in_cgb_mode(&gb);
    yyjson_mut_obj_add_uint(doc, result, "count", cgb ? 512 : 256);
    yyjson_mut_obj_add_uint(doc, result, "bank", cgb ? (vram[0x40] >> 7) : 0);
    yyjson_mut_val *data = yyjson_mut_arr(doc);
    for (size_t i = 0; i < 0x2000; i++) {
        yyjson_mut_arr_add_uint(doc, data, vram[i]);
    }
    yyjson_mut_obj_add_val(doc, result, "data", data);
    json_send_result(doc, result);
    return false;
}

static void json_add_all_sprites(yyjson_mut_doc *doc, yyjson_mut_val *sprites)
{
    for (uint8_t i = 0; i < 40; i++) {
        yyjson_mut_arr_add_val(sprites, json_oam_sprite_val(doc, &gb, i));
    }
}

static bool handle_oam_read(unsigned id, yyjson_val *params)
{
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return false;
    if (json_phas(params, "sprite_id")) {
        uint8_t sprite_id = (uint8_t) json_pnum(params, "sprite_id", 0);
        json_send_result(doc, json_oam_sprite_val(doc, &gb, sprite_id));
        return false;
    }
    yyjson_mut_val *result = yyjson_mut_obj(doc);
    yyjson_mut_val *sprites = yyjson_mut_arr(doc);
    json_add_all_sprites(doc, sprites);
    yyjson_mut_obj_add_val(doc, result, "sprites", sprites);
    json_send_result(doc, result);
    return false;
}

static bool handle_oam_list(unsigned id, yyjson_val *params)
{
    (void) params;
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return false;
    yyjson_mut_val *result = yyjson_mut_arr(doc);
    json_add_all_sprites(doc, result);
    json_send_result(doc, result);
    return false;
}

static bool handle_ppu_state(unsigned id, yyjson_val *params)
{
    (void) params;
    uint8_t lcdc = GB_read_memory(&gb, 0xFF40);
    uint8_t stat = GB_read_memory(&gb, 0xFF41);

    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return false;
    yyjson_mut_val *result = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_uint(doc, result, "lcdc", lcdc);
    yyjson_mut_obj_add_uint(doc, result, "stat", stat);
    yyjson_mut_obj_add_uint(doc, result, "scy", GB_read_memory(&gb, 0xFF42));
    yyjson_mut_obj_add_uint(doc, result, "scx", GB_read_memory(&gb, 0xFF43));
    yyjson_mut_obj_add_uint(doc, result, "ly", GB_read_memory(&gb, 0xFF44));
    yyjson_mut_obj_add_uint(doc, result, "lyc", GB_read_memory(&gb, 0xFF45));
    yyjson_mut_obj_add_uint(doc, result, "lcd_enabled", (lcdc >> 7) & 1);
    yyjson_mut_obj_add_uint(doc, result, "bg_enabled", (lcdc >> 0) & 1);
    yyjson_mut_obj_add_uint(doc, result, "sprites_enabled", (lcdc >> 1) & 1);
    yyjson_mut_obj_add_uint(doc, result, "window_enabled", (lcdc >> 5) & 1);
    yyjson_mut_obj_add_uint(doc, result, "mode", stat & 3);
    json_send_result(doc, result);
    return false;
}

static bool handle_ppu_palette(unsigned id, yyjson_val *params)
{
    (void) params;
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return false;
    yyjson_mut_val *result = yyjson_mut_obj(doc);
    yyjson_mut_obj_add_uint(doc, result, "bgp", GB_read_memory(&gb, 0xFF47));
    yyjson_mut_obj_add_uint(doc, result, "opb0", GB_read_memory(&gb, 0xFF48));
    yyjson_mut_obj_add_uint(doc, result, "opb1", GB_read_memory(&gb, 0xFF49));
    json_send_result(doc, result);
    return false;
}

static bool handle_context_snapshot(unsigned id, yyjson_val *params)
{
    uint16_t address = json_phas(params, "address") ?
        (uint16_t) json_pnum(params, "address", 0) : GB_get_registers(&gb)->pc;
    uint16_t range = (uint16_t) json_pnum(params, "range", 8);

    yyjson_mut_doc *doc = json_frame_doc(id);
    if (doc) json_send_result(doc, json_context_snapshot_val(doc, &gb, address, range));
    return false;
}

static bool handle_context_history(unsigned id, yyjson_val *params)
{
    (void) params;
    // For now, return the current context
    yyjson_mut_doc *doc = json_frame_doc(id);
    if (!doc) return false;
    yyjson_mut_val *result = yyjson_mut_obj(doc);
    yyjson_mut_val *snapshots = yyjson_mut_arr(doc);
    yyjson_mut_arr_add_val(snapshots,
                           json_context_snapshot_val(doc, &gb, GB_get_registers(&gb)->pc, 8));
    yyjson_mut_obj_add_val(doc, result, "snapshots", snapshots);
    json_send_result(doc, result);
    return false;
}

static bool handle_screenshot(unsigned id, yyjson_val *params)
{
    (void) params;
    uint32_t *pixels = GB_get_pixels_output(&gb);
    if (!pixels) {
        json_send_error(id, "no pixel buffer");
        return false;
    }
    uint16_t width = GB_get_screen_width(&gb);
    uint16_t height = GB_get_screen_height(&gb);

    // Convert pixels to RGB888 rows for PNG
    png_byte *rows[256];
    for (uint16_t y = 0; y < height; y++) {
        rows[y] = malloc(width * 3);
        for (uint16_t x = 0; x < width; x++) {
            uint32_t p = pixels[y * width + x];
            rows[y][x * 3] = (p >> 16) & 0xFF;
            rows[y][x * 3 + 1] = (p >> 8) & 0xFF;
            rows[y][x * 3 + 2] = p & 0xFF;
        }
    }

    png_structp png_ptr = png_create_write_struct(PNG_LIBPNG_VER_STRING, NULL, NULL, NULL);
    if (!png_ptr) {
        for (uint16_t y = 0; y < height; y++) free(rows[y]);
        json_send_error(id, "png create failed");
        return false;
    }
    png_infop info_ptr = png_create_info_struct(png_ptr);
    if (!info_ptr) {
        for (uint16_t y = 0; y < height; y++) free(rows[y]);
        png_destroy_write_struct(&png_ptr, NULL);
        json_send_error(id, "png info failed");
        return false;
    }

    png_mem_write_t mem_write = {malloc(65536), 65536, 0};
    png_set_write_fn(png_ptr, &mem_write, png_mem_write, NULL);

    png_set_IHDR(png_ptr, info_ptr, width, height, 8, PNG_COLOR_TYPE_RGB,
                 PNG_INTERLACE_NONE, PNG_COMPRESSION_TYPE_DEFAULT, PNG_FILTER_TYPE_DEFAULT);
    png_write_info(png_ptr, info_ptr);
    png_write_image(png_ptr, rows);
    png_write_end(png_ptr, NULL);

    // Base64 encode the PNG data
    const char b64_chars[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    size_t b64_len = 4 * ((mem_write.pos + 2) / 3) + 1;
    char *b64 = malloc(b64_len);
    size_t out_pos = 0;
    for (size_t i = 0; i < mem_write.pos; i += 3) {
        uint32_t val = mem_write.buffer[i] << 16;
        if (i + 1 < mem_write.pos) val |= mem_write.buffer[i + 1] << 8;
        if (i + 2 < mem_write.pos) val |= mem_write.buffer[i + 2];
        b64[out_pos++] = b64_chars[(val >> 18) & 0x3F];
        b64[out_pos++] = b64_chars[(val >> 12) & 0x3F];
        b64[out_pos++] = i + 1 < mem_write.pos ? b64_chars[(val >> 6) & 0x3F] : '=';
        b64[out_pos++] = i + 2 < mem_write.pos ? b64_chars[val & 0x3F] : '=';
    }
    b64[out_pos] = '\0';

    yyjson_mut_doc *doc = json_frame_doc(id);
    if (doc) {
        yyjson_mut_val *result = yyjson_mut_obj(doc);
        yyjson_mut_obj_add_strcpy(doc, result, "png", b64);
        yyjson_mut_obj_add_uint(doc, result, "width", width);
        yyjson_mut_obj_add_uint(doc, result, "height", height);
        json_send_result(doc, result);
    }

    for (uint16_t y = 0; y < height; y++) free(rows[y]);
    png_destroy_write_struct(&png_ptr, &info_ptr);
    free(mem_write.buffer);
    free(b64);
    return false;
}

typedef bool (*json_command_handler_t)(unsigned id, yyjson_val *params);

typedef struct {
    const char *name;
    json_command_handler_t handler;
} json_command_t;

static const json_command_t json_commands[] = {
    {"quit", handle_quit},
    {"rom.load", handle_rom_load},
    {"emulator.reset", handle_emulator_reset},
    {"emulator.pause", handle_emulator_pause},
    {"emulator.resume", handle_emulator_resume},
    {"cpu.step", handle_cpu_step},
    {"cpu.next", handle_cpu_next},
    {"cpu.finish", handle_cpu_finish},
    {"cpu.backstep", handle_cpu_backstep},
    {"cpu.undo", handle_cpu_undo},
    {"cpu.registers.read", handle_registers_read},
    {"cpu.registers.write", handle_registers_write},
    {"memory.read", handle_memory_read},
    {"memory.write", handle_memory_write},
    {"memory.dump", handle_memory_dump},
    {"breakpoint.add", handle_breakpoint_add},
    {"breakpoint.remove", handle_breakpoint_remove},
    {"breakpoint.list", handle_breakpoint_list},
    {"watchpoint.add", handle_watchpoint_add},
    {"watchpoint.remove", handle_watchpoint_remove},
    {"watchpoint.list", handle_watchpoint_list},
    {"disassemble", handle_disassemble},
    {"eval", handle_eval},
    {"backtrace", handle_backtrace},
    {"state.save", handle_state_save},
    {"state.load", handle_state_load},
    {"symbol.load", handle_symbol_load},
    {"input.press", handle_input_press},
    {"apu.state", handle_apu_state},
    {"apu.wave", handle_apu_wave},
    {"lcd.state", handle_lcd_state},
    {"cartridge.info", handle_cartridge_info},
    {"vram.read", handle_vram_read},
    {"vram.tile", handle_vram_tile},
    {"vram.tiles", handle_vram_tiles},
    {"oam.read", handle_oam_read},
    {"oam.list", handle_oam_list},
    {"ppu.state", handle_ppu_state},
    {"ppu.palette", handle_ppu_palette},
    {"context.snapshot", handle_context_snapshot},
    {"context.history", handle_context_history},
    {"screenshot", handle_screenshot},
};

// Parse a JSON-RPC request line and dispatch to its handler.
// Returns true if the command loop should stop.
static bool json_handle_command(const char *line)
{
    yyjson_doc *doc = yyjson_read(line, strlen(line), 0);
    yyjson_val *root = doc ? yyjson_doc_get_root(doc) : NULL;
    if (!root || !yyjson_is_obj(root)) {
        json_send_error(0, "invalid json");
        yyjson_doc_free(doc);
        return false;
    }
    unsigned id = (unsigned) yyjson_get_uint(yyjson_obj_get(root, "id"));
    const char *method = yyjson_get_str(yyjson_obj_get(root, "method"));
    if (!method) {
        json_send_error(id, "missing method");
        yyjson_doc_free(doc);
        return false;
    }
    yyjson_val *params = yyjson_obj_get(root, "params");
    if (!yyjson_is_obj(params)) params = NULL;

    bool stop = false;
    bool matched = false;
    for (const json_command_t *command = json_commands;
         command < json_commands + sizeof(json_commands) / sizeof(json_commands[0]); command++) {
        if (strcmp(method, command->name) == 0) {
            matched = true;
            stop = command->handler(id, params);
            break;
        }
    }
    if (!matched) json_send_error(id, "unknown method");

    yyjson_doc_free(doc);
    return stop;
}

static void json_boot_rom_callback(GB_gameboy_t *gb, GB_boot_rom_t type)
{
    static const char *const names[] = {
        [GB_BOOT_ROM_DMG_0] = "dmg0_boot.bin",
        [GB_BOOT_ROM_DMG] = "dmg_boot.bin",
        [GB_BOOT_ROM_CGB] = "cgb_boot.bin",
        [GB_BOOT_ROM_AGB] = "agb_boot.bin",
    };
    // CGB_E falls back to CGB
    if (type == GB_BOOT_ROM_CGB_E) type = GB_BOOT_ROM_CGB;
    if ((size_t) type < sizeof(names) / sizeof(names[0])) {
        GB_load_boot_rom(gb, resource_path(names[type]));
    }
}

void json_mode_init(void)
{
    signal(SIGPIPE, SIG_IGN);
}

void json_mode_run(const char *rom_path)
{
    GB_init(&gb, GB_MODEL_CGB_E);
    GB_set_log_callback(&gb, json_log_callback);
    GB_set_input_callback(&gb, json_input_callback);
    GB_set_async_input_callback(&gb, json_async_input_callback);
    GB_set_boot_rom_load_callback(&gb, json_boot_rom_callback);
    // Disable debugger to prevent GB_run from blocking
    gb.debug_disable = true;

    // Initialize SDL display FIRST (needed for pixel format)
    json_sdl_init();

    // Register signal handlers AFTER SDL_Init (SDL may override them)
    signal(SIGTERM, json_signal_handler);
    signal(SIGINT, json_signal_handler);

    GB_set_rgb_encode_callback(&gb, json_rgb_encode);
    GB_set_pixels_output(&gb, screen_buffer);

    if (rom_path) {
        if (GB_load_rom(&gb, rom_path) != 0) {
            fprintf(stderr, "Failed to load ROM: %s\n", rom_path);
            return;
        }
        current_rom = strdup(rom_path);

        json_audio_init();

        // Run frames until the LCD is enabled, then start running
        for (unsigned i = 0; i < 10000; i++) {
            GB_run_frame(&gb);
            json_sdl_render();
            if (GB_read_memory(&gb, 0xFF40) & 0x80) break;
        }
        json_running = true;
    }

    {
        yyjson_mut_doc *doc = json_notification_doc("ready");
        if (doc) {
            yyjson_mut_val *params = yyjson_mut_obj(doc);
            if (rom_path) yyjson_mut_obj_add_str(doc, params, "rom", "loaded");
            yyjson_mut_obj_add_val(doc, yyjson_mut_doc_get_root(doc), "params", params);
            json_send_doc(doc);
        }
    }
    if (json_window) SDL_ShowWindow(json_window);

    // Command loop — non-blocking stdin so the emulator and SDL stay responsive
    int flags = fcntl(STDIN_FILENO, F_GETFL, 0);
    fcntl(STDIN_FILENO, F_SETFL, flags | O_NONBLOCK);

    static char line[65536];
    size_t line_pos = 0;
    while (!json_shutdown_requested) {
        // Run one frame worth of instructions with real-time pacing
        if (json_running && !json_breakpoint_hit) {
            do {
                GB_run(&gb);
                if (json_breakpoint_hit) break;
            } while (!gb.vblank_just_occured);
            if (json_breakpoint_hit) {
                json_send_stop_notification(&gb);
                json_running = false;
                json_breakpoint_hit = false;
            }
        }

        json_sdl_render();

        char c;
        ssize_t n;
        bool input_eof = false;
        while ((n = read(STDIN_FILENO, &c, 1)) != 0) {
            if (n < 0) {
                if (errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR) break;
                input_eof = true;
                break;
            }
            if (c == '\n') {
                line[line_pos] = '\0';
                if (line_pos > 0 && json_handle_command(line)) {
                    json_shutdown_requested = 1;
                }
                line_pos = 0;
            }
            else if (c != '\r' && line_pos < sizeof(line) - 1) {
                line[line_pos++] = c;
            }
        }
        if (n == 0 || input_eof) break; // stdin closed

        // When idle, sleep to avoid busy-waiting
        if (!json_running) {
            usleep(50000); // 50ms
        }
    }
}

void json_mode_cleanup(void)
{
    if (json_audio_enabled) {
        GB_audio_deinit();
        json_audio_enabled = false;
    }
    json_sdl_cleanup();
    free(captured_log);
    captured_log = NULL;
    free(current_rom);
    current_rom = NULL;
    GB_free(&gb);
}

void json_mode_usage(void)
{
    fprintf(stderr, "Usage: sameboy-json [rom_path]\n");
    fprintf(stderr, "Reads JSON commands from stdin, writes JSON responses to stdout.\n");
}

int main(int argc, char *argv[])
{
    json_mode_init();

    const char *rom_path = NULL;
    if (argc > 1) {
        if (strcmp(argv[1], "--help") == 0 || strcmp(argv[1], "-h") == 0) {
            json_mode_usage();
            return 0;
        }
        rom_path = argv[1];
    }

    json_mode_run(rom_path);
    json_mode_cleanup();
    return 0;
}
