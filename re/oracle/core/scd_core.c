/* Dual-68000 core glue for the Snatcher test oracle (dev-only, never shipped).
 * Wraps Musashi (third_party/Musashi) with: two CPU contexts, 24-bit page tables
 * (64 KB pages: host RAM pointer or MMIO callback into Python), PC hooks,
 * level-triggered IRQ lines with acknowledge callback. */
#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include "m68k.h"

typedef uint32_t (*mmio_r_t)(int cpu, uint32_t addr, int size);
typedef void (*mmio_w_t)(int cpu, uint32_t addr, int size, uint32_t val);
typedef void (*hook_t)(int cpu, uint32_t pc);
typedef void (*ack_t)(int cpu, int level);

static uint8_t *pages[2][256];
static mmio_r_t mmio_r;
static mmio_w_t mmio_w;
static hook_t hook_cb;
static ack_t ack_cb;
static uint8_t *ctx[2];
static int cur = 0;
static int running = -1;
static uint32_t hooks[2][128];
static int nhooks[2];
static int irq_level[2];
static uint8_t hookpage[2][256];  /* page has hooks */

void core_init(mmio_r_t r, mmio_w_t w, hook_t h, ack_t a) {
    mmio_r = r; mmio_w = w; hook_cb = h; ack_cb = a;
    m68k_init();
    m68k_set_cpu_type(M68K_CPU_TYPE_68000);
    for (int c = 0; c < 2; c++) {
        ctx[c] = calloc(1, m68k_context_size());
        m68k_pulse_reset();
        m68k_get_context(ctx[c]);
        memset(pages[c], 0, sizeof(pages[c]));
        nhooks[c] = 0; irq_level[c] = 0;
    }
    m68k_set_context(ctx[0]);
    cur = 0;
}

static void activate(int c) {
    if (cur == c) return;
    m68k_get_context(ctx[cur]);
    m68k_set_context(ctx[c]);
    cur = c;
}

void core_map(int c, uint32_t start, uint32_t size, uint8_t *ptr) {
    for (uint32_t a = start; a < start + size; a += 0x10000)
        pages[c][(a >> 16) & 0xFF] = ptr ? ptr + (a - start) : NULL;
}

void core_add_hook(int c, uint32_t pc) {
    if (nhooks[c] < 128) { hooks[c][nhooks[c]++] = pc & 0xFFFFFF; hookpage[c][(pc >> 16) & 0xFF] = 1; }
}

/* ---- Musashi memory callbacks ---- */
static inline uint8_t *hp(uint32_t a) { return pages[cur][(a >> 16) & 0xFF]; }

unsigned int m68k_read_memory_8(unsigned int a) {
    a &= 0xFFFFFF; uint8_t *p = hp(a);
    if (p) return p[a & 0xFFFF];
    return mmio_r(cur, a, 1) & 0xFF;
}
unsigned int m68k_read_memory_16(unsigned int a) {
    a &= 0xFFFFFF; uint8_t *p = hp(a);
    if (p) { p += a & 0xFFFF; return (p[0] << 8) | p[1]; }
    return mmio_r(cur, a, 2) & 0xFFFF;
}
unsigned int m68k_read_memory_32(unsigned int a) {
    return (m68k_read_memory_16(a) << 16) | m68k_read_memory_16(a + 2);
}
void m68k_write_memory_8(unsigned int a, unsigned int v) {
    a &= 0xFFFFFF; uint8_t *p = hp(a);
    if (p) { p[a & 0xFFFF] = v; return; }
    mmio_w(cur, a, 1, v & 0xFF);
}
void m68k_write_memory_16(unsigned int a, unsigned int v) {
    a &= 0xFFFFFF; uint8_t *p = hp(a);
    if (p) { p += a & 0xFFFF; p[0] = v >> 8; p[1] = v; return; }
    mmio_w(cur, a, 2, v & 0xFFFF);
}
void m68k_write_memory_32(unsigned int a, unsigned int v) {
    m68k_write_memory_16(a, v >> 16);
    m68k_write_memory_16(a + 2, v & 0xFFFF);
}
unsigned int m68k_read_disassembler_8(unsigned int a) { return m68k_read_memory_8(a); }
unsigned int m68k_read_disassembler_16(unsigned int a) { return m68k_read_memory_16(a); }
unsigned int m68k_read_disassembler_32(unsigned int a) { return m68k_read_memory_32(a); }

int scd_int_ack(int level) {
    if (irq_level[cur] == level) { irq_level[cur] = 0; m68k_set_irq(0); }
    ack_cb(cur, level);
    return M68K_INT_ACK_AUTOVECTOR;
}

void scd_instr_hook(unsigned int pc) {
    pc &= 0xFFFFFF;
    if (!hookpage[cur][pc >> 16]) return;
    for (int i = 0; i < nhooks[cur]; i++)
        if (hooks[cur][i] == pc) { hook_cb(cur, pc); return; }
}

/* ---- API ---- */
int core_run(int c, int cycles) {
    activate(c);
    running = c;
    int n = m68k_execute(cycles);
    running = -1;
    return n;
}
void core_end_timeslice(void) { m68k_end_timeslice(); }
void core_set_irq(int c, int level) { activate(c); irq_level[c] = level; m68k_set_irq(level); }
int core_get_irq(int c) { return irq_level[c]; }
uint32_t core_get_reg(int c, int r) {
    if (c == cur) return m68k_get_reg(NULL, (m68k_register_t)r);
    return m68k_get_reg(ctx[c], (m68k_register_t)r);
}
void core_set_reg(int c, int r, uint32_t v) {
    int prev = cur;
    activate(c);
    m68k_set_reg((m68k_register_t)r, v);
    if (running < 0) activate(prev);
}
uint32_t core_read(int c, uint32_t a, int size) {
    int prev = cur; activate(c);
    uint32_t v = size == 1 ? m68k_read_memory_8(a) : size == 2 ? m68k_read_memory_16(a) : m68k_read_memory_32(a);
    if (running < 0) activate(prev);
    return v;
}
void core_write(int c, uint32_t a, int size, uint32_t v) {
    int prev = cur; activate(c);
    if (size == 1) m68k_write_memory_8(a, v); else if (size == 2) m68k_write_memory_16(a, v); else m68k_write_memory_32(a, v);
    if (running < 0) activate(prev);
}
unsigned int core_disasm(int c, uint32_t pc, char *buf) {
    int prev = cur; activate(c);
    unsigned int n = m68k_disassemble(buf, pc, M68K_CPU_TYPE_68000);
    if (running < 0) activate(prev);
    return n;
}
