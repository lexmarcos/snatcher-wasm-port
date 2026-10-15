/* oracle2 glue: a headless libretro frontend + accessors to the internals of Genesis Plus GX.
 * Dev-only test tool (the core has a non-commercial license: never link this into the web port).
 * Built into liboracle2.so together with the unmodified core objects (see build.sh).
 * All memory accessors return CANONICAL big-endian images (the core stores 16-bit word RAM byte-swapped on LSB_FIRST hosts). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdarg.h>
#include "libretro.h"
#include "shared.h"
#include "scd.h"
#include "genesis.h"
#include "vdp_ctrl.h"
#include "m68k.h"
#include "ym2612.h"
#include "input.h"

#define API __attribute__((visibility("default")))

/* libretro entry points (libretro.c) */
extern void retro_init(void);
extern void retro_deinit(void);
extern bool retro_load_game(const struct retro_game_info *game);
extern void retro_unload_game(void);
extern void retro_run(void);
extern void retro_reset(void);
extern void retro_get_system_av_info(struct retro_system_av_info *info);
extern size_t retro_serialize_size(void);
extern bool retro_serialize(void *data, size_t size);
extern bool retro_unserialize(const void *data, size_t size);
extern void retro_set_environment(retro_environment_t);
extern void retro_set_video_refresh(retro_video_refresh_t);
extern void retro_set_audio_sample(retro_audio_sample_t);
extern void retro_set_audio_sample_batch(retro_audio_sample_batch_t);
extern void retro_set_input_poll(retro_input_poll_t);
extern void retro_set_input_state(retro_input_state_t);
extern void retro_set_controller_port_device(unsigned port, unsigned device);

static char g_sysdir[1024], g_savedir[1024];
static int g_verbose;
static struct { char key[96]; char val[96]; } g_opts[64];
static int g_nopts;
static unsigned g_pixfmt = RETRO_PIXEL_FORMAT_RGB565;
static int g_vopt_changed;

/* video */
static const void *g_vdata; static unsigned g_vw, g_vh; static size_t g_vpitch;
static uint8_t *g_rgb; static size_t g_rgb_cap;
static unsigned g_frame_w, g_frame_h;
/* audio */
static int16_t *g_aud; static size_t g_aud_n, g_aud_cap;   /* in int16 units */
static double g_srate = 44100.0;
/* input */
static int g_pad[8];            /* libretro JOYPAD mask per port */
static int g_gun[8][6];         /* x(-0x7fff..0x7fff), y, trigger, auxa, auxb, start */
static int g_gun_off[8];
static int g_frames;

API void o2_set_option(const char *k, const char *v)
{
  int i;
  for (i = 0; i < g_nopts; i++) if (!strcmp(g_opts[i].key, k)) { snprintf(g_opts[i].val, 96, "%s", v); g_vopt_changed = 1; return; }
  if (g_nopts < 64) { snprintf(g_opts[g_nopts].key, 96, "%s", k); snprintf(g_opts[g_nopts].val, 96, "%s", v); g_nopts++; g_vopt_changed = 1; }
}

static void o2_log(enum retro_log_level level, const char *fmt, ...)
{
  va_list ap;
  if (!g_verbose && level < RETRO_LOG_WARN) return;
  va_start(ap, fmt); fputs("[core] ", stderr); vfprintf(stderr, fmt, ap); va_end(ap);
}

