#!/usr/bin/env bash
# Build libscdcore.so (Musashi + glue) for the oracle. Musashi: re/oracle/third_party/Musashi
set -euo pipefail
cd "$(dirname "$0")"
M=../third_party/Musashi
mkdir -p build
cc -O2 -o build/m68kmake $M/m68kmake.c
(cd build && ./m68kmake . ../$M/m68k_in.c >/dev/null)
DEFS="-DM68K_EMULATE_010=0 -DM68K_EMULATE_EC020=0 -DM68K_EMULATE_020=0 -DM68K_EMULATE_030=0 -DM68K_EMULATE_040=0 \
 -DM68K_EMULATE_INT_ACK=2 -DM68K_INT_ACK_CALLBACK=scd_int_ack -DM68K_INSTRUCTION_HOOK=2 -DM68K_INSTRUCTION_CALLBACK=scd_instr_hook"
cat > build/m68kconf_extra.h <<EOH
int scd_int_ack(int level);
void scd_instr_hook(unsigned int pc);
EOH
cc -O2 -fPIC -shared -I build -I $M $DEFS -include build/m68kconf_extra.h \
  -o build/libscdcore.so scd_core.c $M/m68kcpu.c $M/m68kdasm.c build/m68kops.c $M/softfloat/softfloat.c -lm
echo built build/libscdcore.so
