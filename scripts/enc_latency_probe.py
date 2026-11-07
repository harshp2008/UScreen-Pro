#!/usr/bin/env python3
"""Per-frame encode latency of the daemon's ffmpeg command (capture.rs), fed through a FIFO like the helper does.

usage: enc_latency_probe.py W H FPS interval_ms n [extra ffmpeg output args...]
env:   ENC=h264_vaapi|hevc_vaapi   PIX=nv12|bgr0 (bgr0 = GPU does the colour conversion)   LOGLEVEL=error|verbose
Latency = frame written to the FIFO -> its slice NAL arrives on ffmpeg's stdout.
Run it while the daemon's own encoder is idle; results include FIFO write time."""
import os,sys,time,subprocess,threading,statistics
ENC=os.environ.get("ENC","h264_vaapi"); PIX=os.environ.get("PIX","nv12"); LOGLEVEL=os.environ.get("LOGLEVEL","error"); DUP=float(os.environ.get("DUP_MS","0"))  # >0: re-send each frame after DUP ms to push it through
W,H,FPS,IV,N=int(sys.argv[1]),int(sys.argv[2]),int(sys.argv[3]),float(sys.argv[4]),int(sys.argv[5]); extra=sys.argv[6:]
fifo=os.path.join(os.path.dirname(os.path.abspath(__file__)),"enc_lat.fifo")
if os.path.exists(fifo): os.unlink(fifo)
os.mkfifo(fifo)
cmd=["ffmpeg","-hide_banner","-loglevel",LOGLEVEL,"-vaapi_device","/dev/dri/renderD128","-fflags","nobuffer","-flags","low_delay",
 "-color_primaries","bt709","-color_trc","bt709","-colorspace","bt709","-color_range","tv","-f","rawvideo","-pix_fmt",PIX,"-s",f"{W}x{H}",
 "-framerate",str(FPS),"-i",fifo,"-vf",("format=nv12,hwupload" if PIX=="nv12" else "format=bgr0,hwupload,scale_vaapi=format=nv12"),"-c:v",ENC,"-rc_mode","CQP","-qp","24","-maxrate","30000k","-bf","0","-g",str(FPS),
 "-idr_interval","1","-async_depth","1"]+(["-profile:v","main","-b_depth","1","-sei","0","-aud","0"] if ENC=="hevc_vaapi" else [])+extra+["-f","h264" if ENC=="h264_vaapi" else "hevc","pipe:1"]
p=subprocess.Popen(cmd,stdout=subprocess.PIPE,stdin=subprocess.DEVNULL)
fd=os.open(fifo,os.O_WRONLY)
arr=[]; tw=[]
def reader():
    tail=b""
    while True:
        d=os.read(p.stdout.fileno(),1<<16)
        if not d: break
        t=time.perf_counter(); b=tail+d; i=0
        while True:
            j=b.find(b"\x00\x00\x01",i)
            if j<0 or j+3>=len(b): break
            nb=b[j+3]
            if (ENC=="h264_vaapi" and (nb&0x1f) in (1,5)) or (ENC=="hevc_vaapi" and ((nb>>1)&0x3f) in (0,1,19,20,21)): arr.append(t)
            i=j+3
        tail=b[-3:]
th=threading.Thread(target=reader,daemon=True); th.start()
fsz=(W*H*3//2) if PIX=="nv12" else (W*H*4); frame=bytearray(os.urandom(fsz//64)*64)
t0=time.perf_counter()
for i in range(N):
    target=t0+i*IV/1000
    while time.perf_counter()<target: pass
    frame[i%fsz]^=0xFF
    tw.append(time.perf_counter()); mv=memoryview(frame); o=0
    while o<fsz: o+=os.write(fd,mv[o:])
    if DUP>0:
        time.sleep(DUP/1000); o=0
        while o<fsz: o+=os.write(fd,mv[o:])
time.sleep(1.0); os.close(fd); p.terminate()
n=min(len(arr),len(tw)); step=2 if DUP>0 else 1
n=min(len(arr)//step,len(tw))
lat=sorted((arr[i*step]-tw[i])*1000 for i in range(n))[2:-2] if n>10 else []
print(f"frames in {len(tw)}, packets out {len(arr)} | latency p50 {statistics.median(lat):.1f} ms  p95 {lat[int(len(lat)*.95)]:.1f} ms" if lat else f"no data in={len(tw)} out={len(arr)}")
os.unlink(fifo)