static bool env_cb(unsigned cmd, void *data)
{
  switch (cmd)
  {
    case RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY: *(const char **)data = g_sysdir; return true;
    case RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY: *(const char **)data = g_savedir; return true;
    case RETRO_ENVIRONMENT_GET_CORE_ASSETS_DIRECTORY: *(const char **)data = g_savedir; return true;
    case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT:
      g_pixfmt = *(unsigned *)data; return g_pixfmt == RETRO_PIXEL_FORMAT_RGB565 || g_pixfmt == RETRO_PIXEL_FORMAT_XRGB8888;
    case RETRO_ENVIRONMENT_GET_VARIABLE:
    {
      struct retro_variable *v = data; int i;
      for (i = 0; i < g_nopts; i++) if (!strcmp(g_opts[i].key, v->key)) { v->value = g_opts[i].val; return true; }
      v->value = NULL; return false;
    }
    case RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE: *(bool *)data = g_vopt_changed; g_vopt_changed = 0; return true;
    case RETRO_ENVIRONMENT_GET_LOG_INTERFACE: ((struct retro_log_callback *)data)->log = o2_log; return true;
    case RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION: *(unsigned *)data = 0; return true;
    case RETRO_ENVIRONMENT_GET_MESSAGE_INTERFACE_VERSION: *(unsigned *)data = 0; return true;
    case RETRO_ENVIRONMENT_GET_LANGUAGE: *(unsigned *)data = RETRO_LANGUAGE_ENGLISH; return true;
    case RETRO_ENVIRONMENT_GET_CAN_DUPE: *(bool *)data = true; return true;
    case RETRO_ENVIRONMENT_SET_VARIABLES:
    case RETRO_ENVIRONMENT_SET_CORE_OPTIONS:
    case RETRO_ENVIRONMENT_SET_CORE_OPTIONS_INTL:
    case RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS:
    case RETRO_ENVIRONMENT_SET_CONTROLLER_INFO:
    case RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME:
    case RETRO_ENVIRONMENT_SET_GEOMETRY:
    case RETRO_ENVIRONMENT_SET_MEMORY_MAPS:
    case RETRO_ENVIRONMENT_SET_SUBSYSTEM_INFO:
    case RETRO_ENVIRONMENT_SET_SERIALIZATION_QUIRKS:
    case RETRO_ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY:
    case RETRO_ENVIRONMENT_SET_CORE_OPTIONS_UPDATE_DISPLAY_CALLBACK:
      return true;
    default: return false;
  }
}

static void video_cb(const void *data, unsigned w, unsigned h, size_t pitch)
{
  if (data) { g_vdata = data; g_vw = w; g_vh = h; g_vpitch = pitch; }
  else if (!g_vdata) { g_vw = w; g_vh = h; }
}
static void audio_sample_cb(int16_t l, int16_t r)
{
  if (g_aud_n + 2 > g_aud_cap) { g_aud_cap = g_aud_cap ? g_aud_cap * 2 : 1 << 20; g_aud = realloc(g_aud, g_aud_cap * 2); }
  g_aud[g_aud_n++] = l; g_aud[g_aud_n++] = r;
}
static size_t audio_batch_cb(const int16_t *d, size_t frames)
{
  size_t n = frames * 2;
  if (g_aud_n + n > g_aud_cap) { while (g_aud_n + n > g_aud_cap) g_aud_cap = g_aud_cap ? g_aud_cap * 2 : 1 << 20; g_aud = realloc(g_aud, g_aud_cap * 2); }
  memcpy(g_aud + g_aud_n, d, n * 2); g_aud_n += n; return frames;
}
static void input_poll_cb(void) {}
static int16_t input_state_cb(unsigned port, unsigned device, unsigned index, unsigned id)
{
  if (port >= 8) return 0;
  if ((device & 0xff) == RETRO_DEVICE_JOYPAD && index == 0)
  {
    if (id == RETRO_DEVICE_ID_JOYPAD_MASK) return (int16_t)g_pad[port];
    return (g_pad[port] >> id) & 1;
  }
  if ((device & 0xff) == RETRO_DEVICE_LIGHTGUN && index == 0)
  {
    switch (id)
    {
      case RETRO_DEVICE_ID_LIGHTGUN_SCREEN_X: return g_gun[port][0];
      case RETRO_DEVICE_ID_LIGHTGUN_SCREEN_Y: return g_gun[port][1];
      case RETRO_DEVICE_ID_LIGHTGUN_IS_OFFSCREEN: return g_gun_off[port];
      case RETRO_DEVICE_ID_LIGHTGUN_TRIGGER: return g_gun[port][2];
      case RETRO_DEVICE_ID_LIGHTGUN_AUX_A: return g_gun[port][3];
      case RETRO_DEVICE_ID_LIGHTGUN_AUX_B: return g_gun[port][4];
      case RETRO_DEVICE_ID_LIGHTGUN_START: return g_gun[port][5];
    }
  }
  return 0;
}

