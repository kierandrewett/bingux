"""Opt-in native hover/click capture proof; saves to a temporary folder and restores options."""
import json, subprocess, time, tempfile
from pathlib import Path
from gi.repository import Gio,GLib
base=['qs','ipc','--any-display','-p',str(Path(__file__).resolve().parents[1] / 'shell/bingux/CaptureShell.qml'),'call','capture']
def ipc(method,*args):
 for attempt in range(30):
  output=subprocess.check_output(base+[method,'--',*args],text=True,timeout=4)
  lines=[line for line in output.splitlines() if line.startswith('{')]
  if lines: return lines[-1]
  if method in ('cancel',): return output
  time.sleep(.1)
 raise RuntimeError(output)
old=json.loads(ipc('options'))
wins=json.loads(subprocess.check_output(['gnoblinctl','windows','--json'],text=True))['windows']
w=next(w for w in wins if w['focused']);r=w['geometry']
bus=Gio.bus_get_sync(Gio.BusType.SESSION,None);dest='org.gnome.Mutter.RemoteDesktop'
def call(path,iface,method,args=None):return bus.call_sync(dest,path,iface,method,args,None,Gio.DBusCallFlags.NONE,3000,None).unpack()
s=call('/org/gnome/Mutter/RemoteDesktop',dest,'CreateSession')[0]
try:
 call(s,dest+'.Session','Start')
 print(ipc('openOptions',json.dumps({'kind':'screenshot','target':'window','copy':False,'cursor':False,'delay':0,'directory':tempfile.mkdtemp(prefix='bingux-window-capture-test-')})))
 time.sleep(1)
 call(s,dest+'.Session','NotifyPointerMotionRelative',GLib.Variant('(dd)',(-10000.,-10000.)))
 call(s,dest+'.Session','NotifyPointerMotionRelative',GLib.Variant('(dd)',(float(r['x']+r['width']/2),float(r['y']+r['height']/2))))
 time.sleep(.2)
 hovered=json.loads(ipc('status'));print('Hovered:',hovered['selectedWindow'])
 assert hovered['selectedWindow'] is not None
 subprocess.run(['grim','/tmp/bingux-window-picker-hover.png'],check=True)
 for down in [True,False]:call(s,dest+'.Session','NotifyPointerButton',GLib.Variant('(ib)',(272,down)))
 time.sleep(.3)
 state=json.loads(ipc('status'));print('After clicking visible window:',{k:state.get(k) for k in ['opened','state','target','selectedWindow']})
 assert not state['opened'],'FAIL: clicking a visible window does not select or capture it'
 deadline=time.monotonic()+10
 while state['state'] not in ('saved','error') and time.monotonic()<deadline:
  time.sleep(.1);state=json.loads(ipc('status'))
 print('Capture result:',state['state'],state['message'],state['savedPath'])
 assert state['state']=='saved', state
 dimensions=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_entries','stream=width,height','-of','json',state['savedPath']],text=True))['streams'][0]
 selected=hovered['selectedWindow']
 assert (dimensions['width'],dimensions['height'])==(selected['bufferWidth'],selected['bufferHeight']), (dimensions,selected)
 print('PASS: native window image matches selected window dimensions',dimensions)
finally:
 ipc('cancel');ipc('configure',json.dumps(old));call(s,dest+'.Session','Stop')
