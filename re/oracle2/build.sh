#!/bin/sh
# Builds re/oracle2/build/liboracle2.so = unmodified Genesis Plus GX core objects + o2.c (our glue). Dev-only.
# usage: re/oracle2/build.sh   (clone first: git clone https://github.com/libretro/Genesis-Plus-GX re/oracle2/third_party/Genesis-Plus-GX)
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
G="$HERE/third_party/Genesis-Plus-GX"
cd "$G"
make -f Makefile.libretro platform=unix -j"$(nproc)" >/dev/null 2>&1 || make -f Makefile.libretro platform=unix -j"$(nproc)" 2>&1 | tail -5
mkdir -p "$HERE/build"
# flags = the ones Makefile.libretro uses for the core objects (so struct layouts and #defines match)
INC="-I./libretro/deps/libchdr/include -I./libretro/deps/lzma-19.00/include -I./libretro/deps/zlib-1.2.11 -I./libretro/deps/zstd/lib -I./core -I./core/z80 -I./core/m68k -I./core/ntsc -I./core/sound -I./core/sound/minimp3 -I./core/input_hw -I./core/cd_hw -I./core/cart_hw -I./core/cart_hw/svp -I./libretro -I./libretro/libretro-common/include"
DEF="-DUSE_LIBTREMOR -DUSE_LIBCHDR -DUSE_PER_SOUND_CHANNELS_CONFIG -D_7ZIP_ST -DZSTD_DISABLE_ASM -DHAVE_CDROM -DUSE_16BPP_RENDERING -DFRONTEND_SUPPORTS_RGB565 -DLSB_FIRST -DBYTE_ORDER=LITTLE_ENDIAN -DHAVE_ZLIB -D__LIBRETRO__ -DUSE_LIBRETRO_VFS -DM68K_OVERCLOCK_SHIFT=20 -DZ80_OVERCLOCK_SHIFT=20 -DHAVE_YM3438_CORE -DHAVE_OPLL_CORE -DMAXROMSIZE=33554432"
cc -fPIC -O2 -w $INC $DEF -DINLINE="static inline" -c "$HERE/o2.c" -o "$HERE/build/o2.o"
OBJS=$(find . -name '*.o' | sort)
cc -shared -o "$HERE/build/liboracle2.so" -Wl,--wrap=cdd_process -Wl,--wrap=cdd_read_data $OBJS "$HERE/build/o2.o" -lm
echo "built $HERE/build/liboracle2.so ($(git rev-parse HEAD))"
