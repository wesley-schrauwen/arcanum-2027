import struct,sys,glob
exec(open('fields.py').read())
def walk(d,pos,bits=None):
    out=[]
    for blk,base,fl in PC:
        for i,n in enumerate(fl):
            bit=base+i
            if bits is not None and bit not in bits: continue
            t=ftype(n)
            if t=='i32': v=struct.unpack_from('<i',d,pos)[0]; pos+=4
            elif t=='i64':
                if d[pos]==0: v=None; pos+=1
                else: v=struct.unpack_from('<q',d,pos+1)[0]; pos+=9
            elif t=='oid':
                if d[pos]==0: v=None; pos+=1
                else: v=d[pos+1:pos+25].hex(); pos+=25
            elif t=='str':
                if d[pos]==0: v=None; pos+=1
                else:
                    ln,=struct.unpack_from('<I',d,pos+1); v=d[pos+5:pos+5+ln].decode('latin1'); pos+=5+ln
            elif t=='sa':
                f=d[pos]
                if f==0: v=[]; pos+=1
                elif f!=1: raise Exception(f'bad sa flag {f} at {pos} field {n}')
                else:
                    size,cnt,bid=struct.unpack_from('<III',d,pos+1); p=pos+13
                    v=[struct.unpack_from('<i',d,p+4*k)[0] for k in range(cnt)] if size==4 else f'{cnt}x{size}'
                    p+=size*cnt; m,=struct.unpack_from('<I',d,p); pos=p+4+4*m
            out.append((bit,n,t,v))
    return pos,out
if __name__=='__main__':
    for f in sorted(glob.glob('/Users/wesley/sourcetree/arcanum-2027/assets/Arcanum/data/proto/0160*PC.pro')):
        d=open(f,'rb').read()
        try:
            pos,out=walk(d,76)
            g={n:v for b,n,t,v in out}
            print(f.split('/')[-1][:6],'end',pos,'len',len(d),'| name',g['pc_player_name'],'| bg',g['pc_background'],g['pc_background_text'],'| portrait',g['critter_portrait'],'| race/gender',(g['critter_stat_base_idx'] or [None]*28)[27],(g['critter_stat_base_idx'] or [None]*28)[26], '| desc',g['description'],'name id',g['name'])
        except Exception as e: print(f.split('/')[-1][:6],'ERR',e)
    # mpc check
    for f in sorted(glob.glob('players/*.mpc')):
        d=open(f,'rb').read()
        bm=int.from_bytes(d[62:82],'little'); bits={i for i in range(192) if bm>>i&1}
        pos,out=walk(d,82,bits); print(f[-40:-32],'mpc end',pos,'objend',4+struct.unpack_from('<I',d,0)[0])