/* ---- YM2612 write log (installed over the core's fm_write function pointer; no core edit) ---- */
typedef struct { uint32_t cyc; uint8_t addr, data; } ymw_t;
static ymw_t *g_ym; static int g_ym_n, g_ym_cap;
static void (*g_fm_orig)(unsigned int, unsigned int, unsigned int);
static int g_ym_log = 1;
static void fm_spy(unsigned int cycles, unsigned int address, unsigned int data)
{
  if (g_ym_log)
  {
    if (g_ym_n == g_ym_cap) { g_ym_cap = g_ym_cap ? g_ym_cap * 2 : 4096; g_ym = realloc(g_ym, g_ym_cap * sizeof(ymw_t)); }
    g_ym[g_ym_n].cyc = cycles; g_ym[g_ym_n].addr = address & 3; g_ym[g_ym_n].data = data; g_ym_n++;
  }
  g_fm_orig(cycles, address, data);
}
static void ym_install(void)
{
  if (fm_write != fm_spy) { g_fm_orig = fm_write; fm_write = fm_spy; }
}

/* ---- API ---- */
API int o2_init(const char *sysdir, const char *savedir, const char *content, int verbose)
{
  struct retro_game_info gi; struct retro_system_av_info av;
  snprintf(g_sysdir, sizeof g_sysdir, "%s", sysdir);
  snprintf(g_savedir, sizeof g_savedir, "%s", savedir);
  g_verbose = verbose;
  retro_set_environment(env_cb);
  retro_set_video_refresh(video_cb);
  retro_set_audio_sample(audio_sample_cb);
  retro_set_audio_sample_batch(audio_batch_cb);
  retro_set_input_poll(input_poll_cb);
  retro_set_input_state(input_state_cb);
  retro_init();
  memset(&gi, 0, sizeof gi); gi.path = content;
  if (!retro_load_game(&gi)) return -1;
  retro_get_system_av_info(&av);
  g_srate = av.timing.sample_rate;
  ym_install();
  return 0;
}
API void o2_deinit(void) { retro_unload_game(); retro_deinit(); }
API void o2_set_port_device(unsigned port, unsigned device) { retro_set_controller_port_device(port, device); }
API void o2_set_pad(int port, int mask) { if (port >= 0 && port < 8) g_pad[port] = mask; }
API void o2_set_gun(int port, int x, int y, int off, int trig, int aux_a, int aux_b, int start)
{
  if (port < 0 || port >= 8) return;
  g_gun[port][0] = x; g_gun[port][1] = y; g_gun_off[port] = off;
  g_gun[port][2] = trig; g_gun[port][3] = aux_a; g_gun[port][4] = aux_b; g_gun[port][5] = start;
}
API int o2_run(void) { g_ym_n = 0; ym_install(); retro_run(); g_frames++; return g_frames; }
API void o2_reset(void) { retro_reset(); }
API double o2_sample_rate(void) { return g_srate; }

/* video: RGB888, native size of the last frame */
API const uint8_t *o2_frame(int *w, int *h)
{
  unsigned x, y;
  size_t need = (size_t)g_vw * g_vh * 3;
  if (!g_vdata) { *w = *h = 0; return NULL; }
  if (need > g_rgb_cap) { g_rgb = realloc(g_rgb, need); g_rgb_cap = need; }
  for (y = 0; y < g_vh; y++)
    for (x = 0; x < g_vw; x++)
    {
      uint8_t *o = g_rgb + ((size_t)y * g_vw + x) * 3;
      if (g_pixfmt == RETRO_PIXEL_FORMAT_RGB565)
      {
        uint16_t p = ((const uint16_t *)((const uint8_t *)g_vdata + y * g_vpitch))[x];
        o[0] = ((p >> 11) & 31) * 255 / 31; o[1] = ((p >> 5) & 63) * 255 / 63; o[2] = (p & 31) * 255 / 31;
      }
      else
      {
        uint32_t p = ((const uint32_t *)((const uint8_t *)g_vdata + y * g_vpitch))[x];
        o[0] = p >> 16; o[1] = p >> 8; o[2] = p;
      }
    }
  *w = g_vw; *h = g_vh;
  return g_rgb;
}
/* audio taken since the last call (interleaved int16 stereo), returns frames */
API size_t o2_audio_take(int16_t *out, size_t max_frames)
{
  size_t n = g_aud_n / 2;
  if (!out) { size_t r = n; return r; }
  if (n > max_frames) n = max_frames;
  memcpy(out, g_aud, n * 4);
  memmove(g_aud, g_aud + n * 2, (g_aud_n - n * 2) * 2);
  g_aud_n -= n * 2;
  return n;
}
/* YM2612 writes of the last frame: triples (cycle, addr 0..3, data); the main-CPU cycle counter is in 68k master-clock/7 units */
API int o2_ym_count(void) { return g_ym_n; }
API void o2_ym_get(int i, uint32_t *cyc, uint8_t *addr, uint8_t *data) { *cyc = g_ym[i].cyc; *addr = g_ym[i].addr; *data = g_ym[i].data; }

