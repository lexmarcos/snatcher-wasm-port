import sys; sys.path.insert(0,'scripts/gfx_i18n')
from lib import *; from data import *
d=Data('DATA_D2')
blob=d.blob(186540)
vram=bytearray(0x10000); vram[0x20:0x20+len(blob)]=blob
tiles=tiles_from_bytes(bytes(vram))
def strip(pieces):
    w=max(x+8*(((s>>2)&3)+1) for y,s,a,x in pieces); h=16
    g=np.zeros((h,w),dtype=np.uint8)
    for y,s,a,x in pieces:
        grid,pw,ph=piece_tiles(s,a)
        for r in range(ph):
            for c in range(pw):
                g[r*8:(r+1)*8, x+c*8:x+c*8+8]=tiles[grid[r][c]]
    return g
anim=239348
f0=d.frame(anim,0); f1=d.frame(anim,1)
newb=strip([p for p in f0 if p[0]==0 and p[3]>0])
contd=strip([p for p in f0 if p[0]==16 ])
contb=strip([p for p in f1 if p[0]==16 and p[3]>0])
newd=strip([p for p in f1 if p[0]==0])
S={'new_b':newb,'cont_d':contd,'cont_b':contb,'new_d':newd}
if __name__=='__main__':
    ch=' 123456789abcdef'
    for k,g in S.items():
        print(k,g.shape)
        for row in g: print(''.join(ch[v] if v else '.' for v in row))