/* ---- memory (canonical big-endian images) ---- */
static void swap_copy(uint8_t *dst, const uint8_t *src, size_t n) { size_t i; for (i = 0; i < n; i += 2) { dst[i] = src[i + 1]; dst[i + 1] = src[i]; } }
API void o2_read_main_ram(uint8_t *dst) { swap_copy(dst, work_ram, 0x10000); }          /* M:0xFF0000.. */
API void o2_read_prg_ram(uint8_t *dst) { swap_copy(dst, scd.prg_ram, 0x80000); }       /* S:0x000000.. */
API void o2_read_word_ram_2m(uint8_t *dst) { swap_copy(dst, scd.word_ram_2M, 0x40000); } /* M:0x200000.. / S:0x080000.. in 2M mode */
API void o2_read_word_ram_1m(int bank, uint8_t *dst) { swap_copy(dst, scd.word_ram[bank & 1], 0x20000); }
API void o2_read_vram(uint8_t *dst) { swap_copy(dst, vram, 0x10000); }
API void o2_read_cram(uint16_t *dst) { int i; for (i = 0; i < 64; i++) dst[i] = ((uint16_t *)cram)[i]; }   /* 64 words, MD 0BGR format */
API void o2_read_vsram(uint16_t *dst) { int i; for (i = 0; i < 40; i++) dst[i] = ((uint16_t *)vsram)[i]; }
API void o2_read_vdp_regs(uint8_t *dst) { memcpy(dst, reg, 0x20); }
API void o2_read_pcm_ram(uint8_t *dst) { memcpy(dst, scd.pcm_hw.ram, 0x10000); }
API int o2_vdp_status(void) { return status; }
/* gate array registers as 16-bit big-endian words: index = byte offset / 2 (0x00..0x7F) */
API unsigned o2_asic_reg(int i) { return scd.regs[i & 0xff].w; }
API unsigned o2_main_pc(void) { return m68k_get_reg(M68K_REG_PC); }
API unsigned o2_sub_pc(void) { return s68k_get_reg(M68K_REG_PC); }
API unsigned o2_main_reg(int r) { return m68k_get_reg((m68k_register_t)r); }
API unsigned o2_sub_reg(int r) { return s68k_get_reg((m68k_register_t)r); }
API unsigned o2_main_sr(void) { return m68k_get_reg(M68K_REG_SR); }
API unsigned o2_sub_sr(void) { return s68k_get_reg(M68K_REG_SR); }
API int o2_system_hw(void) { return system_hw; }
API int o2_region(void) { return region_code; }
API int o2_cd_type(void) { return scd.type; }

/* write into sub PRG-RAM / main RAM (canonical addresses), used by the scenario tools to inject state */
API void o2_write_prg(unsigned addr, const uint8_t *src, unsigned n) { unsigned i; for (i = 0; i < n; i++) scd.prg_ram[(addr + i) ^ 1] = src[i]; }
API void o2_write_main_ram(unsigned addr, const uint8_t *src, unsigned n) { unsigned i; for (i = 0; i < n; i++) work_ram[(addr + i) ^ 1] = src[i]; }
API void o2_write_word_ram_2m(unsigned addr, const uint8_t *src, unsigned n) { unsigned i; for (i = 0; i < n; i++) scd.word_ram_2M[(addr + i) ^ 1] = src[i]; }

/* save states */
API size_t o2_state_size(void) { return retro_serialize_size(); }
API int o2_state_save(void *buf, size_t n) { return retro_serialize(buf, n); }
API int o2_state_load(const void *buf, size_t n) { return retro_unserialize(buf, n); }
/* lightgun: what the core latched */
API void o2_input_dbg(int *x, int *y, int *dev) { *x = input.analog[4][0]; *y = input.analog[4][1]; *dev = input.dev[4]; }
